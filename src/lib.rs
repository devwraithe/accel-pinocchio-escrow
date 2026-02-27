#![allow(unexpected_cfgs)]
use pinocchio::{
    address::declare_id, entrypoint, error::ProgramError, nostd_panic_handler, AccountView,
    Address, ProgramResult,
};

use crate::instructions::EscrowInstructions;

mod instructions;
mod state;
mod tests;

entrypoint!(process_instruction);

declare_id!("7XnntXKF78m6UiSjHZ4aB6vkPakCnS8VcN1688J6LJQV");

pub fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    assert_eq!(program_id, &ID);

    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    match EscrowInstructions::try_from(discriminator)? {
        EscrowInstructions::Make => instructions::process_make_instruction(accounts, data)?,
        EscrowInstructions::Take => instructions::process_take_instruction(accounts, data)?,
        // EscrowInstrctions::MakeV2 => instructions::process_make_instruction_v2(accounts, data)?,
        _ => return Err(ProgramError::InvalidInstructionData),
    }
    Ok(())
}
