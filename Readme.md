[![MIT license](https://img.shields.io/github/license/RCasatta/blocks_iterator)](https://github.com/RCasatta/blocks_iterator/blob/master/LICENSE)
[![Crates](https://img.shields.io/crates/v/blocks_iterator.svg)](https://crates.io/crates/blocks_iterator)

# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in height order, it avoids following reorgs (see `max_reorg` parameter)
* Blocks come with [metadata](https://docs.rs/blocks_iterator/latest/blocks_iterator/struct.BlockExtra.html) like all block's previous outputs, it allows computing transactions fee or [verifying](examples/verify.rs) scripts in the blockchain.

## Memory requirements and performance

Running [iterate](examples/iterate.rs) example on threadripper 1950X, Testnet @ 2090k, Mainnet @ 709k. Spinning disk.

| Network | `--skip--prevout` | `--max-reorg` | Memory | Time   |
|---------|-------------------|---------------|-------:|-------:|
| Mainnet | false             |            6  |  8.3GB | 1h:15m |
| Mainnet | true              |            6  |  157MB | 1h:00m |
| Testnet | false             |           40  |  2.4GB | 5m:12s |
| Testnet | true              |           40  |  221MB | 5m:12s |

## Examples

Run examples with:

```
cargo run --release --example iterate
```

* [iterate](examples/iterate.rs) iterate over blocks and print block fee
* [heaviest](examples/heaviest.rs) find the transaction with greatest weight
* [most_output](examples/most_output.rs) find the transaction with most output
* [verify](examples/verify.rs) verify transactions

## Similar projects

* [bitcoin-iterate](https://github.com/rustyrussell/bitcoin-iterate) this project inspired blocks_iterator, the differences are:
  * bitcoin-iterate is C-based
  * bitcoin-iterate it's more suited for shell piping, while blocks_itearator is intended to use as a rust library
  * bitcoin-iterate doesn't give previous outputs information
  * bitcoin-iterate is making two passage over `blocks*.dat` while blocks_iterator is doing one pass keeping out-order-blocks in memory (the latter is faster at the expense of higher memory usage)
* [rust-bitcoin-indexer](https://github.com/dpc/rust-bitcoin-indexer) this project requires longer setup times (about 9 hours indexing) and a postgre database, once the indexing is finished it allows fast queries on relational database.
* [electrs](https://github.com/romanz/electrs) specifically intended for the Electrum protocol
