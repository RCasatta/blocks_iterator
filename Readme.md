[![MIT license](https://img.shields.io/github/license/RCasatta/blocks_iterator)](https://github.com/RCasatta/blocks_iterator/blob/master/LICENSE)
[![Crates](https://img.shields.io/crates/v/blocks_iterator.svg)](https://crates.io/crates/blocks_iterator)

# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in height order, it avoids following reorgs (see `max_reorg` parameter)
* Blocks come with [metadata](https://docs.rs/blocks_iterator/0.1.0/blocks_iterator/struct.BlockExtra.html) like all block's previous outputs, it allows computing transactions fee.

## Memory requirements and performance

Running on threadripper 1950X, Testnet @ 1970k, Mainnet @ 681k

| Network | `--skip--prevout` | Memory | Time   |
|---------|-------------------|-------:|-------:|
| Mainnet | false             | 10.3GB | 1h:00m |
| Mainnet | true              |  2.7GB |    36m |
| Testnet | false             |  3.2GB |     5m |
| Testnet | true              |  1.0GB |     3m |

## Example

See [iterate](examples/iterate.rs) example