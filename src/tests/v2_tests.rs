mod v2_tests {

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

    use crate::state::MakeArgs;

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
    const DISCRIMINATOR_MAKE_V2: u8 = 3;
    const _DISCRIMINATOR_TAKE_V2: u8 = 4;
    const _DISCRIMINATOR_REFUND_V2: u8 = 5;

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
        let args = MakeArgs {
            bump,
            amount_to_receive,
            amount_to_give,
        };

        let mut data = wincode::serialize(&args).expect("failed to serialize MakeArgs");
        data.insert(0, DISCRIMINATOR_MAKE_V2);

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
    pub fn test_make_v2() {
        let (svm, accounts, metadata) = make_escrow();

        let EscrowAccounts { vault, .. } = accounts;

        println!("make: ok — {} CUs", metadata.compute_units_consumed);
        println!("make: ok - logs {:#?}", metadata.logs);

        let vault_bal = svm.get_balance(&vault).unwrap_or(0);
        println!("make: ok - vault balance: {:?}", vault_bal);

        // Add assertions to confirm transaction state changes
    }
}
