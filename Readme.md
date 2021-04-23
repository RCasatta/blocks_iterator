[![MIT license](https://img.shields.io/github/license/RCasatta/blocks_iterator)](https://github.com/RCasatta/blocks_iterator/blob/master/LICENSE)
[![Crates](https://img.shields.io/crates/v/blocks_iterator.svg)](https://crates.io/crates/blocks_iterator)

# Blocks iterator

Iterates over Bitcoin blocks, decoding data inside Bitcoin Core's blocks directory.

Features:
* Blocks are returned in order, it avoids following reorgs (see `max_reorg` parameter)
* Blocks come with metadata like all block's previous outputs, it allows computing transactions fee.
* Scan takes about 1 hour for mainnet (at block 680000) and 5 minutes on testnet (at block 1972337)

Handling fee computation in memory requires good amount of ram, about 10GB for testnet and 20GB for mainnet.
If you are not interested in previous outputs use `--skip-prevout`, ram requirements will be about 1GB for tesnet and 2GB for mainnet.

# Example

See [iterate](examples/iterate.rs) example