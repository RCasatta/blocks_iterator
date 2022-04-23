# Changelog

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