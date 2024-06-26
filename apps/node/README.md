# `yuv-node`

An implementation of YUV-node.

## Usage

For example, lets setup two working YUV nodes:

Setup configuration file for first node:

``` toml
# config-1.toml
# Network type used in p2p and other crates.
# Accepting values: mainnet, bitcoin, testnet, regtest, sigtest, mutiny
network = "regtest"

[p2p]
address = "0.0.0.0:8002" # address on which node will listen p2p connections
max_inbound_connections = 16 # maximum number of inbound connections
max_outbound_connections = 8 # maximum number of outbound connections
bootnodes = [] # list of ip addresses of nodes to connect

[rpc]
address = "127.0.0.1:18337" # address on which RPC API will be served.
max_items_per_request = 1 # items limitation in the list requests
max_request_size_kb = 20480 # Optional: max size of request in kilobytes (default: 20480, which is 20 megabytes)

[storage]
path = "./.yuvd/node-1" # path to directory with stored txs.
create_if_missing = true # Create database if missing with all missing directories in path
tx_per_page = 100 # Number of transactions per one page return by `getlistrawyuvtransactions`
flush_period = 100 # responds for the saving data period (in sececonds) 

[checkers]
pool_size = 4 # how many checker workers will node have

[bnode]
url = "http://127.0.0.1:18443" # url to bitcoin node
auth = { username = "admin1", password = "123" } # bitcoin node auth

[logger]
level = "INFO" # level logging, accepting values: TRACE, DEBUG, INFO, WARN, ERROR

[indexer]
# blockhash from which the indexer indexes blocks
starting_block = "000000000000000000027e245190ea0b27c4eb344618816fbdd8b5eec8e234d3"
polling_period = { secs = 5, nanos = 0 } # interval between indexer runs
# max time after each transaction should be discarded from pool
max_confirmation_time = { secs = 86400, nanos = 0 } 
blockloader = { 
    workers_number = 10, # number of workers which load blocks
    buffer_size = 50, # Number of blocks that will be fetched by the block loader in each iteration
    worker_time_sleep = 3 # Sleep the worker for seconds when the worker exceeds the rate limit
}

[controller]
max_inv_size = 100 # max number of txs in inv message
inv_sharing_interval = 10 # interval between inv messages
```

And run:

``` sh
cargo run -p yuv-node -- run --config ./config-1.toml
```

Setup configuration file for second node:

``` toml
# config-2.toml
# Network type used in p2p and other crates.
# Accepting values: mainnet, bitcoin, testnet, regtest, sigtest, mutiny
network = "regtest"

[p2p]
port = 8002 # if settuping locally, bumping port here
max_inbound_connections = 16 # maximum number of inbound connections
max_outbound_connections = 8 # maximum number of outbound connections
bootnodes = ["127.0.0.1:8001"] # address of first node

[rpc]
address = "127.0.0.1:18334" # bumping port here also

[storage]
path = "./.yuvd/node-2" # another path to directory with stored txs.

[bnode]
url = "127.0.0.1:18443"
auth = { username = "admin1", password = "123" }
```

And run:

``` sh
cargo run -p yuv-node -- run --config ./config-2.toml
```
