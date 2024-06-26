# JSONRPC API spec of the YUVd node

## Transactions Methods

Table of contents:

- [`provideyuvproof`]
- [`providelistyuvproofs`]
- [`getlistrawyuvtransactions`]
- [`listyuvtransactions`]
- [`sendrawyuvtransaction`]
- [`isyuvtxoutfrozen`]
- [`emulateyuvtransaction`]

### Provide Proof/Proofs Methods

- [`provideyuvproof`]
- [`providelistyuvproofs`]

These methods are used to provide either a single or a list of YUV proofs for transactions existing on the Bitcoin chain.

Both [`provideyuvproof`] and [`providelistyuvproofs`] will return an error if the Bitcoin node to which the YUV node is connected does not have such a transaction.

#### [`provideyuvproof`]

Provide proof for a single YUV transaction to the YUV node without submitting it on-chain.

```
provideyuvproof "yuv-transaction"
```

Parameters:

- `yuv-transaction` - a [YUV transaction] serialized in JSON format.

Returns:

`boolean` - `true` if the proof was successfully provided, `false` otherwise.

> [!NOTE] 
> For now, `true` on success, otherwise an error is thrown.

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"provideyuvproof","params":[{"bitcoin_tx":{"version":1,"lock_time":315,"input":[{"previous_output":"9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec:1","script_sig":"","sequence":4294967294,"witness":["304402203c50474c2ba73b0b00d3e660d05bfd1edb2fc056995c45acba5181eab21f7c19022049599c14c20aa85da311536abd9967825d6b9f36eafc67429a112efe6d7f57fa01","025510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"]}],"output":[{"value":10000,"script_pubkey":"001416648ddda83c0322c36b889dd32a8be3eb828553"},{"value":99918624,"script_pubkey":"001429999f2fa94a248eff7187471847dd9fa47c02dc"}]},"tx_type":{"type":"Issue","data":{"output_proofs":{"0":{"type":"Sig","data":{"pixel":{"luma":{"amount":1111},"chroma":"5510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"},"inner_key":"027bf59465bf6cb3faa969e963c6934a2bee2b38c5d981c0b2226ed669149945db"}}}}}}]}' \
    http://127.0.0.1:18333
    
# Response
{
    "result": true,
    "error": null,
    "id": 1
}
```

#### [`providelistyuvproofs`]

Provide YUV transactions to the YUV node without submitting them on-chain.

```
providelistyuvproofs "yuv-transactions"
```

Parameters:

`yuv-transactions` - list of [YUV transaction]s serialized in JSON format.

Returns:

`boolean` - `true` if the proof was successfully provided, `false` otherwise.

## Get YUV Transactions Methods

- [`listyuvtransactions`]
- [`getrawyuvtransaction`]
- [`getlistrawyuvtransactions`]

### [`listyuvtransactions`]

Transactions in the YUV node are stored in pages, where order in each page is
determined by the arrival time of the transaction. Therefore, different nodes
may have different order of transactions in pages. This method returns a list of
YUV transactions from the specified page.

> [!NOTE]
> The page size for each node may vary, as it's a configurable parameter. This
> method is used for wallets to sync and index wallet's transactions history.

```
listyuvtransactions "page"
```

Parameters:

- `page` - page number of the list of YUV transactions.

Returns:

List of [YUV transaction]s along with their `Txid`s.

Examples:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"listyuvtransactions","params":[0]}' \
    http://127.0.0.1:18333

# Response
{
    "result": [
        # serialized YUV transactions in JSON format. 
    ],
    "error": null,
    "id": 1
}
```

### [`getlistrawyuvtransactions`]

Get a list of YUV transactions by IDs. If the YUV node is missing some of the
transactions, `getlistrawyuvtransactions` will skip them and return the other.

```
getlistrawyuvtransactions "txids"
```

Parameters:

- `txids` - list of transaction ids.

Returns:

List of [YUV transaction]s 

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getlistrawyuvtransactions","params":[["txid1", "txid2"]]}' \
    http://127.0.0.1:18333
    
# Response

{
    "result": [
        # serialized YUV transactions in JSON format. 
    ],
    "error": null,
    "id": 1
}
```

### [`getrawyuvtransaction`]

Get YUV transaction by id with it's current state.

```
getrawyuvtransaction "txid"
```

Parameters:

- `txid` - transaction id.

Returns:

JSON object with the following fields:

* `status` - status of the transaction. Possible values are:
    * `none` - transaction is not found;
    * `pending` - transaction is in the mempool, but it's in the queue to be checked;
    * `checked` - transaction is in the mempool and is checked, but not attached;
    * `attached` - transaction is attached and accepted by the YUV node.
    
* `data` - a [YUV transaction] serialized in JSON format. Is presented only if
  `status` is `attached`.

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getrawyuvtransaction","params":["9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec"]}' \
    http://127.0.0.1:18333
    
# Response

{
  "jsonrpc": "2.0",
  "result": {
    "status": "none"
  },
  "id": 1
}
```

