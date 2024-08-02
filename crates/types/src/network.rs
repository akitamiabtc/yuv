use bitcoin::network::Magic;
use core::{fmt::Display, str::FromStr};

use alloc::string::String;
use bitcoin::{BlockHash, Network as BitcoinNetwork};

/// Mutiny network magic.
pub const MUTINY_MAGIC: [u8; 4] = [0xCB, 0x2D, 0xDF, 0xA5];
/// YUV genesis block for `Mainnet`.
const MAINNET_GENESIS_BLOCK: &str =
    "00000000000000000000cde86faf8ea6994e4ca31ed351e55912f617f5dd8ee8";
/// YUV genesis block for `Testnet`.
const TESTNET_GENESIS_BLOCK: &str =
    "000000008ce763d0e9906fc5b50acdd7c8ddc5b1413b1b526f386500628a505c";
/// YUV genesis block for `Mutiny`.
const MUTINY_GENESIS_BLOCK: &str =
    "000002d06087e074a71f1e8e805dcb3264fa9ff4700250ba3f0e95ab05e61afa";

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
/// Default Bitcoin network types.
pub enum Network {
    Bitcoin,
    Testnet,
    Signet,
    Regtest,

    // Custom Bitcoin network types:
    Mutiny,
}

impl Network {
    pub fn to_bitcoin_network(&self) -> BitcoinNetwork {
        match self {
            Network::Bitcoin => BitcoinNetwork::Bitcoin,
            Network::Testnet => BitcoinNetwork::Testnet,
            Network::Signet => BitcoinNetwork::Signet,
            Network::Regtest => BitcoinNetwork::Regtest,
            _ => BitcoinNetwork::Testnet,
        }
    }

    pub fn magic(&self) -> Magic {
        // Mutiny network has custom network magic.
        if let Network::Mutiny = self {
            Magic::from_bytes(MUTINY_MAGIC)
        } else {
            self.to_bitcoin_network().magic()
        }
    }

    /// Returns the block that contains the very first YUV transaction for the given network.
    /// Note: indexing should always start from the block with the height not higher than the one
    /// specified in the genesis block.
    ///
    /// List of supported networks:
    /// - `network::Bitcoin`
    /// - `network::Testnet`
    /// - `network::Mutiny`
    pub fn yuv_genesis_block(&self) -> Option<BlockHash> {
        let Some(network_str) = self.get_block_by_network() else {
            return None;
        };

        Some(BlockHash::from_str(&network_str).expect("valid block hash"))
    }

    fn get_block_by_network(&self) -> Option<String> {
        match self {
            Network::Bitcoin => Some(MAINNET_GENESIS_BLOCK.into()),
            Network::Testnet => Some(TESTNET_GENESIS_BLOCK.into()),
            Network::Mutiny => Some(MUTINY_GENESIS_BLOCK.into()),
            _ => None,
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Network {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        Network::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl From<BitcoinNetwork> for Network {
    fn from(network: BitcoinNetwork) -> Self {
        match network {
            BitcoinNetwork::Bitcoin => Self::Bitcoin,
            BitcoinNetwork::Testnet => Self::Testnet,
            BitcoinNetwork::Signet => Self::Testnet,
            BitcoinNetwork::Regtest => Self::Regtest,
            _ => Self::Regtest,
        }
    }
}

impl FromStr for Network {
    type Err = NetworkParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bitcoin" => Ok(Self::Bitcoin),
            "testnet" => Ok(Self::Testnet),
            "regtest" => Ok(Self::Regtest),
            "signet" => Ok(Self::Signet),
            "mutiny" => Ok(Self::Mutiny),
            _ => Err(NetworkParseError::UnknownType),
        }
    }
}

#[derive(Debug)]
pub enum NetworkParseError {
    UnknownType,
}

impl Display for NetworkParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetworkParseError::UnknownType => write!(f, "Unknown network type"),
        }
    }
}

#[cfg(not(feature = "no-std"))]
impl std::error::Error for NetworkParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NetworkParseError::UnknownType => None,
        }
    }
}
