//! Bitcoin peer network. Eg. *Mainnet*.

use bitcoin::network::constants::ServiceFlags;

/// Peer services supported by p2p.
#[derive(Debug, Copy, Clone, Default)]
pub enum Services {
    /// Peers with compact filter support.
    #[default]
    All,
    /// Peers with only block support.
    Chain,
}

impl From<Services> for ServiceFlags {
    fn from(value: Services) -> Self {
        match value {
            Services::All => Self::COMPACT_FILTERS | Self::NETWORK,
            Services::Chain => Self::NETWORK,
        }
    }
}
