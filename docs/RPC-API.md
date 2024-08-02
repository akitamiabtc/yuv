# JSONRPC API spec of the YUVd node

## Transactions Methods

Table of contents:

- [`provideyuvproof`]
- [`providelistyuvproofs`]
- [`getlistrawyuvtransactions`]
- [`listyuvtransactions`]
- [`sendrawyuvtransaction`]
- [`sendyuvtransaction`]
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
provideyuvproof "txid" "proofs" ( blockhash )
```

Parameters:

- `txid` - `Txid` of the [YUV transaction].
- `proofs` - hex encoded [YUV transaction] proofs.
- `blockhash` (optional) - the block in which to look for the transaction using the provided `Txid`.

Returns:

`boolean` - `true` if the proof was successfully provided, `false` otherwise.

> [!NOTE] 
> For now, `true` on success, otherwise an error is thrown.

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"provideyuvproof","params":["2b5ca3ed29459c0bb6d3dc554b87319ce6f7a03a1762dadde4d33f24bd950f89", "00797576000286b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f1027000000000000000000000000000001020000000100000000000000000000000000000000000027100000000000000000000000000000000086b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f02c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a2302000000050286b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f"]}' \      
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
providelistyuvproofs "proofs"
```

Parameters:

`proofs` - list of `Txid`s and hex encoded [YUV transaction] types.

Returns:

`boolean` - `true` if the proofs were successfully provided, `false` otherwise.

Example:

```shell
# Request
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"providelistyuvproofs","params":[[{"txid":"2b5ca3ed29459c0bb6d3dc554b87319ce6f7a03a1762dadde4d33f24bd950f89", "tx_type":"00797576000286b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f1027000000000000000000000000000001020000000100000000000000000000000000000000000027100000000000000000000000000000000086b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f02c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a2302000000050286b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f"}]]}' \       
    http://127.0.0.1:18333
    
# Response
{
    "result": true,
    "error": null,
    "id": 1
}
```

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
- [`sendyuvtransaction`]

#### [`sendrawyuvtransaction`]

Send a HEX or JSON serialized YUV transaction to the YUV node and broadcast it to the Bitcoin network. Once the transaction is confirmed, the YUV node will check and attach it if it's valid.

```
sendrawyuvtransaction "yuv-transaction" ( "max_burn_amount" )
```

Parameters:

- `yuv-transaction` - [YUV transaction] serialized in HEX or JSON format.
- `max_burn_amount` - optional unsigned integer the maximum amount of Bitcoin in satoshis to burn. If unspecified, no burn amount limit is applied.

> [!NOTE]
> Currently it's possible to send JSON serialized YUV transactions, but soon this method will only
> accept HEX encoded YUV transactions.

Returns:

`boolean` - `true` if sent successfully.

> [!NOTE]
> Returns `true` if sent to the Bitcoin node successfully, otherwise an error will be returned.

Example:

```shell
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":0,"method":"sendrawyuvtransaction","params":["01000000000102ccad3b0ace9ef14ddd105792deb1d46b771691c219e995771a58589c512c83f30100000000feffffff890f95bd243fd3e4ddda62173aa0f7e69c31874b55dcd3b60b9c4529eda35c2b0100000000feffffff03e8030000000000001600149bc9216986b8877ea5d5c076c28df95548865e70e8030000000000001600141038c58013308287767d11ec4631686bb362428df7dbf505000000001600145ae96a8a5a39ac3efb331d8372f9003a22ebb8a302473044022076ff76196007999c6d4288f22f385a04ab7f926e205ec1d914b4d0fad77c093202205d20ce3c960ea386141c80682048f0c4c15ce0a1c7fdc8864d46a3a1463e8d5a012102c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a2302473044022015bc7ca7b939d5982fcf8ada3c90bfad6e175822fbef54f3e134d1fcd7bb95b202201cf2d629a6496bb7dd65e3447bb703fc2fac82dbed70ac177e1c91ac7e8b1d9c012103d1ebea96e0c05b91c04330d3791db2370a3f5efb0fad913777d9cce83b941f2b6e00000001010000000100000000000000000000000000000000000027100000000000000000000000000000000086b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f02c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a23030000000000000000000000000000000000000000000013880000000000000000000000000000000086b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f0286b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f0100000000000000000000000000000000000013880000000000000000000000000000000086b00b8679dc75ff5b1cc9ed2fa07c58780b0e8b4b7a334e470e2ea3f9fd005f02c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a23020000000502c515be9647504e94bfdc20c4629843d80ee27777b065c2af79fa97cc0eff4a23", 1000000]}' \ 
    http://127.0.0.1:18333
```

#### [`sendyuvtransaction`]

Send a JSON serialized YUV transaction to the YUV node and broadcast it to the Bitcoin network. Once the transaction is confirmed, the YUV node will check and attach it if it's valid.

```
sendyuvtransaction "yuv-transaction" ( "max_burn_amount" )
```

Parameters:

- `yuv-transaction` - [YUV transaction] serialized in JSON format.
- `max_burn_amount` - optional unsigned integer the maximum amount of Bitcoin in satoshis to burn. If unspecified, no burn amount limit is applied.

Returns:

`boolean` - `true` if sent successfully.

> [!NOTE]
> Returns `true` if sent to the Bitcoin node successfully, otherwise an error will be returned.

Example:

```shell
curl -X POST \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"sendyuvtransaction","params":[{"bitcoin_tx":{"version":1,"lock_time":315,"input":[{"previous_output":"9ea621f64b8d64ebe3430e2212caa9b77175825cd3fc0c800ab9e30f03736cec:1","script_sig":"","sequence":4294967294,"witness":["304402203c50474c2ba73b0b00d3e660d05bfd1edb2fc056995c45acba5181eab21f7c19022049599c14c20aa85da311536abd9967825d6b9f36eafc67429a112efe6d7f57fa01","025510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"]}],"output":[{"value":10000,"script_pubkey":"001416648ddda83c0322c36b889dd32a8be3eb828553"},{"value":99918624,"script_pubkey":"001429999f2fa94a248eff7187471847dd9fa47c02dc"}]},"tx_type":{"type":"Issue","data":{"output_proofs":{"0":{"type":"Sig","data":{"pixel":{"luma":{"amount":1111},"chroma":"5510996bdb5271f84896eb42ea5b6c4ba3bd96f90a605c70a7f2b402f0afdad0"},"inner_key":"027bf59465bf6cb3faa969e963c6934a2bee2b38c5d981c0b2226ed669149945db", 500000}}}}}}]}' \
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
[`sendyuvtransaction`]: #sendyuvtransaction
[`isyuvtxoutfrozen`]: #isyuvtxoutfrozen
[`emulateyuvtransaction`]: #emulateyuvtransaction
[`getrawyuvtransaction`]: #getrawyuvtransaction

[YUV transaction]: ../crates/types/src/transactions/mod.rs#L16
