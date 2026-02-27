use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, ProgramResult,
};

use crate::state::Escrow;

pub fn process_refund_ixn(accounts: &[AccountView]) -> ProgramResult {
    let [maker, mint_a, escrow, vault, maker_ata_a, _token_program, _system_program, _associated_token_program @ ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Signer check
    if !maker.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Deserialize state
    let escrow_state = Escrow::from_account_info(escrow)?;

    {
        let maker_ata_a_state =
            pinocchio_token::state::TokenAccount::from_account_view(&maker_ata_a)?;

        if maker_ata_a_state.owner() != maker.address() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        if maker_ata_a_state.mint() != mint_a.address() {
            return Err(ProgramError::InvalidAccountData);
        }
    }

    // CPI signer seeds (to sign as vault authority)
    let bump = [escrow_state.bump.to_le()];
    let signer_seeds = [
        Seed::from(b"escrow"),
        Seed::from(maker.address().as_array()),
        Seed::from(&bump),
    ];
    let signer = Signer::from(&signer_seeds);

    // Escrow vault sends mint_a back to to the maker (triggering a refund)
    pinocchio_token::instructions::Transfer {
        from: vault,
        to: maker_ata_a,
        authority: escrow,
        amount: escrow_state.amount_to_give(),
    }
    .invoke_signed(&[signer.clone()])?;

    // Close vault token account, return rent to maker
    pinocchio_token::instructions::CloseAccount {
        account: vault,
        destination: maker,
        authority: escrow,
    }
    .invoke_signed(&[signer])?;

    // Return escrow lamports to maker then close account
    let escrow_lamports = escrow.lamports();
    maker.set_lamports(maker.lamports() + escrow_lamports);
    escrow.set_lamports(0);
    escrow.close()?;

    Ok(())
}
