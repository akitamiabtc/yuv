use crate::{network::Network, Announcement, AnyAnnouncement};
use alloc::string::{FromUtf8Error, String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use bitcoin::consensus::{encode, ReadExt};
use bitcoin::ScriptBuf;
use core::fmt;
use core::mem::size_of;
use core2::io;
use core2::io::{Cursor, Read};
use yuv_pixels::{Chroma, ChromaParseError, CHROMA_SIZE};

use crate::announcements::{AnnouncementKind, AnnouncementParseError};

/// Two bytes that represent the [`ChromaAnnouncement`]'s kind.
pub const CHROMA_ANNOUNCEMENT_KIND: AnnouncementKind = [0, 0];
/// The maximum size of the name in [`ChromaAnnouncement`] in bytes.
pub const MAX_NAME_SIZE: usize = 20;
/// The minimum size of the name in [`ChromaAnnouncement`] in bytes.
pub const MIN_NAME_SIZE: usize = 3;
/// The maximum size of the symbol in [`ChromaAnnouncement`] in bytes.
pub const MAX_SYMBOL_SIZE: usize = 6;
/// The minimum size of the symbol in [`ChromaAnnouncement`] in bytes.
pub const MIN_SYMBOL_SIZE: usize = 3;
/// The minimum size of the [`ChromaAnnouncement`] in bytes.
pub const MIN_CHROMA_ANNOUNCEMENT_SIZE: usize =
    CHROMA_SIZE + 1 + MIN_NAME_SIZE + 1 + MIN_SYMBOL_SIZE + 1 + 16 + 1;
/// The maxim size of the [`ChromaAnnouncement`] in bytes.
pub const MAX_CHROMA_ANNOUNCEMENT_SIZE: usize =
    CHROMA_SIZE + 1 + MAX_NAME_SIZE + 1 + MAX_SYMBOL_SIZE + 1 + 16 + 1;

/// Chroma's initial announcement from the issuer. It contains the information about the token and
/// issuer.
///
/// # Structure
///
/// - `chroma` - 32 bytes [`Chroma`].
/// - `name` - 1 + [3 - 20] bytes name of the token. Where the first byte is the length of the name.
/// - `symbol` - 1 + [3 - 6] bytes symbol of the token. Where the first byte is the length of the
/// symbol.
/// - `decimal` - 1 byte number of decimal places for the token (u8).
/// - `max_supply` - 16 bytes maximum supply of the token (u128).
/// - `is_freezable` - 1 byte indicates whether the token can be freezed or not by the issuer (bool).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChromaAnnouncement {
    /// The token's [`Chroma`].
    pub chroma: Chroma,
    /// The name of the token. e.g. Bitcoin.
    pub name: String,
    /// The symbol (i.e. the short name) of the token. e.g. `BTC` for Bitcoin. The maximum size is
    /// [`MAX_SYMBOL_SIZE`], the minimum is [`MIN_SYMBOL_SIZE`].
    pub symbol: String,
    /// The number of decimal places for the token. e.g. 8 for Bitcoin.
    pub decimal: u8,
    /// The maximum supply of the token. e.g. 21_000_000 for Bitcoin.
    pub max_supply: u128,
    /// Indicates whether the token can be freezed or not by the issuer.
    pub is_freezable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChromaInfo {
    pub announcement: Option<ChromaAnnouncement>,
    pub total_supply: u128,
    pub owner: Option<ScriptBuf>,
}

impl ChromaAnnouncement {
    /// Create a new [`ChromaAnnouncement`].
    pub fn new(
        chroma: Chroma,
        name: String,
        symbol: String,
        decimal: u8,
        max_supply: u128,
        is_freezable: bool,
    ) -> Result<Self, ChromaAnnouncementParseError> {
        if name.len() < MIN_NAME_SIZE || name.len() > MAX_NAME_SIZE {
            return Err(ChromaAnnouncementParseError::InvalidNameLength);
        }

        if symbol.len() < MIN_SYMBOL_SIZE || symbol.len() > MAX_SYMBOL_SIZE {
            return Err(ChromaAnnouncementParseError::InvalidSymbolLength);
        }

        let result = Self {
            chroma,
            name,
            symbol,
            decimal,
            max_supply,
            is_freezable,
        };

        Ok(result)
    }
}

#[cfg_attr(feature = "serde", typetag::serde(name = "chroma_announcement"))]
impl AnyAnnouncement for ChromaAnnouncement {
    fn kind(&self) -> AnnouncementKind {
        CHROMA_ANNOUNCEMENT_KIND
    }

    fn minimal_block_height(&self, _network: Network) -> usize {
        // For the default, innitial announcements, there is no minimal block height.
        0
    }

    fn from_announcement_data_bytes(data: &[u8]) -> Result<Self, AnnouncementParseError> {
        if data.len() < MIN_CHROMA_ANNOUNCEMENT_SIZE {
            Err(ChromaAnnouncementParseError::ShortLength)?;
        }

        let mut cursor = Cursor::new(data);

        // Read the chroma
        let mut chroma_bytes = [0u8; CHROMA_SIZE];
        cursor
            .read_exact(&mut chroma_bytes)
            .map_err(|err| wrap_io_error(err, "failed to read the chroma"))?;

        let chroma =
            Chroma::from_bytes(&chroma_bytes).map_err(ChromaAnnouncementParseError::from)?;

        // Read the name
        let name_len = cursor
            .read_u8()
            .map_err(|err| wrap_io_error(err, "failed to read the name length"))?
            as usize;

        if !(MIN_NAME_SIZE..=MAX_NAME_SIZE).contains(&name_len) {
            Err(ChromaAnnouncementParseError::InvalidNameLength)?;
        }

        let mut name_bytes = vec![0; name_len];
        cursor
            .read_exact(&mut name_bytes)
            .map_err(|err| wrap_io_error(err, "failed to read the name"))?;

        let name = String::from_utf8(name_bytes)
            .map_err(ChromaAnnouncementParseError::InvalidUtf8String)?;

        // Read the symbol
        let symbol_len = cursor
            .read_u8()
            .map_err(|err| wrap_io_error(err, "failed to read the symbol length"))?
            as usize;

        if !(MIN_SYMBOL_SIZE..=MAX_SYMBOL_SIZE).contains(&symbol_len) {
            Err(ChromaAnnouncementParseError::InvalidSymbolLength)?;
        }

        let mut symbol_bytes = vec![0; symbol_len];
        cursor
            .read_exact(&mut symbol_bytes)
            .map_err(|err| wrap_io_error(err, "failed to read the symbol"))?;

        let symbol = String::from_utf8(symbol_bytes)
            .map_err(ChromaAnnouncementParseError::InvalidUtf8String)?;

        // Read the decimal
        let decimal = cursor
            .read_u8()
            .map_err(|err| wrap_io_error(err, "failed to read the decimal"))?;

        // Read the max_supply
        let mut max_supply_bytes = vec![0; size_of::<u128>()];
        cursor
            .read_exact(&mut max_supply_bytes)
            .map_err(|err| wrap_io_error(err, "failed to read the max supply"))?;
        let max_supply = u128::from_le_bytes(max_supply_bytes.try_into().unwrap());

        // Read the is_freezable
        let is_freezable = cursor
            .read_u8()
            .map_err(|err| wrap_io_error(err, "failed to read is freezable"))?;

        let announcement = ChromaAnnouncement {
            chroma,
            name,
            symbol,
            decimal,
            max_supply,
            is_freezable: is_freezable != 0,
        };

        Ok(announcement)
    }

    fn to_announcement_data_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(MAX_CHROMA_ANNOUNCEMENT_SIZE);

        result.extend_from_slice(&self.chroma.to_bytes());
        result.push(self.name.len() as u8);
        result.extend_from_slice(self.name.as_bytes());
        result.push(self.symbol.len() as u8);
        result.extend_from_slice(self.symbol.as_bytes());
        result.push(self.decimal);
        result.extend_from_slice(&self.max_supply.to_le_bytes());
        result.push(if self.is_freezable { 1 } else { 0 });

        result
    }
}

