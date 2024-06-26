# `yuv-cli`

CLI tool for managing YUV transactions.

## Features

- Create a YUV transaction (`transfer`, `issue`, `freeze`):
    - Issue an asset from your pair of keys;
    - Transfer issued tokens;
    - Freeze, unfreeze YUV outputs;
- Communicate with a YUV node (`node` subcommand):
    - Provide pixel proofs to the YUV node;
    - Get YUV transactions from the YUV node;
- Validate proofs locally (`validate` subcommand);
- Generate YUV addresses, key-pairs, pixel hashes (`generate` subcommand);
- Convert instances between each other (`convert` subcommand).

## Build and install

Clone git repository:

```sh
git clone git@github.com:akitamiabtc/yuv.git
```

Install using cargo:

```sh
cargo install --path ./apps/cli
```

From now, if you've added `$HOME/.cargo/bin` to your `$PATH`, `yuv-cli`
should be available from your terminal session.

## Usage

Setup configuration file:

```toml
# config.toml
private_key = "cMzCipjMyeNdnPmG6FzB1GAL7ziTBPQ2TJ4EPWZWPdeGgbLTCAEE"

storage = "path/to/storage"

[bitcoin_provider]
type = "bitcoin_rpc"
url = "http://127.0.0.1:18443" # bitcoin node RPC url
network = "regtest"
auth = { username = "admin1", password = "123" }
# Start syncing the blockchain history from the certain timestamp
start_time = 0

# Or if you want to use Esplora:
# [bitcoint-provider]
# type = "esplora"
# url = "http://127.0.0.1:3000"
# network = "regtest"
# # stop gap - It is a setting that determines when to stop fetching transactions for a set of
# # addresses by indicating a gap of unused addresses. For example, if set to 20, the syncing
# # mechanism would stop if it encounters 20 consecutive unused addresses.
# stop_gap = 20


[yuv_rpc]
url = "http://127.0.0.1:18333"

# The fee rate strategy. Possible values:
# - { type = "estimate", target_blocks: 2 } The fee rate is fetched from Bitcoin RPC. If an error
#   occurs, the tx building process is interrupted.
# - { type = "manual", fee_rate = 1.0 } Default fee rate is used.
# - { type = "try_estimate", fee_rate = 1.0, target_blocks: 2 } The fee rate is fetched
#   automatically from Bitcoin RPC. If an error occurs, the default fee rate is used.
# NOTE: fee_rate is measured in sat/vb.
# https://developer.bitcoin.org/reference/rpc/estimatesmartfee.html
[fee_rate_strategy]
type = "manual"
fee_rate = 1.2
```

### Simple scenario

Let's go through some of the scenarios:

1. Synchronize all the wallet history (see [step 1]);
2. Create **USD Issuer** and **EUR Issuer** accounts which will issue tokens to
   users (see [step 2]);
