[![MIT license](https://img.shields.io/github/license/RCasatta/blocks_iterator)](https://github.com/RCasatta/blocks_iterator/blob/master/LICENSE)
[![Crates](https://img.shields.io/crates/v/blocks_iterator.svg)](https://crates.io/crates/blocks_iterator)
[![Docs](https://img.shields.io/badge/docs.rs-blocks_iterator-green)](https://docs.rs/blocks_iterator)

# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in height order, it avoids following reorgs (see [`Config::max_reorg`] parameter)
* Blocks come with extra data [`BlockExtra`] like all block's previous outputs, it allows computing 
transactions fee or [verifying](https://github.com/RCasatta/blocks_iterator/blob/master/examples/verify.rs) 
scripts in the blockchain.

Note:

Bitcoin Core 28.0 introduced xoring of bitcoin blocks and this project doesn't yet support reading the blocks directory when xored. You can disable xoring in core via `-blocksxor=0`.

## Iteration modes

### In rust programs

Used as a library blocks could be iterated via the [`iter()`] method like:

```rust
// "blocks" dir contains first 400 testnet blocks
let conf = blocks_iterator::Config::new("blocks", bitcoin::Network::Testnet);
let mut total_fee = 0u64;
for b in blocks_iterator::iter(conf) {
  total_fee += b.fee().expect("fee available cause we are keeping prevouts");
}

// Only a bunch of tx with fee exists on testnet with height < 400
// in blocks: 385, 387, 389, 390, 392, 394
assert_eq!(total_fee, 450_000u64);
```

When the task to be performed is computational costly, like verifying spending conditions, it is 
suggested to parallelize the execution like it's done with rayon (or similar) in the 
[verify](https://github.com/RCasatta/blocks_iterator/blob/master/examples/verify.rs) example 
(note `par_bridge()` call).

### Through Pipes

Other than inside Rust programs, ordered blocks with previous outputs could be iterated using Unix pipes.

```sh
$ cargo build --release 
$ cargo build --release --examples
$ ./target/release/blocks_iterator --blocks-dir ~/.bitcoin/testnet3/blocks --network testnet --max-reorg 40 --stop-at-height 200000 | ./target/release/examples/with_pipe
...
[2023-03-31T15:01:23Z INFO  with_pipe] Max number of txs: 6287 block: 0000000000bc915505318327aa0f18568ce024702a024d7c4a3ecfe80a893d6c
[2023-03-31T15:01:23Z INFO  with_pipe] total missing reward: 50065529986 in 100 blocks
[2023-03-31T15:01:23Z INFO  with_pipe] most_output tx is 640e22b5ddee1f6d2d701e37877027221ba5b36027634a2e3c3ee1569b4aa179 with #outputs: 10001
```

If you have more consumer process you can concatenate pipes by passing stdout to `PipeIterator::new` or using `tee` utility to split the stdout of blocks_iterator. The latter is better because it doesn't require re-serialization of the data.

## Memory requirements and performance

Running (`cargo run --release -- --network X --blocks-dir Y >/dev/null`) on threadripper 1950X, 
Testnet @ 2130k, Mainnet @ 705k. Spinning disk. Take following benchmarks with a grain of salt 
since they refer to older versions.

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

```sh
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

Check minimum rust version run in CI, as of Aug 2023 is:

* `1.60.0` without features (needs some pinning, check CI)
* `1.67.0` with features.