### Send YUV Transaction Methods

- [`sendrawyuvtransaction`]

#### [`sendrawyuvtransaction`]

Send a YUV transaction to the YUV node and broadcast it to the Bitcoin network. Once the transaction is confirmed, the YUV node will check and attach it if it's valid.

```
sendrawyuvtransaction "yuv-transaction"
```

Parameters:

- `yuv-transaction` - serialized in JSON format [YUV transaction].

Returns:

`boolean` - `true` if sent successfully.

> [!NOTE]
> Returns `true` if sent to the Bitcoin node successfully, otherwise an error will be returned.

Example:

```shell
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"sendrawyuvtransaction","params":[{"bitcoin_tx":{"version":1,"lock_time":315,"input":[{"previous_output":"9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec:1","script_sig":"","sequence":4294967294,"witness":["304402203c50474c2ba73b0b00d3e660d05bfd1edb2fc056995c45acba5181eab21f7c19022049599c14c20aa85da311536abd9967825d6b9f36eafc67429a112efe6d7f57fa01","025510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"]}],"output":[{"value":10000,"script_pubkey":"001416648ddda83c0322c36b889dd32a8be3eb828553"},{"value":99918624,"script_pubkey":"001429999f2fa94a248eff7187471847dd9fa47c02dc"}]},"tx_type":{"type":"Issue","data":{"output_proofs":{"0":{"type":"Sig","data":{"pixel":{"luma":{"amount":1111},"chroma":"5510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"},"inner_key":"027bf59465bf6cb3faa969e963c6934a2bee2b38c5d981c0b2226ed669149945db"}}}}}}]}' \
    http://127.0.0.1:18333
```

### YUV Transaction Validation Methods

- [`isyuvtxoutfrozen`]
- [`emulateyuvtransaction`]

#### [`isyuvtxoutfrozen`]

Check whether the output of a YUV transaction is frozen by the issuer or not.

```
isyuvtxoutfrozen "txid" "vout"
```

Parameters:

- `txid` - YUV transaction id.
- `vout` - output index.

Returns:

`true` if output is frozen, otherwise `false`.

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"isyuvtxoutfrozen","params":["9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec", 0]}' \
    http://127.0.0.1:18333

# Response
{
    "result": false,
    "error": null,
    "id": 1
}
```

#### [`emulateyuvtransaction`]

Emulate the process of checking and attaching a transaction without broadcasting it to the Bitcoin and YUV networks.

> [!TIP]
> This method is useful for checking if a node can immediately check and attach 
> a transaction to the internal storage.

```
emulateyuvtransaction "yuv-transation"
```

Parameters:

* `yuv-transaction` - a [YUV transaction] serialized in JSON format.

Returns:

JSON object with two formats:

On invalid:

```json
{
    "status": "invalid",
    "data": {
        "reason": "" // reason as string
    }
}
```

On valid:

```json
{
    "status": "valid",
}
```

Example:

``` shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"emulateyuvtransaction","params":[{"bitcoin_tx":{"version":1,"lock_time":315,"input":[{"previous_output":"9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec:1","script_sig":"","sequence":4294967294,"witness":["304402203c50474c2ba73b0b00d3e660d05bfd1edb2fc056995c45acba5181eab21f7c19022049599c14c20aa85da311536abd9967825d6b9f36eafc67429a112efe6d7f57fa01","025510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"]}],"output":[{"value":10000,"script_pubkey":"001416648ddda83c0322c36b889dd32a8be3eb828553"},{"value":99918624,"script_pubkey":"001429999f2fa94a248eff7187471847dd9fa47c02dc"}]},"tx_type":{"type":"Issue","data":{"output_proofs":{"0":{"type":"Sig","data":{"pixel":{"luma":{"amount":1111},"chroma":"5510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"},"inner_key":"027bf59465bf6cb3faa969e963c6934a2bee2b38c5d981c0b2226ed669149945db"}}}}}}]}' \
    http://127.0.0.1:18333

# Response
{
    "jsonrpc":"2.0",
    "result": {
        "status":"valid"
    },
    "id":1
}
```

[`provideyuvproof`]: #provideyuvproof
[`listyuvtransactions`]: #listyuvtransactions
[`providelistyuvproofs`]: #providelistyuvproofs
[`getlistrawyuvtransactions`]: #getlistrawyuvtransactions
[`sendrawyuvtransaction`]: #sendrawyuvtransaction
[`isyuvtxoutfrozen`]: #isyuvtxoutfrozen
[`emulateyuvtransaction`]: #emulateyuvtransaction
[`getrawyuvtransaction`]: #getrawyuvtransaction

[YUV transaction]: ../crates/types/src/transactions/mod.rs#L16