3. Generate two key pairs of keys that will transfer YUV-coins between each other
   (let's name them **Alice** and **Bob**, see [step 3]);
4. Issue **USD** and **EUR** tokens to **Alice** (see [step 4]);
    - Check **Alice**'s balances and UTXO.
5. Transfer issued tokens from **Alice** to **Bob** (see [step 5]);
    - Perform a monochromatic transfer.
    - Perform a multichromatic transfer.
6. Using **USD Issuer**'s keys create a freeze transaction for **Bob**'s output
   (see [step 6]);
7. Using **USD Issuer**'s keys create an unfreeze transaction for **Bob**'s output (see [step 7]);

> We will use [Nigiri] for this demo to setup configured Regtest Bitcoin node and fund our freshly
> created users with Bitcoins.

[Nigiri]: https://nigiri.vulpem.com/

> When you've installed `nigiri`, start the node using `nigiri start` with some
> helpful daemons like explorer and webapp.

#### 1. Synchronize the wallet history

Use the following command to synchronize your wallet:

> NOTE: replace the `config.toml` with a path to your configuration file.

``` sh
yuv-cli --config ./config.toml wallet sync
```

It could take some time, so be calm and make a cup of coffee for yourself. Also you can change
`start_time` field in the `[bitcoin_provider]` section to cut down on synchronizing time. If you
want to
interrupt the syncing process, use the following command:

``` sh
yuv-cli --config ./config.toml wallet abort-rescan
```

This command will be done in case when you are using `bitcoin_rpc` configuration for
`[bitcoin_provider]` (see  [usage]);

#### 2. Generate **USD Issuer** and **EUR Issuer** key pairs

Generate **EUR Issuer** key pair:

```sh
yuv-cli generate keypair --network regtest
```

RESULT:

```text
Private key: cUK2ZdLQWWpKeFcrrD7BBjiUsEns9M3MFBTkmLTXyzs66TQN72eX
P2TR address: bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n
P2WPKH address: bcrt1qplal8wyn20chw4jfdamkk5vnfkpwdm3vyd46ew
```

<details>
<summary>Configuration file for <b>EUR Issuer</b> </summary>

```toml
# eur.toml
private_key = "cUK2ZdLQWWpKeFcrrD7BBjiUsEns9M3MFBTkmLTXyzs66TQN72eX"

storage = ".users/eur"

[bitcoin_provider]
type = "bitcoin_rpc"
url = "http://127.0.0.1:18443"
auth = { username = "admin1", password = "123" }
network = "regtest"
fee_rate_strategy = { type = "manual", fee_rate = 1.0, target = 2 }
start_time = 0

[yuv_rpc]
url = "http://127.0.0.1:18333"

[fee_rate_strategy]
type = "manual"
fee_rate = 1.2
```

</details>

**USD Issuer** keypair:

```text
Private key: cNMMXcLoM65N5GaULU7ct2vexmQnJ5i5j3Sjc6iNnEF18vY7gzn9
P2TR address: bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30
P2WPKH address: bcrt1qycd9xdayguzayn40ua56slsdm0a9ckn3n34tv0
```

<details>
<summary>Configuration file for <b>USD Issuer</b> </summary>

```toml
# usd.toml
private_key = "cNMMXcLoM65N5GaULU7ct2vexmQnJ5i5j3Sjc6iNnEF18vY7gzn9"

storage = ".users/usd"

[bitcoin_provider]
type = "bitcoin_rpc"
url = "http://127.0.0.1:18443"
auth = { username = "admin1", password = "123" }
network = "regtest"
fee_rate_strategy = { type = "manual", fee_rate = 1.0, target = 2 }
start_time = 0

[yuv_rpc]
url = "http://127.0.0.1:18333"

[fee_rate_strategy]
type = "manual"
fee_rate = 1.2
```

</details>

Also, lets fund issuers with one Bitcoin:

```sh
nigiri faucet bcrt1qplal8wyn20chw4jfdamkk5vnfkpwdm3vyd46ew 1
nigiri faucet bcrt1qycd9xdayguzayn40ua56slsdm0a9ckn3n34tv0 1
```

#### 3. Generate **Alice** and **Bob** key pairs

Generate a key pair for **Alice**:

```text
Private key: cQb7JarJTBoeu6eLvyDnHYNr6Hz4AuAnELutxcY478ySZy2i29FA
P2TR address: bcrt1phhfvq20ysdh6ht8fhtp7e8xfemva23lr703mtyrnuv7fkdggayvsz8x8gd
P2WPKH address: bcrt1q69j54cjd44wuvaqv4lmnyrw89ve4ufq3cx37mr
```

<details>
<summary>Configuration file for <b>Alice</b></summary>

```toml
# alice.toml
private_key = "cQb7JarJTBoeu6eLvyDnHYNr6Hz4AuAnELutxcY478ySZy2i29FA"

storage = ".users/alice"

[bitcoin_provider]
type = "bitcoin_rpc"
url = "http://127.0.0.1:18443"
auth = { username = "admin1", password = "123" }
network = "regtest"
fee_rate_strategy = { type = "manual", fee_rate = 1.0, target = 2 }
start_time = 0

[yuv_rpc]
url = "http://127.0.0.1:18333"

[fee_rate_strategy]
type = "manual"
fee_rate = 1.2
```

</details>

and **Bob**:

```text
Private key: cUrMc62nnFeQuzXb26KPizCJQPp7449fsPsqn5NCHTwahSvqqRkV
P2TR address: bcrt1p03egc6nv2ardypk2qpwru20sv7pfsxrn43wv7ts785rq5s8a8tmqjhunh7
P2WPKH address: bcrt1q732vnwgml595glrucr00rt8584x58mjp6xtnmf
```

<details>
<summary>Configuration file for <b>Bob</b></summary>

```toml
# bob.toml
private_key = "cUrMc62nnFeQuzXb26KPizCJQPp7449fsPsqn5NCHTwahSvqqRkV"

storage = ".users/bob"

[bitcoin_provider]
type = "bitcoin_rpc"
url = "http://127.0.0.1:18443"
auth = { username = "admin1", password = "123" }
network = "regtest"
fee_rate_strategy = { type = "manual", fee_rate = 1.0, target = 2 }
start_time = 0

[yuv_rpc]
url = "http://127.0.0.1:18333"

[fee_rate_strategy]
type = "manual"
fee_rate = 1.2
```

</details>

Also, lets copy their keys to environmental variables:

```sh
export ALICE="bcrt1phhfvq20ysdh6ht8fhtp7e8xfemva23lr703mtyrnuv7fkdggayvsz8x8gd"
export BOB="bcrt1p03egc6nv2ardypk2qpwru20sv7pfsxrn43wv7ts785rq5s8a8tmqjhunh7"
export USD="bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30"
export EUR="bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n"
```

#### 4. Create issuances for **Alice**

Now we are ready to create issuance of 10000 **USD** tokens for **Alice**:

```sh
yuv-cli --config ./usd.toml issue --amount 10000 --recipient $ALICE
```

Where `amount` is issuance amount, `recipient` - **Alice**'s public key (read
from environment variable added in [step 2]).

