use alloc::string::{String, ToString};
use alloc::vec::Vec;
pub use announcement::{
    Announcement, AnnouncementKind, AnnouncementParseError, AnyAnnouncement,
    ANNOUNCEMENT_KIND_LENGTH, ANNOUNCEMENT_MINIMAL_LENGTH, ANNOUNCEMENT_PREFIX,
};
use bitcoin::blockdata::opcodes::all::OP_PUSHBYTES_32;
use bitcoin::blockdata::opcodes::All as Opcodes;
use bitcoin::blockdata::script;
use bitcoin::blockdata::script::Instruction;
use bitcoin::Script;
pub use chroma::{
    ChromaAnnouncement, ChromaInfo, CHROMA_ANNOUNCEMENT_KIND, MAX_CHROMA_ANNOUNCEMENT_SIZE,
    MAX_NAME_SIZE, MAX_SYMBOL_SIZE, MIN_CHROMA_ANNOUNCEMENT_SIZE, MIN_NAME_SIZE, MIN_SYMBOL_SIZE,
};
use core::fmt;
pub use freeze::{FreezeAnnouncement, FreezeAnnouncementParseError, FREEZE_ANNOUNCEMENT_KIND};

pub use issue::{IssueAnnouncement, ISSUE_ANNOUNCEMENT_KIND};

pub use transfer_ownership::{TransferOwnershipAnnouncement, TRANSFER_OWNERSHIP_ANNOUNCEMENT_KIND};

use crate::announcements::announcement::ANNOUNCEMENT_INSTRUCTION_NUMBER;

mod announcement;
mod chroma;
mod freeze;
mod issue;
mod transfer_ownership;

/// Parse the bytes into an [`Announcement`] without specification of the [announcement kind].
///
/// # Returns
///
/// Returns the parsed announcement message or an error if the data is invalid or
/// [announcement kind] is unknown.
///
/// [announcement kind]: AnnouncementKind
pub fn announcement_from_bytes(bytes: &[u8]) -> Result<Announcement, AnnouncementParseError> {
    if bytes.len() < ANNOUNCEMENT_MINIMAL_LENGTH {
        return Err(AnnouncementParseError::ShortLength);
    }

    let prefix = [bytes[0], bytes[1], bytes[2]];
    if prefix != ANNOUNCEMENT_PREFIX {
        return Err(AnnouncementParseError::InvalidPrefix);
    }

    let kind = [bytes[3], bytes[4]];
    let announcement_data = &bytes[ANNOUNCEMENT_MINIMAL_LENGTH..];

    match kind {
        CHROMA_ANNOUNCEMENT_KIND => Ok(Announcement::Chroma(
            ChromaAnnouncement::from_announcement_data_bytes(announcement_data)?,
        )),
        FREEZE_ANNOUNCEMENT_KIND => Ok(Announcement::Freeze(
            FreezeAnnouncement::from_announcement_data_bytes(announcement_data)?,
        )),
        ISSUE_ANNOUNCEMENT_KIND => Ok(Announcement::Issue(
            IssueAnnouncement::from_announcement_data_bytes(announcement_data)?,
        )),
        TRANSFER_OWNERSHIP_ANNOUNCEMENT_KIND => Ok(Announcement::TransferOwnership(
            TransferOwnershipAnnouncement::from_announcement_data_bytes(announcement_data)?,
        )),
        _ => Err(AnnouncementParseError::UnknownAnnouncementKind),
    }
}

/// Parse the Bitcoin script into an [`Announcement`] without specification of the
/// [announcement kind].
///
/// # Returns
///
/// Returns the parsed announcement message or an error if the data is invalid or
/// [announcement kind] is unknown.
///
/// [announcement kind]: AnnouncementKind
pub fn announcement_from_script(script: &Script) -> Result<Announcement, ParseOpReturnError> {
    parse_op_return_script(script, announcement_from_bytes)
}

/// Pull the bytes from [`OP_RETURN`] in Bitcoin [`Script`] and parse it with the provided function.
///
/// # Returns
///
/// Returns the parsed value or an [error] if the script is not [`OP_RETURN`] or the parsing
/// function returns an error.
///
/// [error]: ParseOpReturnError
/// [`OP_RETURN`]: bitcoin::blockdata::opcodes::all::OP_RETURN
pub fn parse_op_return_script<T, ParseError, ParseFn>(
    script: &Script,
    parse_fn: ParseFn,
) -> Result<T, ParseOpReturnError>
where
    ParseError: fmt::Display,
    ParseFn: FnOnce(&[u8]) -> Result<T, ParseError>,
{
    if !script.is_op_return() {
        return Err(ParseOpReturnError::NoOpReturn);
    }

    let instructions = script.instructions().collect::<Result<Vec<_>, _>>()?;

    // OP_PUSHBYTES_32 in instruction is not stored, for some reason
    if instructions.len() != ANNOUNCEMENT_INSTRUCTION_NUMBER - 1 {
        return Err(ParseOpReturnError::InvalidInstructionsNumber(
            instructions.len(),
        ));
    }

    match &instructions[1] {
        Instruction::PushBytes(bytes) => {
            if !is_announcement(bytes.as_bytes()) {
                return Err(ParseOpReturnError::IsNotAnnouncement);
            }
            parse_fn(bytes.as_bytes())
                .map_err(|err| ParseOpReturnError::InvaliOpReturnData(err.to_string()))
        }
        inst => Err(ParseOpReturnError::InvalidInstruction(
            instruction_into_opcode(inst),
        )),
    }
}

/// Error that can occur during the parsing [`OP_RETURN`] Bitcoin [`Script`].
///
/// [`OP_RETURN`]: bitcoin::blockdata::opcodes::all::OP_RETURN
#[derive(Debug)]
pub enum ParseOpReturnError {
    InvalidInstructionsNumber(usize),
    NoOpReturn,
    InvalidInstruction(Opcodes),
    ScriptError(script::Error),
    IsNotAnnouncement,
    InvaliOpReturnData(String),
}

impl fmt::Display for ParseOpReturnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInstructionsNumber(num) => write!(
                f,
                "invalid number of instructions, should be {}, got {}",
                ANNOUNCEMENT_INSTRUCTION_NUMBER, num
            ),
            Self::NoOpReturn => write!(f, "no OP_RETURN in script"),
            Self::InvalidInstruction(opcode) => write!(f, "invalid opcode {}", opcode),
            Self::ScriptError(e) => write!(f, "script error: {}", e),
            Self::IsNotAnnouncement => write!(f, "it is not an announcement"),
            Self::InvaliOpReturnData(e) => write!(f, "invalid announcement: {}", e),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for ParseOpReturnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ScriptError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<script::Error> for ParseOpReturnError {
    fn from(err: script::Error) -> Self {
        Self::ScriptError(err)
    }
}

fn instruction_into_opcode(inst: &Instruction) -> Opcodes {
    match inst {
        Instruction::Op(op) => *op,
        Instruction::PushBytes(_) => OP_PUSHBYTES_32,
    }
}

fn is_announcement(src: &[u8]) -> bool {
    src.len() >= ANNOUNCEMENT_MINIMAL_LENGTH && src[0..3] == ANNOUNCEMENT_PREFIX
}
