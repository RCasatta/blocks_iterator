`blk-testnet.dat` is the first 100kbytes of a `blk00000.dat` of a testnet instance. It's useful to launch a quick run like:

```
RUST_LOG=debug cargo run --example iterate -- --blocks-dir blocks --network testnet
```