RESULT:

```text
tx id: 4f98d522ad33152af8392fc13f191ae966c5503e2ced2aad116c41890641b807
type: Issue
data:
  output_proofs:
    0:
      type: Sig
      data:
        pixel:
          luma:
            amount: 10000
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
    1:
      type: EmptyPixel
      data:
        inner_key: 03ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
```

As the result, you will get the transaction ID and structure of the issuance
proof of the YUV transaction. By parameters obtained from configuration file,
`yuv-cli` will send it for broadcasting to YUV node with created proofs, where
the node will wait until the tranasction is mined to check it before accepting.

> There is an empty pixel. It doesn't hold any Pixel data, it is
> just empty proof indicating that this Bitcoin output holds only satoshis
> and zero YUV tokens.

Using `nigiri` let's mine the next block:

```sh
nigiri rpc --generate 1
```

Check that the transaction has been accepted by the node:

```sh
yuv-cli --config ./usd.toml get --txid 4f98d522ad33152af8392fc13f191ae966c5503e2ced2aad116c41890641b807
```

As a sign of acceptance, you would receive a YUV transaction in JSON format.

Also, we can check current **Alice**'s balances:

```sh
yuv-cli --config ./alice.toml balances
```

RESULT:

```text
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 10000
```

Let's do the same with **EUR Issuer**:

```sh
yuv-cli --config ./eur.toml issue --amount 10000 --recipient $ALICE
nigiri rpc --generate 1
```

And check balances again:

```sh
yuv-cli --config ./alice.toml balances
```

RESULT:

```text
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 10000
bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n: 10000
```

#### 5. Transfer from **Alice** to **Bob**

