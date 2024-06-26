# `e2e-test`

An end-to-end test for the YUV protocol.
Do not run with more than 2000 blocks mined on Regtest, as it will never finish.

## Usage

For example, lets setup a test with five working YUV nodes, one Bitcoin node and 20 accounts:

Setup configuration file for the test will look as follows:

``` toml
duration = { secs = 500, nanos = 0 }

[nodes]
# List of YUV nodes that will be randomly distributed among the accounts. 
# *At least one required.
yuv = ["http://127.0.0.1:18333"]
# List of Bitcoin nodes that will be randomly distributed among the accounts. 
# *At least one required.
bitcoin = [
    { url = "http://127.0.0.1:18443", auth = { username = "admin1", password = "123" } },
]
# Esplora url. 
# If not specified, `accounts.threshold` must be set to 1.0.
esplora = ["http://127.0.0.1:30000"]

[accounts]
# Number of account to generate.
number = 5
# Percent of accounts connected to Bitcoin nodes. Other nodes are connected to Esplora.
# Accepts values in range (0;1]. 
# NOTE: Esplora feature is experimental, setting threshold to 1.0 disables it, i.e. all the accounts will be connected to Bitcoin RPC.
threshold = 1.0
# Defines how often should the faucet fund the account.
funding_interval = 60

[checker]
# Defines the threshold needed to initiate a tx check (number of accounts)
# For example, if threshold is 20, the tx check will start when there are at least 20 transactions broadcasted.
threshold = 20
# Experimental: will count the expected balances and compare it to the actual balances in the end of the test.
# The balances often don't match because of the bad synchronization.
check_balances_matching = false

[miner]
interval = { secs = 1, nanos = 0 }

[report]
result_path = ".result.dev.txt"
error_log_file = ".logs.dev.log"
```

Make sure the nodes are running and execute the following command:

``` sh
cargo run --release -p e2e-test -- run --config e2e.dev.toml 
```
