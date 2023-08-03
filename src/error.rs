#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "redb")]
    #[error(transparent)]
    Redb(#[from] bitcoin_slices::redb::Error),

    #[cfg(feature = "db")]
    #[error(transparent)]
    Rocksdb(#[from] rocksdb::Error),

    #[error("You can use only one db at a time")]
    OneDb,
}
