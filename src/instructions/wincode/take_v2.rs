use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, ProgramResult,
};

use crate::state::EscrowV2;

pub fn process_take_v2_ixn(accounts: &[AccountView]) -> ProgramResult {
    let [taker, maker, mint_a, mint_b, escrow_account, vault, taker_ata_a, taker_ata_b, maker_ata_b, _token_program, _system_program, _associated_token_program @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Signer check
    if !taker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Deserialize state
    let escrow_state = EscrowV2::from_account_info(escrow_account)?;

    {
        let maker_ata_b_state =
            pinocchio_token::state::TokenAccount::from_account_view(maker_ata_b)?;
        let taker_ata_a_state =
            pinocchio_token::state::TokenAccount::from_account_view(taker_ata_a)?;
        let taker_ata_b_state =
            pinocchio_token::state::TokenAccount::from_account_view(taker_ata_b)?;

        // Confirm the taker owns their token accounts.
        if taker_ata_b_state.owner() != taker.address() {
            return Err(ProgramError::IllegalOwner);
        }

        // Confirm each ATA is for the expected mint.
        if taker_ata_a_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
        if taker_ata_b_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
        if maker_ata_b_state.mint() != mint_b.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    // CPI signer seeds (to sign as vault authority)
    let bump = [escrow_state.bump];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(&bump),
    ];
    let signer = Signer::from(&signer_seeds);

    // Taker sends mint_b to the maker (fulfilling the maker's ask).
    pinocchio_token::instructions::Transfer {
        from: taker_ata_b,
        to: maker_ata_b,
        authority: taker,
        amount: escrow_state.amount_to_receive(),
    }
    .invoke()?;

    // Escrow vault sends mint_a to the taker (releasing the maker's deposit).
    pinocchio_token::instructions::Transfer {
        from: vault,
        to: taker_ata_a,
        authority: escrow_account,
        amount: escrow_state.amount_to_give(),
    }
    .invoke_signed(&[signer.clone()])?;

    // Close vault token account, return rent to maker
    pinocchio_token::instructions::CloseAccount {
        account: vault,
        destination: maker,
        authority: escrow_account,
    }
    .invoke_signed(&[signer])?;

    // Return escrow lamports to maker then close account
    let escrow_lamports = escrow_account.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow_account.set_lamports(0);
    escrow_account.close()?;

    Ok(())
}
