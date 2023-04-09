#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Redb(#[from] bitcoin_slices::redb::Error),

    #[error("You can use only one db at a time")]
    OneDb,
}
