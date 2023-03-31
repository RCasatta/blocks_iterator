[![MIT license](https://img.shields.io/github/license/RCasatta/blocks_iterator)](https://github.com/RCasatta/blocks_iterator/blob/master/LICENSE)
[![Crates](https://img.shields.io/crates/v/blocks_iterator.svg)](https://crates.io/crates/blocks_iterator)
[![Docs](https://img.shields.io/badge/docs.rs-blocks_iterator-green)](https://docs.rs/blocks_iterator)

# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in height order, it avoids following reorgs (see `max_reorg` parameter)
* Blocks come with [metadata](https://docs.rs/blocks_iterator/latest/blocks_iterator/struct.BlockExtra.html) like all block's previous outputs, it allows computing transactions fee or [verifying](examples/verify.rs) scripts in the blockchain.

## Iteration modes

### In rust programs

Used as a library blocks could be iterated via the [`blocks_iterator::iter`](https://docs.rs/blocks_iterator/latest/blocks_iterator/fn.iter.html) method like done in [outputs_versions](examples/outputs_versions.rs) example.

When the task to be performed is computational costly, like verifying spending conditions, it is suggested to parallelize the execution like it's done with rayon
(or similar) in the [verify](examples/verify.rs) example (note `par_bridge()` call).

### Through Pipes

Other than inside Rust programs, ordered blocks with previous outputs could be iterated using Unix pipes.

```
$ cargo build --release 
$ cargo build --release --examples
$ ./target/release/blocks_iterator --blocks-dir /Volumes/Transcend/bitcoin-testnet/testnet3/blocks --network testnet --max-reorg 40 | ./target/release/examples/most_output_pipe | ./target/release/examples/heaviest_pipe >/dev/null
...
[2021-10-21T10:10:24Z INFO  most_output_pipe] most_output tx is d28305817238ee92e5d9ac0d81c3bf5ecf7399528e6d87226d726e4070c7e665 with #outputs: 30001
[2021-10-21T10:10:24Z INFO  heaviest_pipe] heaviest tx is 73e64e38faea386c88a578fd1919bcdba3d0b3af7b6302bf6ee1b423dc4e4333 with weight: 3999608
```

## Memory requirements and performance

Running (`cargo run --release -- --network X --blocks-dir Y >/dev/null`) on threadripper 1950X, Testnet @ 2130k, Mainnet @ 705k. Spinning disk. Take following benchmarks with a grain of salt since they refer to older versions.

| Network | `--skip--prevout` | `--max-reorg` | `utxo-db` | Memory | Time    |
|---------|-------------------|---------------|----------:|-------:|--------:|
| Mainnet | true              |           6   | no        |   33MB |  1h:00m |
| Mainnet | false             |           6   | no        |  5.3GB |  1h:29m |
| Mainnet | false             |           6   | 1 run     |  201MB |  9h:42m |
| Mainnet | false             |           6   | 2 run     |  113MB |  1h:05m |
| Testnet | true              |           40  | no        |  123MB |  3m:03s |
| Testnet | false             |           40  | no        |  1.4GB |  8m:02s |
| Testnet | false             |           40  | 1 run     |  247MB | 16m:12s |
| Testnet | false             |           40  | 2 run     |  221MB |  8m:32s |

## Doc

To build docs:

```sh
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --all-features --open
```

## Examples

Run examples with:

```
cargo run --release --example verify
```

* [heaviest](examples/heaviest_pipe.rs) find the transaction with greatest weight
* [most_output](examples/most_output_pipe.rs) find the transaction with most output
* [outputs_versions](examples/outputs_versions.rs) Count outputs witness version
* [signatures_in_witness](examples/signatures_in_witness.rs) Count signatures in witness
* [verify](examples/verify.rs) verify transactions in blocks using libbitcoin-consensus. Consumers are run in parallel fashion.

## Version 1.0 meaning

The `1.0` is not to be intended as *battle-tested production-ready* library, the binary format of 
`BlockExtra` changed and I wanted to highlight it with major version rollout.

## Similar projects

* [bitcoin-iterate](https://github.com/rustyrussell/bitcoin-iterate) this project inspired blocks_iterator, the differences are:
  * It is C-based
  * It's more suited for shell piping, while blocks_iterator can be used with piping but also as a rust library
  * It doesn't give previous outputs information
  * It is making two passage over `blocks*.dat` serially, while blocks_iterator is doing two passes in parallel.
* [rust-bitcoin-indexer](https://github.com/dpc/rust-bitcoin-indexer) this project requires longer setup times (about 9 hours indexing) and a postgre database, once the indexing is finished it allows fast queries on relational database.
* [electrs](https://github.com/romanz/electrs) specifically intended for the Electrum protocol


## MSRV 

Check minimum rust version run in CI (as of Mar 2023 is stable `1.66.0`)
