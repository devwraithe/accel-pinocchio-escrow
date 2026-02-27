use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, ProgramResult,
};
use pinocchio_log::log;
use pinocchio_pubkey::derive_address;
use pinocchio_system::instructions::CreateAccount;

use crate::state::{EscrowV2, MakeArgs};

pub fn process_make_ixn_v2(accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    // Destructure `accounts` &[AccountView] slice
    let [maker, mint_a, mint_b, escrow_account, maker_ata, vault, system_program, token_program, _associated_token_program @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Contain the `maker_ata_state` validation within this scope so Ref is dropped before CPI
    {
        let maker_ata_state = pinocchio_token::state::TokenAccount::from_account_view(&maker_ata)?;

        if maker_ata_state.owner() != maker.address() {
            return Err(ProgramError::IllegalOwner);
        }

        if maker_ata_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    let args =
        wincode::deserialize::<MakeArgs>(data).map_err(|_| ProgramError::InvalidInstructionData)?;

    let bump = args.bump;
    let seed = [b"escrow".as_ref(), maker.address().as_ref(), &[bump]];
    let _seeds = &seed[..];

    let escrow_account_pda = derive_address(&seed, None, &crate::ID.to_bytes());
    assert_eq!(escrow_account_pda, *escrow_account.address().as_array());

    let bump = [bump.to_le()];
    let seed = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(&bump),
    ];
    let seeds = Signer::from(&seed);

    unsafe {
        if escrow_account.owner() != &crate::ID {
            CreateAccount {
                from: maker,
                to: escrow_account,
                lamports: Rent::get()?.try_minimum_balance(EscrowV2::LEN)?,
                space: EscrowV2::LEN as u64,
                owner: &crate::ID,
            }
            .invoke_signed(&[seeds.clone()])?;

            {
                let escrow_state = EscrowV2::from_account_info(escrow_account)?;

                escrow_state.set_maker(maker.address());
                escrow_state.set_mint_a(mint_a.address());
                escrow_state.set_mint_b(mint_b.address());
                escrow_state.set_amount_to_receive(args.amount_to_receive);
                escrow_state.set_amount_to_give(args.amount_to_give);
                escrow_state.bump = data[0];
            }
        } else {
            return Err(ProgramError::IllegalOwner);
        }
    }

    pinocchio_associated_token_account::instructions::Create {
        funding_account: maker,
        account: vault,
        wallet: escrow_account,
        mint: mint_a,
        token_program: token_program,
        system_program: system_program,
    }
    .invoke()?;

    pinocchio_token::instructions::Transfer {
        from: maker_ata,
        to: vault,
        authority: maker,
        amount: args.amount_to_give,
    }
    .invoke()?;

    log!("Vault holds {}", args.amount_to_give);

    Ok(())
}
