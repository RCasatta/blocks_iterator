use std::fmt;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The `xor.dat` file in the blocks directory exists but can't be read or has an unexpected
    /// length (it must be 8 bytes)
    #[error("invalid blocks dir XOR key {path:?}")]
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

impl Error {
    /// Display this error followed by its causes, separated by `": "`.
    ///
    /// The `Display` of [`Error`] doesn't include the underlying cause, which is available via
    /// [`std::error::Error::source`], use this to show it, eg. `error!("{}", e.display_chain())`.
    pub fn display_chain(&self) -> impl fmt::Display + '_ {
        DisplayChain(self)
    }
}

struct DisplayChain<'a>(&'a Error);

impl fmt::Display for DisplayChain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = std::error::Error::source(self.0);
        while let Some(s) = source {
            write!(f, ": {}", s)?;
            source = s.source();
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::Error;
    use std::io;

    #[test]
    fn display_chain() {
        let e = Error::XorKey {
            path: "xor.dat".into(),
            source: io::Error::new(io::ErrorKind::InvalidData, "unexpected length: 3"),
        };
        assert_eq!(e.to_string(), "invalid blocks dir XOR key \"xor.dat\"");
        assert_eq!(
            e.display_chain().to_string(),
            "invalid blocks dir XOR key \"xor.dat\": unexpected length: 3"
        );

        let e = Error::OneDb;
        assert_eq!(e.display_chain().to_string(), e.to_string());
    }
}
