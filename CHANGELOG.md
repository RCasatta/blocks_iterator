# Changelog

## Unreleased

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