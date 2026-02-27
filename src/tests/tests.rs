mod tests {

    use std::path::PathBuf;

    use litesvm::{types::TransactionMetadata, LiteSVM};
    use litesvm_token::{
        spl_token::{self},
        CreateAssociatedTokenAccount, CreateMint, MintTo,
    };

    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_message::Message;
    use solana_native_token::LAMPORTS_PER_SOL;
    use solana_pubkey::Pubkey;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    // ----- Program constants ----- //
    const TOKEN_PROGRAM_ID: Pubkey = spl_token::ID;

    fn program_id() -> Pubkey {
        Pubkey::from(crate::ID)
    }

    fn associated_token_program_id() -> Pubkey {
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
            .parse()
            .unwrap()
    }

    fn system_program_id() -> Pubkey {
        solana_sdk_ids::system_program::ID
    }

    // ----- Shared test state ----- //
    pub struct EscrowAccounts {
        payer: Keypair,
        maker: Pubkey,
        vault: Pubkey,
        escrow: Pubkey,
        mint_a: Pubkey,
        mint_b: Pubkey,
        maker_ata_a: Pubkey,
        maker_ata_b: Pubkey,
    }

    // ----- Instruction discriminators ----- //
    const DISCRIMINATOR_MAKE: u8 = 0;
    const DISCRIMINATOR_TAKE: u8 = 1;
    const DISCRIMINATOR_REFUND: u8 = 2;

    // ----- Helpers ----- //
    fn setup() -> (LiteSVM, Keypair) {
        let mut svm = LiteSVM::new();
        let payer = Keypair::new();

        svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL)
            .expect("Airdrop failed");

        let so_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/deploy/escrow.so");

        let program_data = std::fs::read(so_path).expect("failed to read program .so file");

        svm.add_program(program_id(), &program_data)
            .expect("failed to add program");

        (svm, payer)
    }

    fn derive_escrow_pda(maker: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"escrow", maker.as_ref()], &program_id())
    }

    fn derive_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        spl_associated_token_account::get_associated_token_address(owner, mint)
    }

    fn make_escrow() -> (LiteSVM, EscrowAccounts, TransactionMetadata) {
        let (mut svm, payer) = setup();
        let maker = payer.pubkey();

        // --- Mints --- //
        let mint_a = CreateMint::new(&mut svm, &payer)
            .decimals(6)
            .authority(&payer.pubkey())
            .send()
            .expect("Failed to create mint_a");

        let mint_b = CreateMint::new(&mut svm, &payer)
            .decimals(6)
            .authority(&payer.pubkey())
            .send()
            .expect("Failed to create mint_b");

        // ---  Associated token accounts --- //
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_a)
            .owner(&maker)
            .send()
            .expect("failed to create maker_ata_a");

        let maker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &payer, &mint_b)
            .owner(&maker)
            .send()
            .expect("failed to create maker_ata_b");

        // --- PDA derivation ---//
        let (escrow, bump) = derive_escrow_pda(&maker);
        let vault = derive_ata(&escrow, &mint_a);

        // --- Token supply --- //
        let amount_to_give: u64 = 500_000_000; // 500 mint_a tokens (6 decimals)
        let amount_to_receive: u64 = 100_000_000; // 100 mint_b tokens (6 decimals)

        MintTo::new(&mut svm, &payer, &mint_a, &maker_ata_a, amount_to_give)
            .send()
            .expect("failed to mint tokens to maker_ata_a");

        // --- Make instruction --- //
        let data = [
            vec![DISCRIMINATOR_MAKE],
            bump.to_le_bytes().to_vec(),
            amount_to_receive.to_le_bytes().to_vec(),
            amount_to_give.to_le_bytes().to_vec(),
        ]
        .concat();

        let make_ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(maker, true),
                AccountMeta::new(mint_a, false),
                AccountMeta::new(mint_b, false),
                AccountMeta::new(escrow, false),
                AccountMeta::new(maker_ata_a, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(system_program_id(), false),
                AccountMeta::new(TOKEN_PROGRAM_ID, false),
                AccountMeta::new(associated_token_program_id(), false),
            ],
            data,
        };

        let tx = {
            let message = Message::new(&[make_ix], Some(&maker));
            let blockhash = svm.latest_blockhash();
            Transaction::new(&[&payer], message, blockhash)
        };

        let metadata = svm.send_transaction(tx).expect("make transaction failed");

        let accounts = EscrowAccounts {
            payer,
            maker,
            vault,
            escrow,
            mint_a,
            mint_b,
            maker_ata_a,
            maker_ata_b,
        };

        (svm, accounts, metadata)
    }

    // ----- Tests ----- //
    #[test]
    pub fn test_make() {
        let (svm, accounts, metadata) = make_escrow();

        let EscrowAccounts { vault, .. } = accounts;

        println!("make: ok — {} CUs", metadata.compute_units_consumed);
        // println!("make: ok - logs {:#?}", metadata.logs);

        let vault_bal = svm.get_balance(&vault).unwrap_or(0);
        println!("make: ok - vault balance: {:?}", vault_bal);

        // Add assertions to confirm transaction state changes
    }

    #[test]
    fn test_take() {
        let (mut svm, accounts, _) = make_escrow();

        let EscrowAccounts {
            payer,
            maker,
            vault,
            escrow,
            mint_a,
            mint_b,
            maker_ata_b,
            ..
        } = accounts;

        let amount_to_receive: u64 = 100_000_000;

        // --- Taker setup --- //
        let taker = Keypair::new();
        svm.airdrop(&taker.pubkey(), 10 * LAMPORTS_PER_SOL)
            .expect("airdrop to taker failed");

        let taker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_a)
            .owner(&taker.pubkey())
            .send()
            .expect("failed to create taker_ata_a");

        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_b)
            .owner(&taker.pubkey())
            .send()
            .expect("failed to create taker_ata_b");

        // Fund the taker's mint_b account so they can fulfill the maker's ask.
        MintTo::new(&mut svm, &payer, &mint_b, &taker_ata_b, amount_to_receive)
            .send()
            .expect("failed to mint tokens to taker_ata_b");

        // --- Take instruction --- //
        let take_ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(taker.pubkey(), true),
                AccountMeta::new(maker, false),
                AccountMeta::new(mint_a, false),
                AccountMeta::new(mint_b, false),
                AccountMeta::new(escrow, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(taker_ata_a, false),
                AccountMeta::new(taker_ata_b, false),
                AccountMeta::new(maker_ata_b, false),
                AccountMeta::new(TOKEN_PROGRAM_ID, false),
                AccountMeta::new(system_program_id(), false),
                AccountMeta::new(associated_token_program_id(), false),
            ],
            data: vec![DISCRIMINATOR_TAKE],
        };

        let tx = {
            let message = Message::new(&[take_ix], Some(&taker.pubkey()));
            let blockhash = svm.latest_blockhash();
            Transaction::new(&[&taker], message, blockhash)
        };

        let metadata = svm.send_transaction(tx).expect("take transaction failed");

        println!("take: ok — {} CUs", metadata.compute_units_consumed);
        // println!("take: ok - logs {:#?}", metadata.logs);

        // Check account state changes
        let vault_balance = svm.get_balance(&vault).unwrap_or(0);
        println!("take: ok - vault balance: {:?}", vault_balance);

        let taker_balance = svm.get_balance(&taker_ata_a).unwrap_or(0);
        println!("take: ok - taker balance: {:?}", taker_balance);
    }

    #[test]
    fn test_refund() {
        let (mut svm, accounts, _) = make_escrow();

        let EscrowAccounts {
            payer,
            maker,
            vault,
            escrow,
            mint_a,
            maker_ata_a,
            ..
        } = accounts;

        // --- Refund instruction --- //
        let refund_ix = Instruction {
            program_id: program_id(),
            accounts: vec![
                AccountMeta::new(maker, true),
                AccountMeta::new(mint_a, false),
                AccountMeta::new(escrow, false),
                AccountMeta::new(vault, false),
                AccountMeta::new(maker_ata_a, false),
                AccountMeta::new(TOKEN_PROGRAM_ID, false),
                AccountMeta::new(system_program_id(), false),
                AccountMeta::new(associated_token_program_id(), false),
            ],
            data: vec![DISCRIMINATOR_REFUND],
        };

        let tx = {
            let message = Message::new(&[refund_ix], Some(&maker));
            let blockhash = svm.latest_blockhash();
            Transaction::new(&[&payer], message, blockhash)
        };

        let metadata = svm.send_transaction(tx).expect("refund transaction failed");

        println!("refund: ok - {} CUs", metadata.compute_units_consumed);
        // println!("refund: ok - logs {:#?}", metadata.logs);

        let vault_balance = svm.get_balance(&vault).unwrap_or(0);
        println!("refund: ok - vault balance: {:?}", vault_balance);

        let maker_balance = svm.get_balance(&maker_ata_a).unwrap_or(0);
        println!("refund: ok - maker balance: {:?}", maker_balance);
    }
}
