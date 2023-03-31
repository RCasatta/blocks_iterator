use bitcoin_slices::redb;

/// crate error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "db")]
    #[error(transparent)]
    Rocksdb(#[from] rocksdb::Error),

    #[error(transparent)]
    Redb(#[from] redb::Error),

    #[error(transparent)]
    Bitcoin(#[from] bitcoin::Error),

    #[error("Rocksdb and Redb cannot be specified together")]
    OneDb,
}