impl From<ChromaAnnouncement> for Announcement {
    fn from(value: ChromaAnnouncement) -> Self {
        Self::Chroma(value)
    }
}

/// Error parsing the [`ChromaAnnouncement`].
#[derive(Debug)]
pub enum ChromaAnnouncementParseError {
    /// Short length of the announcement data. It should be at least
    /// [`MIN_CHROMA_ANNOUNCEMENT_SIZE`].
    ShortLength,
    /// Announcement data is invalid or incorectly encoded.
    InvalidAnnouncementData(String),
    /// The string is not a valid UTF-8 string.
    InvalidUtf8String(FromUtf8Error),
    /// The length of the symbol is less than [`MIN_SYMBOL_SIZE`] or more than [`MAX_SYMBOL_SIZE`].
    InvalidSymbolLength,
    /// The length of the name is less than [`MIN_NAME_SIZE`] or more than [`MAX_NAME_SIZE`].
    InvalidNameLength,
    /// Invalid chroma.
    InvalidChroma(ChromaParseError),
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for ChromaAnnouncementParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidUtf8String(e) => Some(e),
            Self::InvalidChroma(e) => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for ChromaAnnouncementParseError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortLength => write!(
                _f,
                "the announcement data is too short, it must be at least {} bytes",
                MIN_CHROMA_ANNOUNCEMENT_SIZE
            ),
            Self::InvalidAnnouncementData(e) => write!(_f, "invalid announcement data: {}", e),
            Self::InvalidUtf8String(e) => write!(_f, "invalid utf-8 string: {}", e),
            Self::InvalidSymbolLength => write!(
                _f,
                "the length of the symbol is invalid, it must be between {} and {}",
                MIN_SYMBOL_SIZE, MAX_SYMBOL_SIZE
            ),
            Self::InvalidChroma(e) => write!(_f, "invalid chroma: {}", e),
            Self::InvalidNameLength => write!(
                _f,
                "the length of the name is invalid, it must be between {} and {}",
                MIN_NAME_SIZE, MAX_NAME_SIZE
            ),
        }
    }
}

