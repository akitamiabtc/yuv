# `bitcoin-client`

A simple asynchronous wrapper around [`bitcoincore-rpc`] crate which adds
a queue for requests and responses for Bitcoin node.

> As this method is not efficient due to lack of effective parallelism (#11),
> we deciding to move alternative methods, so this crate in near future should
> be deprecated.

[`bitcoincore-rpc`]: https://docs.rs/bitcoincore-rpc/0.17.0/bitcoincore_rpc/
