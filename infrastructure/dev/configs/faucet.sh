#!/bin/sh
sleep 10
echo "Faucet is running"
bitcoin-cli createwallet yuv-faucet
bitcoin-cli loadwallet yuv-faucet
while true; do bitcoin-cli -regtest -generate 1; sleep 30; done