impl From<FromUtf8Error> for ChromaAnnouncementParseError {
    fn from(err: FromUtf8Error) -> Self {
        Self::InvalidUtf8String(err)
    }
}

impl From<encode::Error> for ChromaAnnouncementParseError {
    fn from(err: encode::Error) -> Self {
        Self::InvalidAnnouncementData(err.to_string())
    }
}

impl From<io::Error> for ChromaAnnouncementParseError {
    fn from(err: io::Error) -> Self {
        Self::InvalidAnnouncementData(err.to_string())
    }
}

impl From<ChromaParseError> for ChromaAnnouncementParseError {
    fn from(err: ChromaParseError) -> Self {
        Self::InvalidChroma(err)
    }
}

impl From<ChromaAnnouncementParseError> for AnnouncementParseError {
    fn from(err: ChromaAnnouncementParseError) -> Self {
        AnnouncementParseError::InvalidAnnouncementData(err.to_string())
    }
}

/// Wrap Error with InvalidAnnouncementData and the given message.
fn wrap_io_error(err: impl fmt::Display, message: &str) -> ChromaAnnouncementParseError {
    ChromaAnnouncementParseError::InvalidAnnouncementData(format!("{}: {}", message, err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::announcements::{announcement_from_bytes, announcement_from_script};
    use alloc::string::ToString;
    use bitcoin::ScriptBuf;

    pub const TEST_CHROMA: &str =
        "bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30";

    #[test]
    fn test_serialize_desirialize() {
        struct TestData {
            announcement: ChromaAnnouncement,
            expect_error: bool,
        }

        let test_vector = vec![
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "TokenName".to_string(),
                    symbol: "TNK".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: true,
                },
                expect_error: false,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "TokenName20Character".to_string(),
                    symbol: "TESTSY".to_string(),
                    decimal: 255,
                    max_supply: 18_446_744_073_709_551_615,
                    is_freezable: true,
                },
                expect_error: false,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "TokenName".to_string(),
                    symbol: "TNK".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: false,
                },
                expect_error: false,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "The String Longer Than MAX_NAME_SIZE".to_string(),
                    symbol: "TNK".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: true,
                },
                expect_error: true,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "TokenName".to_string(),
                    symbol: "The String Longer Than MAX_SYMBOL_SIZE".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: true,
                },
                expect_error: true,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "".to_string(),
                    symbol: "TNK".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: true,
                },
                expect_error: true,
            },
            TestData {
                announcement: ChromaAnnouncement {
                    chroma: Chroma::from_address(TEST_CHROMA).expect("valid chroma"),
                    name: "TokenName".to_string(),
                    symbol: "".to_string(),
                    decimal: 2,
                    max_supply: 1_000_000,
                    is_freezable: true,
                },
                expect_error: true,
            },
        ];

        for test in test_vector {
            let data = test.announcement.to_announcement_data_bytes();
            match ChromaAnnouncement::from_announcement_data_bytes(&data) {
                Ok(announcement) => {
                    assert_eq!(announcement, test.announcement);
                }
                Err(err) => {
                    assert!(test.expect_error, "Unexpected error: {}", err);
                }
            }

            let bytes = test.announcement.to_bytes();
            match ChromaAnnouncement::from_bytes(&bytes) {
                Ok(announcement) => {
                    assert_eq!(announcement, test.announcement);
                    assert_eq!(Announcement::Chroma(announcement).to_bytes(), bytes);
                }
                Err(err) => {
                    assert!(test.expect_error, "Unexpected error: {}", err);
                }
            }

            let announcement_script = test.announcement.to_script();
            match ChromaAnnouncement::from_script(&announcement_script) {
                Ok(announcement) => {
                    assert_eq!(announcement, test.announcement);
                }
                Err(err) => {
                    assert!(test.expect_error, "Unexpected error: {}", err);
                }
            }

            match announcement_from_script(&announcement_script) {
                Ok(announcement) => {
                    assert_eq!(announcement, Announcement::Chroma(test.announcement));
                    assert_eq!(announcement.to_script(), announcement_script);
                }
                Err(err) => {
                    assert!(test.expect_error, "Unexpected error: {}", err);
                }
            }
        }
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
                err: format!(
                    "the announcement data is too short, it must be at least {} bytes",
                    MIN_CHROMA_ANNOUNCEMENT_SIZE
                )
                .to_string(),
            },
            TestData {
                bytes: vec![0; 58],
                err: "invalid chroma: Invalid x only public key structure: malformed public key"
                    .to_string(),
            },
        ];

        for test in test_vector {
            match ChromaAnnouncement::from_announcement_data_bytes(&test.bytes) {
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
        struct TestData {
            bytes: Vec<u8>,
            data_bytes: Vec<u8>,
            script: ScriptBuf,
        }

        let valid_announcements = vec![
            TestData {
                bytes: vec![121, 117, 118, 0, 0, 113, 128, 188, 163, 232, 82, 234, 249, 5, 150, 157, 37, 7, 70, 36, 152, 160, 25, 195, 239, 213, 68, 75, 114, 164, 41, 27, 114, 180, 221, 38, 204, 7, 72, 114, 121, 118, 110, 121, 97, 3, 85, 65, 72, 0, 48, 117, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                data_bytes: vec![113, 128, 188, 163, 232, 82, 234, 249, 5, 150, 157, 37, 7, 70, 36, 152, 160, 25, 195, 239, 213, 68, 75, 114, 164, 41, 27, 114, 180, 221, 38, 204, 7, 72, 114, 121, 118, 110, 121, 97, 3, 85, 65, 72, 0, 48, 117, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                script: ScriptBuf::from_hex("6a4379757600007180bca3e852eaf905969d2507462498a019c3efd5444b72a4291b72b4dd26cc07487279766e796103554148003075000000000000000000000000000001").unwrap(),
            },
            TestData {
                bytes: vec![121, 117, 118, 0, 0, 197, 21, 190, 150, 71, 80, 78, 148, 191, 220, 32, 196, 98, 152, 67, 216, 14, 226, 119, 119, 176, 101, 194, 175, 121, 250, 151, 204, 14, 255, 74, 35, 7, 72, 114, 121, 118, 110, 121, 97, 3, 85, 65, 72, 5, 255, 255, 255, 255, 159, 54, 244, 0, 217, 70, 218, 213, 16, 238, 133, 7, 1],
                data_bytes: vec![197, 21, 190, 150, 71, 80, 78, 148, 191, 220, 32, 196, 98, 152, 67, 216, 14, 226, 119, 119, 176, 101, 194, 175, 121, 250, 151, 204, 14, 255, 74, 35, 7, 72, 114, 121, 118, 110, 121, 97, 3, 85, 65, 72, 5, 255, 255, 255, 255, 159, 54, 244, 0, 217, 70, 218, 213, 16, 238, 133, 7, 1],
                script: ScriptBuf::from_hex("6a437975760000c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a2307487279766e79610355414805ffffffff9f36f400d946dad510ee850701").unwrap(),
            },
        ];

        for announcement in valid_announcements {
            assert!(announcement_from_script(&announcement.script).is_ok());
            assert!(announcement_from_bytes(&announcement.bytes).is_ok());
            assert!(ChromaAnnouncement::from_bytes(&announcement.bytes).is_ok());
            assert!(
                ChromaAnnouncement::from_announcement_data_bytes(&announcement.data_bytes).is_ok()
            );
            assert!(ChromaAnnouncement::from_script(&announcement.script).is_ok());
        }
    }
}
