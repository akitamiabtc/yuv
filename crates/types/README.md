# `yuv-types`

A small utility crate that contains shared data types.

All the types that come through [p2p](../p2p/) implement `bitcoin::consensus::Encodable` and `bitcoin::consensus::Decodable` for the serialization. These implementations are hidden under the `consensus` feature.