Now, let's move on to the transfer. Fund **Alice** with one Bitcoin:

```sh
nigiri faucet bcrt1qm5wu5zjyswyw877kq8dup6k02nef29wwc2tcwu 1
```

We are ready to transfer 1000 **USD** tokens from **Alice** to **Bob**:

```sh
yuv-cli --config ./alice.toml transfer \
    --chroma $USD \
    --amount 1000 \
    --recipient $BOB
```

RESULT:

```text
tx id: a5bfd730c26e6b08ee9a0a02f1140e5527d1d47d45fc78fecbc661c5bc9383d5
type: Transfer
data:
  input_proofs:
    0:
      type: Sig
      data:
        pixel:
          luma:
            amount: 10000
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
  output_proofs:
    0:
      type: Sig
      data:
        pixel:
          luma:
            amount: 1000
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 027c728c6a6c5746d206ca005c3e29f06782981873ac5ccf2e1e3d060a40fd3af6
    1:
      type: Sig
      data:
        pixel:
          luma:
            amount: 9000
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
    2:
      type: EmptyPixel
      data:
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
```

Generate block using `nigiri`:

```sh
nigiri rpc --generate 1
```

And check balances of both users:

```sh
yuv-cli --config ./alice.toml balances
```

RESULT:

```text
bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n: 10000
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 9000
```

```sh
yuv-cli --config ./bob.toml balances
```

RESULT:

```text
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 1000
```

##### Tweaked Bitcoin UTXOs and Sweep

You have already seen that YUV puts empty pixel proofs to the outputs that don't hold any YUV
tokens.
These outputs are actually tweaked just like the outputs that hold actual Pixel data, but they are
tweaked
with empty pixels, i.e. with zero Luma and Chroma.

To spend these tweaked UTXOs, you need to create a **sweep** transaction. This means to create a
transaction which spends
all YUV outputs tweaked by zero pixels to a **P2WPKH address**.

This can be easily done with `yuv-cli`.
In the above example, Alice's transfer transaction contained a change output that was tweaked with
an empty pixel.
To sweep it and all the other tweaked outputs (if any), Alice simply needs to execute:

```sh
yuv-cli --config ./alice.toml sweep
```

RESULT:

```text
tx id: f552b5b5146b390c5c73e4a4f22920a5fff14e56dffe17ca7f8b3235324f6c06
```

If there are no tweaked Bitcoin outputs with empty Pixel proofs, the following message will be
displayed:

```text
Address has no tweaked Bitcoin UTXOs
```

##### Multichromatic transfers

We covered monochromatic transfers above (i.e. each transfer contained a single chroma).
Now, let's try to perform a multichromatic transfer and send both **EUR** and **USD** from **Alice**
to **Bob** in a single transfer.

As Alice's balance is already filled with some **EUR** and **USD**, we are ready to make a transfer:

```sh
yuv-cli --config ./alice.toml transfer \
    --chroma $USD \
    --amount 500 \
    --recipient $BOB \
    --chroma $EUR \
    --amount 1000 \
    --recipient $BOB
```

RESULT:

```text
tx id: 036d18a4c8ceb75c08a3abcabb40f8a239bbebd19751c365c610569581cda7db
type: Transfer
data:
  input_proofs:
    0:
      type: Sig
      data:
        pixel:
          luma:
            amount: 9000
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
    1:
      type: Sig
      data:
        pixel:
          luma:
            amount: 10000
          chroma: b92726575fc55904349c38053ed47633f7a6aa153e25b31f6bc02d92506a8ad5
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
  output_proofs:
    0:
      type: Sig
      data:
        pixel:
          luma:
            amount: 500
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 027c728c6a6c5746d206ca005c3e29f06782981873ac5ccf2e1e3d060a40fd3af6
    1:
      type: Sig
      data:
        pixel:
          luma:
            amount: 1000
          chroma: b92726575fc55904349c38053ed47633f7a6aa153e25b31f6bc02d92506a8ad5
        inner_key: 027c728c6a6c5746d206ca005c3e29f06782981873ac5ccf2e1e3d060a40fd3af6
    2:
      type: Sig
      data:
        pixel:
          luma:
            amount: 8500
          chroma: ab28d32fe218d3cb53d330e2dd21db5b32dafb9fc5296c42d17dcb1cd63beab2
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
    3:
      type: Sig
      data:
        pixel:
          luma:
            amount: 9000
          chroma: b92726575fc55904349c38053ed47633f7a6aa153e25b31f6bc02d92506a8ad5
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
    4:
      type: EmptyPixel
      data:
        inner_key: 02bdd2c029e4836fabace9bac3ec9cc9ced9d547e3f3e3b59073e33c9b3508e919
```

