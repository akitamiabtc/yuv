# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2024-13-06

### Added

* New application ogaki - utility for automatic restart-on-update feature for YUVd node.
* Add the optional `max_request_size_kb` parameter to the node configuration.
* Add `version` method to the `yuv-cli`.
* Add support of the transfer ownership announcement.
* Add `zmqpubrawblock` and `zmqpubrawtx` options to the `bitcoin.conf` file.
* Add minimal block height from which the node will start index it.
* Add the transaction's id to the `listyuvtransactions` RPC method.

## Fixed

* Update nodes configs with the network value set to regtest.
* Remove usage of openssl in YUVd.
* Remove bdk dependency from pixels crate.

## Changed

* The new default transaction size limit is 20480 kilobytes, which is 20 megabytes.
* Upgrade the `bdk` version from the `0.29.0`.
* Upgrade `rust-bitcoin` version to the `0.30.2`.
* Change base image for YUVd docker container.

## [0.2.0] - 2024-05-06

### Added

- Add support for multichromatic bulletproof transfers.
- Add additional Schnorr signature and missing ecdh private key generation for the change output
  to the bulletproof transaction.
- Replace the previous jsonrpc implementation with the fork of `rust-jsonprc`.
- Add request timeout to the Bitcoin RPC client.
- Add `apk add openssl-dev` to the builder image.
- Add schema part to the bnode URL in `yuvd` dockerfiles.
- Temporary decreased the size of the transaction checker worker pool to avoid collision during the
  total supply updating.
- Add Bitcoin forks handling to the `Indexer`.
- Add constants with YUV genesis block hashes for different networks.
- Add banning of p2p peers that have an outdated p2p version.
- Add a custom Network type we can further use to add custom networks.
- Add support for `Mutiny` network.
- Add a list of hardcoded Mutiny bootnodes.
- Add the ability to send announcement messages with Esplora `bitcoin-provider` in YUV CLI.
- Add support for https in bitcoin jsonRPC

### Fixed

- Decreased the default P2P reactor wake-up duration to 5s, which resolves the long shutdown
  problem.
- Fix bitcoind healthcheck in docker-compose. 
- Rename `transaction.rs` to the `isolated_checks.rs` to avoid confusion.
- Add SIGTERM event listening to gracefully shutdown the YUV node in docker container.
- Fix the waste of satoshis on `OP_RETURN` for announcements.

## [0.1.1] - 2024-22-05

### Fixed

- Move the messages about failing to retrieve the block in the blockloader to the warn log level.
- Add the check to the `AnnouncementIndexer` if the `OP_RETURN` isn't an announcement message to not
  spam with error messages.
- Update the handler to properly handle issuance transactions and avoid collisions between RPC and
  indexer.
- Move tx confirmation to a separate crate.
- Add event about an announcement message is checked to the `Controller`.
- Zero amount proofs are skipped at check step.
- Fix missing witness data in issue transaction inputs while drain tweaked satoshis.
- Fix the YUV node's connection to itself due to unfiltered P2P's `Addr` message.
- Fix the waste of satoshis on `OP_RETURN` for announcements.
- Add bootnode for `Mainnet` and `Mutiny Testnet` (more to come in a few days).

### Added

- Add the duration restriction of the end-to-end test to the configuration file.
- Add a bitcoin blocks mining to the end-to-end test.
- Add a custom Network type we can further use to add custom networks.
- Add support for `Mutiny` network.
- Add a list of hardcoded Mutiny bootnodes.
- Add the ability to send announcement messages with Esplora `bitcoin-provider` in YUV CLI.
