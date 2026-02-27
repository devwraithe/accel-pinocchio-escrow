pub mod make;
pub mod refund;
pub mod take;
pub mod wincode;

pub use make::*;
pub use refund::*;
pub use take::*;

pub use wincode::make_v2::*;

use pinocchio::error::ProgramError;

pub enum EscrowInstructions {
    Make = 0,
    Take = 1,
    Refund = 2, // or Cancel
    MakeV2 = 3,
}

impl TryFrom<&u8> for EscrowInstructions {
    type Error = ProgramError;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EscrowInstructions::Make),
            1 => Ok(EscrowInstructions::Take),
            2 => Ok(EscrowInstructions::Refund),
            3 => Ok(EscrowInstructions::MakeV2),
            _ => Err(ProgramError::InvalidInstructionData),
        }
    }
}
