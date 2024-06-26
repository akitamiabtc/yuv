use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt;
use core::mem::size_of;

use crate::{network::Network, Announcement, AnyAnnouncement};
use bitcoin::hashes::Hash;
use bitcoin::{OutPoint, Txid};

use crate::announcements::{AnnouncementKind, AnnouncementParseError};

/// The two bytes that represent the [`freeze announcement`]'s kind.
///
/// [`freeze announcement`]: FreezeAnnouncement
pub const FREEZE_ANNOUNCEMENT_KIND: AnnouncementKind = [0, 1];
/// Size of txid in bytes.
const TX_ID_SIZE: usize = size_of::<Txid>();
/// Size of freeze entry in bytes.
pub const FREEZE_ENTRY_SIZE: usize = TX_ID_SIZE + size_of::<u32>();

/// Freeze announcement. It appears when issuer declares that tx is frozen or unfrozen.
///
/// # Structure
///
/// - `txid` - 32 bytes [`Txid`] of the transaction that is frozen or unfrozen.
/// - `vout` - 4 bytes u32 number of the transaction's output that is frozen or unfrozen.
///
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FreezeAnnouncement {
    /// The outpoint of the transaction that is frozen or unfrozen.
    pub outpoint: OutPoint,
}

impl FreezeAnnouncement {
    /// Create a new freeze announcement.
    pub fn new(outpoint: OutPoint) -> Self {
        Self { outpoint }
    }

    /// Return the transaction id of the frozen or unfrozen transaction.
    pub fn freeze_txid(&self) -> Txid {
        self.outpoint.txid
    }

    /// Return the vout of the frozen or unfrozen transaction.
    pub fn freeze_vout(&self) -> u32 {
        self.outpoint.vout
    }

    /// Return the outpoint of the frozen or unfrozen transaction.
    pub fn freeze_outpoint(&self) -> OutPoint {
        self.outpoint
    }
}

#[cfg_attr(feature = "serde", typetag::serde(name = "freeze_announcement"))]
impl AnyAnnouncement for FreezeAnnouncement {
    fn kind(&self) -> AnnouncementKind {
        FREEZE_ANNOUNCEMENT_KIND
    }

    fn minimal_block_height(&self, _network: Network) -> usize {
        // For the default, innitial announcements, there is no minimal block height.
        0
    }

    fn from_announcement_data_bytes(data: &[u8]) -> Result<Self, AnnouncementParseError> {
        if data.len() != FREEZE_ENTRY_SIZE {
            return Err(FreezeAnnouncementParseError::InvalidSize(data.len()))?;
        }

        let txid = Txid::from_slice(&data[..TX_ID_SIZE])
            .map_err(FreezeAnnouncementParseError::InvalidTxHash)?;
        let vout = u32::from_be_bytes(data[TX_ID_SIZE..].try_into().expect("Size is checked"));

        let outpoint = OutPoint::new(txid, vout);

        Ok(Self { outpoint })
    }

    fn to_announcement_data_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FREEZE_ENTRY_SIZE);

        bytes.extend_from_slice(&self.outpoint.txid[..]);
        bytes.extend_from_slice(&self.outpoint.vout.to_be_bytes());

        bytes
    }
}

impl From<FreezeAnnouncement> for Announcement {
    fn from(freeze_announcement: FreezeAnnouncement) -> Self {
        Self::Freeze(freeze_announcement)
    }
}

impl From<OutPoint> for FreezeAnnouncement {
    fn from(outpoint: OutPoint) -> Self {
        Self { outpoint }
    }
}

impl From<FreezeAnnouncement> for OutPoint {
    fn from(freeze_announcement: FreezeAnnouncement) -> Self {
        freeze_announcement.outpoint
    }
}

/// Errors that can occur when parsing [freeze announcement].
///
/// [freeze announcement]: FreezeAnnouncement
#[derive(Debug)]
pub enum FreezeAnnouncementParseError {
    InvalidSize(usize),
    InvalidTxHash(bitcoin::hashes::Error),
}

