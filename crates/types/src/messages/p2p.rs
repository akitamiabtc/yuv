#[cfg(feature = "consensus")]
use std::io::{self, Read};

#[cfg(feature = "consensus")]
use crate::consensus::{InventoryWrapper, YuvTxsWrapper};

use crate::YuvTransaction;
use alloc::vec::Vec;

#[cfg(feature = "consensus")]
use alloc::vec;

#[cfg(feature = "consensus")]
use bitcoin::consensus::{
    encode::{self, CheckedData},
    Decodable, Encodable,
};

use bitcoin::network::{message::CommandString, message_network::VersionMessage, Address, Magic};
use bitcoin::Txid;

#[cfg(feature = "consensus")]
const MAX_MSG_SIZE: u64 = 5_000_000;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Inventory {
    /// Yuv tx ids
    Ytx(Txid),
}

/// Raw message which is sent between peers
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RawNetworkMessage {
    pub magic: Magic,
    pub payload: NetworkMessage,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NetworkMessage {
    /// INV method. Contains list of recent transaction ids
    Inv(Vec<Inventory>),

    /// ADDR method. Contains peer address
    Addr(Vec<(u32, Address)>),

    /// GETADDR method
    GetAddr,

    /// GET DATA method. Contains list of transaction ids to request
    GetData(Vec<Inventory>),

    /// YUV TX method. Contains list of transactions
    YuvTx(Vec<YuvTransaction>),

    /// PING method. Contains random nonce
    Ping(u64),

    /// PONG method. Contains same nonce that was received by PING method
    Pong(u64),

    /// VERACK method
    Verack,

    /// VERSION method
    Version(VersionMessage),

    /// WTXIDRELAY method (defines whether the node supports BIP 339)
    WtxidRelay,

    /// YTXIDRELAY method (defines whether the node supports YUV protocol )
    YtxidRelay,

    /// YTXIDACK method (acknowledges the support of YUV protocol )
    Ytxidack,

    /// Any other message.
    Unknown {
        /// The command of this message.
        command: CommandString,
        /// The payload of this message.
        payload: Vec<u8>,
    },
}

impl NetworkMessage {
    pub fn cmd(&self) -> &'static str {
        match *self {
            NetworkMessage::Inv(_) => "inv",
            NetworkMessage::Addr(_) => "addr",
            NetworkMessage::GetData(_) => "getdata",
            NetworkMessage::YuvTx(_) => "yuvtx",
            NetworkMessage::Ping(_) => "ping",
            NetworkMessage::Pong(_) => "pong",
            NetworkMessage::Verack => "verack",
            NetworkMessage::Version(_) => "version",
            NetworkMessage::WtxidRelay => "wtxidrelay",
            NetworkMessage::YtxidRelay => "ytxidrelay",
            NetworkMessage::Ytxidack => "ytxidack",
            NetworkMessage::GetAddr => "getaddr",

            _ => "unknown",
        }
    }

    /// Return the CommandString for the message command.
    pub fn command(&self) -> CommandString {
        CommandString::try_from_static(self.cmd()).expect("cmd returns valid commands")
    }
}

impl RawNetworkMessage {
    /// Return the CommandString for the message command.
    pub fn command(&self) -> CommandString {
        self.payload.command()
    }
}

#[cfg(feature = "consensus")]
impl Encodable for RawNetworkMessage {
    fn consensus_encode<W: io::Write + ?Sized>(&self, w: &mut W) -> Result<usize, io::Error> {
        let mut len = 0;
        len += self.magic.consensus_encode(w)?;
        len += self.command().consensus_encode(w)?;
        len += CheckedData(match self.payload {
            NetworkMessage::Inv(ref dat) => serialize_consensus(&InventoryWrapper(dat.to_vec())),
            NetworkMessage::Addr(ref dat) => serialize_consensus(dat),
            NetworkMessage::GetData(ref dat) => {
                serialize_consensus(&InventoryWrapper(dat.to_vec()))
            }
            NetworkMessage::YuvTx(ref dat) => serialize_consensus(&YuvTxsWrapper(dat.to_vec())),
            NetworkMessage::Ping(ref dat) => serialize_consensus(dat),
            NetworkMessage::Pong(ref dat) => serialize_consensus(dat),
            NetworkMessage::Verack
            | NetworkMessage::WtxidRelay
            | NetworkMessage::YtxidRelay
            | NetworkMessage::Ytxidack
            | NetworkMessage::GetAddr => vec![],
            NetworkMessage::Version(ref dat) => serialize_consensus(dat),
            NetworkMessage::Unknown {
                payload: ref dat, ..
            } => serialize_consensus(dat),
        })
        .consensus_encode(w)?;
        Ok(len)
    }
}

#[cfg(feature = "consensus")]
pub fn serialize_consensus<T: Encodable + ?Sized>(data: &T) -> Vec<u8> {
    let mut encoder = Vec::new();
    let len = data
        .consensus_encode(&mut encoder)
        .expect("in-memory writers don't error");
    debug_assert_eq!(len, encoder.len());
    encoder
}

#[cfg(feature = "consensus")]
impl Decodable for RawNetworkMessage {
    fn consensus_decode_from_finite_reader<R: io::Read + ?Sized>(
        r: &mut R,
    ) -> Result<Self, encode::Error> {
        let magic = Decodable::consensus_decode(r)?;
        let cmd = CommandString::consensus_decode(r)?;
        let raw_payload = CheckedData::consensus_decode(r)?.0;

        let mut mem_d = io::Cursor::new(raw_payload.clone());

        let payload = match &cmd.to_string()[..] {
            "inv" => NetworkMessage::Inv(InventoryWrapper::consensus_decode(&mut mem_d)?.0),
            "getdata" => NetworkMessage::GetData(InventoryWrapper::consensus_decode(&mut mem_d)?.0),
            "yuvtx" => {
                let txs = YuvTxsWrapper::consensus_decode(&mut raw_payload.as_slice())?;
                NetworkMessage::YuvTx(txs.0)
            }
            "ping" => NetworkMessage::Ping(Decodable::consensus_decode(&mut mem_d)?),
            "pong" => NetworkMessage::Pong(Decodable::consensus_decode(&mut mem_d)?),
            "addr" => NetworkMessage::Addr(Decodable::consensus_decode(&mut mem_d)?),
            "version" => NetworkMessage::Version(Decodable::consensus_decode(&mut mem_d)?),
            "verack" => NetworkMessage::Verack,
            "wtxidrelay" => NetworkMessage::WtxidRelay,
            "ytxidrelay" => NetworkMessage::YtxidRelay,
            "ytxidack" => NetworkMessage::Ytxidack,
            "getaddr" => NetworkMessage::GetAddr,
            _ => NetworkMessage::Unknown {
                command: cmd,
                payload: mem_d.into_inner(),
            },
        };
        Ok(RawNetworkMessage { magic, payload })
    }

    #[inline]
    fn consensus_decode<R: Read + ?Sized>(r: &mut R) -> Result<Self, encode::Error> {
        Self::consensus_decode_from_finite_reader(r.take(MAX_MSG_SIZE).by_ref())
    }
}