Generate a block using `nigiri`:

```sh
nigiri rpc --generate 1
```

And check balances of both users:

```sh
yuv-cli --config ./alice.toml balances
```

RESULT:

```text
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 8500
bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n: 9000
```

```sh
yuv-cli --config ./bob.toml balances
```

RESULT:

```text
bcrt1phynjv46lc4vsgdyu8qzna4rkx0m6d2s48cjmx8mtcqkey5r23t2swjhv5n: 1000
bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30: 1500
```

**NOTE:** it's also acceptable to specify different recipients in a multichromatic transfer.

#### 6. Freeze Bob's output

Let's see **Bob**'s YUV UTXOS:

```sh
yuv-cli --config ./bob.toml utxos --chroma $USD
```

RESULT:

```text
477df4cb007a46fe9efd7de75ffa7012846d9babea3f31bbb50c9b93f12ff7f5:0 1000
6936880d51e5fd92b6dd3c754905b538f146f69942080c4f3dca8b99d5f1f086:0 500
```

Using **USD Issuer**'s keys create a freeze transaction for **Bob**'s output:

```sh
yuv-cli --config ./usd.toml freeze 477df4cb007a46fe9efd7de75ffa7012846d9babea3f31bbb50c9b93f12ff7f5 0
```

RESULT:

```text
Transaction broadcasted: abf54fedcdd13158b425f2841587f6874c5cc25935c3f2bd0b863ab7bac8e854
```

Generate block using `nigiri`:

```text
nigiri rpc --generate 1
```

> Also, you can check if that transaction was indexed by node:

```sh
yuv-cli --config ./usd.toml get --txid e8891f004680eefdd8faf149073796d1b189e39454ebd8a68a112fed2b135aae
```

And check **Bob**s UTXOS after that:

```sh
yuv-cli --config ./bob.toml utxos $USD
```

Now **Bob** has one less UTXO:

```text
6936880d51e5fd92b6dd3c754905b538f146f69942080c4f3dca8b99d5f1f086:0 500
```

#### 7. Create unfreeze transaction for **Bob**'s output

Using **Issuer**'s keys create an unfreeze transaction for **Bob**'s output:

```sh
yuv-cli --config ./usd.toml unfreeze 477df4cb007a46fe9efd7de75ffa7012846d9babea3f31bbb50c9b93f12ff7f5 0
```

RESULT:

```text
Transaction broadcasted: 5faeae04cd7b4d853866eb427896a3a6fff89f2e2f320def1950cd30e0c43b8f
```

Generate block:

```sh
nigiri rpc --generate 1
```

Also, you may check if that transaction was indexed by node:

```sh
yuv-cli --config ./usd.toml get --txid 5faeae04cd7b4d853866eb427896a3a6fff89f2e2f320def1950cd30e0c43b8f
```

And finally, check **Bob**'s YUV UTXOS:

```sh
yuv-cli --config ./bob.toml utxos $USD
```

RESULT:

```text
477df4cb007a46fe9efd7de75ffa7012846d9babea3f31bbb50c9b93f12ff7f5:0 1000
6936880d51e5fd92b6dd3c754905b538f146f69942080c4f3dca8b99d5f1f086:0 500
```