impl fmt::Display for FreezeAnnouncementParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreezeAnnouncementParseError::InvalidSize(size) => write!(
                f,
                "invalid bytes size should be {}, got {}",
                FREEZE_ENTRY_SIZE, size
            ),
            FreezeAnnouncementParseError::InvalidTxHash(e) => write!(f, "invalid tx hash: {}", e),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for FreezeAnnouncementParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidTxHash(e) => Some(e),
            _ => None,
        }
    }
}

impl From<bitcoin::hashes::Error> for FreezeAnnouncementParseError {
    fn from(err: bitcoin::hashes::Error) -> Self {
        Self::InvalidTxHash(err)
    }
}

impl From<FreezeAnnouncementParseError> for AnnouncementParseError {
    fn from(err: FreezeAnnouncementParseError) -> Self {
        AnnouncementParseError::InvalidAnnouncementData(err.to_string())
    }
}

#[cfg(test)]
mod test {
    use crate::announcements::freeze::FREEZE_ENTRY_SIZE;
    use crate::announcements::{
        announcement_from_bytes, announcement_from_script, AnnouncementParseError,
        FreezeAnnouncement,
    };
    use crate::{Announcement, AnyAnnouncement};
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use alloc::{format, vec};
    use bitcoin::{OutPoint, ScriptBuf, Txid};
    use core::str::FromStr;

    pub const TEST_TXID: &str = "abc0000000000000000000000000000000000000000000000000000000000abc";

    #[test]
    fn test_serialize_desirialize() {
        let outpoint = OutPoint {
            txid: Txid::from_str(TEST_TXID).unwrap(),
            vout: 34,
        };

        let announcement = FreezeAnnouncement { outpoint };

        let data_bytes = announcement.to_announcement_data_bytes();
        let parsed_announcement =
            FreezeAnnouncement::from_announcement_data_bytes(&data_bytes).unwrap();
        assert_eq!(announcement, parsed_announcement);
        assert_eq!(parsed_announcement.freeze_outpoint(), outpoint);

        let announcement_script = announcement.to_script();
        let parsed_announcement = FreezeAnnouncement::from_script(&announcement_script).unwrap();
        assert_eq!(announcement, parsed_announcement);
        assert_eq!(parsed_announcement.freeze_outpoint(), outpoint);

        let parsed_announcement = announcement_from_script(&announcement_script).unwrap();
        assert_eq!(Announcement::Freeze(announcement), parsed_announcement);
    }

    #[test]
    fn parse_invalid_bytes() {
        struct TestData {
            bytes: Vec<u8>,
            err: String,
        }

        let test_vector = vec![
            TestData {
                bytes: vec![0],
                err: format!("invalid bytes size should be {}, got 1", FREEZE_ENTRY_SIZE)
                    .to_string(),
            },
            TestData {
                bytes: vec![0; 37],
                err: format!("invalid bytes size should be {}, got 37", FREEZE_ENTRY_SIZE)
                    .to_string(),
            },
        ];

        for test in test_vector {
            match FreezeAnnouncement::from_announcement_data_bytes(&test.bytes) {
                Err(AnnouncementParseError::InvalidAnnouncementData(err)) => {
                    assert_eq!(err, test.err);
                }
                err => {
                    panic!("Unexpected result: {:?}", err);
                }
            }
        }
    }

    #[test]
    fn test_backward_compatibility() {
        let valid_announcement_bytes = vec![
            121, 117, 118, 0, 1, 188, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 171, 0, 0, 0, 34,
        ];

        let valid_announcement_data = vec![
            188, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 192, 171, 0, 0, 0, 34,
        ];

        let valid_announcement_script = ScriptBuf::from_hex("6a297975760001bc0a00000000000000000000000000000000000000000000000000000000c0ab00000022").unwrap();

        assert!(announcement_from_script(&valid_announcement_script).is_ok());
        assert!(announcement_from_bytes(&valid_announcement_bytes).is_ok());
        assert!(FreezeAnnouncement::from_bytes(&valid_announcement_bytes).is_ok());
        assert!(FreezeAnnouncement::from_announcement_data_bytes(&valid_announcement_data).is_ok());
        assert!(FreezeAnnouncement::from_script(&valid_announcement_script).is_ok());
    }
}
