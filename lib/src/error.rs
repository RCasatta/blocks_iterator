#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The `xor.dat` file in the blocks directory exists but can't be read or has an unexpected
    /// length (it must be 8 bytes)
    #[error("invalid blocks dir XOR key {path:?}: {source}")]
    XorKey {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[cfg(feature = "redb")]
    #[error(transparent)]
    Redb(#[from] bitcoin_slices::redb::Error),

    #[cfg(feature = "db")]
    #[error(transparent)]
    Rocksdb(#[from] rocksdb::Error),

    #[error("You can use only one db at a time")]
    OneDb,
}