#### 8. Bulletproofs

Bulletproof transactions are meant to be used to send anonymous transactions, i.e. transactions with hidden amounts.

> **_NOTE:_** Chromas and recipients are still visible to everyone. Only amounts are hidden.

Only those tokens that were issued using bulletproofs can be transfered anonymously.

Let's start with the bulletproof issuance of 10000 **USD** tokens for **Alice**:

```sh
export ISSUANCE_TX_ID=$(yuv-cli --config ./usd.toml bulletproof issue --satoshis 10000 --amount 10000 --recipient $ALICE)
```

Generate block using `nigiri`:

```sh
nigiri rpc --generate 1
```

Let's check that Pedersen's commitment to the issuance bulletproof that we received is valid:

```sh
yuv-cli --config ./alice.toml bulletproof check --amount 10000 --outpoint $ISSUANCE_TX_ID:0 --sender $USD
```

Now, let's transfer 1000 **USD** tokens from **Alice** to **Bob**.
For that, we are passing the outpoint of the issuance we sent earlier:

```sh
export TRANSFER_TX_ID=$(yuv-cli --config alice.dev.toml bulletproof transfer --amount 1000 --residual 9000 --satoshis 2000 --residual-satoshis 7000 --chroma $USD --recipient $BOB --outpoint $ISSUANCE_TX_ID:0)
```

> **_NOTE:_** if you intend to send the transfer without change, just set `residual` and `residual-satoshis` to `0`.

Generate block using `nigiri`:

```sh
nigiri rpc --generate 1
```

Finally check that Pedersen's commitment to the transfer bulletproof that we received is valid:

```sh
yuv-cli --config ./bob.toml bulletproof check --amount 1000 --tx $TRANSFER_TX_ID:0 --sender $ALICE
```

> **_NOTE:_** multichromatic bulletproof transfers are supported too.

[step 1]: #1-synchronize-the-wallet-history

[step 2]: #2-generate-usd-issuer-and-eur-issuer-key-pairs

[step 3]: #3-generate-alice-and-bob-key-pairs

[step 4]: #4-create-issuances-for-alice

[step 5]: #5-transfer-from-alice-to-bob

[step 6]: #6-freeze-bobs-output

[step 7]: #7-create-unfreeze-transaction-for-bobs-output

[usage]: #usage

#### 9. Chroma announcement

Any issuer can announce a new Chroma (new token) to the network. This is done by creating a
transaction with a single output that contains `OP_RETURN` with information about the new Chroma.

The next data is contained in the Chroma announcement:

- `chroma` - 32 bytes [`Chroma`].
- `name` - 1 + [3 - 32] bytes name of the token. Where the first byte is the length of the name.
- `symbol` - 1 + [3 - 16] bytes symbol of the token. Where the first byte is the length of the
  symbol.
- `decimal` - 1 byte number of decimal places for the token.
- `max_supply` - 8 bytes maximum supply of the token.
- `is_freezable` - 1 byte indicates whether the token can be freezed or not by the issuer.

To announce a new Chroma with `yuv-cli` you need to execute the following command:

```sh
yuv-cli --config ./usd.toml chroma announcement --name "Some name" --symbol SMN --decimal 2
```

`chroma` isn't specified, so it was taken from the config. In this case the `max_supply` is 0 -
unlimited. `is_freezable` is set to `true` by default.

As a result, you will get the transaction ID of the Chroma announcement transaction.

To check the Chroma announcement, you can use the following command:

```sh
yuv-cli --config ./alice.toml chroma info $USD
```

Result:

```text
Chroma: bcrt1p4v5dxtlzrrfuk57nxr3d6gwmtved47ulc55kcsk30h93e43ma2eqvrek30
Name: Some name
Symbol: SMN
Decimal: 2
Max supply: unlimited
Is freezable: true
```
