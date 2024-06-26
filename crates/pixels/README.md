# `yuv-pixels`

Crate which provides base primitives of the YUV protocol, such as [`Chroma`],
[`Luma`], [`PixelKey`], [`PixelHash`], [`PixelPrivateKey`] and [`PixelProof`]
used for sending, spending and validating the transactions created using the YUV
protocol.

The main structure is [`PixelProof`] which helds all required information for
validator side (YUV node) to check if the proof attached to some output of the
Bitcoin transaction is valid.

Currently, crate supports only `P2WPKH` and `P2WSH` addresses with only specific
subset for the last one. They are:

## `P2WPKH` proof

* [`SigPixelProof`] - single signature proof for input/output.

## `P2WSH` proofs

* [`MultisigPixelProof`] - input/output proof that has a multisignature redeem
  script with an arbitary number of participants.
* [`LightningCommitmentProof`] - input/ouput proof for Lightning commitment transaction [`to_local` output].
* [`LightningHtlcProof`] - input/output proof for Lightning commitment transaction [`htlc` output].

> In future, arbitary scripts that have public key in it will be supported.

## Example

Suppose Alice wants to send 5 YUV coins to Bob. For that, she needs to create a
[`PixelKey`] with 5 YUV coins and Bob's key as a spender:

```rust
use std::str::FromStr;

use bitcoin::secp256k1::{PublicKey, Secp256k1};
use bitcoin::{Address, Network};
use yuv_pixels::{PixelKey, Chroma, Luma, Pixel};

// Get Bob's public key
let bob_pubkey = PublicKey::from_str(
    "020677b5829356bb5e0c0808478ac150a500ceab4894d09854b0f75fbe7b4162f8"
).unwrap();

// Create pixel with 5 as Luma and some Chroma
let chroma = Chroma::from_str(
    "6a5e3a83f0b2bdfb2f874c6f4679dc02568deb8987d11314a36bceacb569ad8e"
).unwrap();
let luma = Luma::from(5);
let pixel = Pixel::new(luma, chroma);

let pixel_key = PixelKey::new(pixel, &bob_pubkey).unwrap();

// Generate address for sending YUV coins (Regtest is used as an example).
let address = Address::p2wpkh(&pixel_key, Network::Regtest).unwrap();

println!("{address}");
```

Where `pixel_key` is a public key with YUV coins in it. These coins can be spent
only by Bob. Then Alice can generate a P2WPKH address from `pixel_key` and add it
as an output of the Bitcoin transaction.

After that, Alice needs to create a proof for it, and send it to the YUV node for
validation:

```rust
use std::str::FromStr;

use bitcoin::secp256k1::{PublicKey, Secp256k1};
use bitcoin::{Address, Network};
use yuv_pixels::{Chroma, Luma, Pixel, PixelProof};

// Get Bob's public key
let bob_pubkey = PublicKey::from_str(
    "020677b5829356bb5e0c0808478ac150a500ceab4894d09854b0f75fbe7b4162f8"
).unwrap();

// Create pixel with 5 as Luma and some Chroma
let chroma = Chroma::from_str(
    "6a5e3a83f0b2bdfb2f874c6f4679dc02568deb8987d11314a36bceacb569ad8e"
).unwrap();
let pixel = Pixel::new(5, chroma);

// Create a single signature P2WPKH output
let proof = PixelProof::sig(pixel, bob_pubkey); 
```

[`to_local` output]: https://github.com/lightning/bolts/blob/8a64c6a1cef979b3f0cecb00ba7a48c2d28b3588/03-transactions.md#to_local-output
[`htlc` output]: https://github.com/lightning/bolts/blob/8a64c6a1cef979b3f0cecb00ba7a48c2d28b3588/03-transactions.md#offered-htlc-outputs
