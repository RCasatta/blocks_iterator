# Changelog

## Release 3.0.0 - unreleased

### Added

- Read blocks directories XOR-obfuscated by Bitcoin Core 28.0+ (`-blocksxor`, on by default).
  The key is read from `<blocks_dir>/xor.dat`; a missing file or an all-zero key means the block
  files are read as-is.
- `try_iter(config)`: like `iter(config)` but returns `Result`, so an invalid `xor.dat` can be
  handled by the caller.
- `Error::XorKey { path, source }`, returned when `xor.dat` can't be read or isn't 8 bytes long.

### Changed

- BREAKING: `FsBlock` is no longer public (it's an internal pipeline type), and the public
  `impl TryFrom<FsBlock> for BlockExtra` is removed with it. `XorFile`, which existed only on
  `master` and was reachable only through `FsBlock::file`, is crate-private.
- BREAKING: `Error` is now `#[non_exhaustive]` and has the new `XorKey` variant, so matches on `Error` need a wildcard arm.
- `iter(config)` panics with the error message (instead of panicking inside a pipeline thread)
  when `xor.dat` is invalid.
- The CLI uses `try_iter` and exits with status 1 on an invalid `xor.dat`.
- MSRV: Rust 1.78 without default features, 1.85 with the `db` feature (rocksdb 0.24).

### Fixed

- CI: moved off the retired `ubuntu-20.04` runners (jobs were cancelled after 24h waiting for a
  runner), added job timeouts, and the pipe example now runs with `pipefail`.
- clippy and `dead_code` errors on current toolchains.
- Dependency advisories, via `cargo update`: RUSTSEC-2026-0204 (crossbeam-epoch),
  RUSTSEC-2025-0055 (tracing-subscriber), RUSTSEC-2026-0097 (rand).

## Releases 0.12.2 to 2.1.2

Not recorded in this file. See the git history and the tags (`release_*`, `lib_*`, `cli_*`,
`1.0.4`, `2.1.1`; there is no tag for 2.1.2, which was published from lib `f319b72` and cli
`37412d5`).

## After 0.12.1 (written as "Unreleased" in 455fc0f, 2022-04-25)

### Removed

- `par_iter` is deprecated beacuse you can getter better composable results by simply concateneting
  methods on the iterator, like `iter(config).flat_map(pre_proc).par_bridge().for_each(task)`

## Release 0.12.1 - 2022-04-23

### Changed

- `blocks_iterator::par_iter` accepts an `Arc<STATE>` instead of a `STATE` so it can be used after
  the call

## Release 0.12.0 - 2022-04-23

### Added

- `blocks_iterator::iter` returns a proper iterator
- `blocks_iterator::par_iter` to iterate in parallel fashion

### Changed

- `iterate` method become private in favor a real iterator through `iter` method
- UTXO db format changed, it use more space but it allows to compute the UTXO set
- By default all features are enabled, exclude default features to decrease building times
- Increase MSRV to `1.56.1`
- bitcoin dep increased to `0.28.0`
- in `PipeIterator` streaming to stdout become optional, since it cost an additional serialization
  that one could avoid by forking previous stdout with the unix `tee` command