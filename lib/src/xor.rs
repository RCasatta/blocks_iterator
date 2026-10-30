use std::convert::TryInto;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const XOR_KEY_LEN: usize = 8;
const XOR_FILE_NAME: &str = "xor.dat";

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct XorKey([u8; XOR_KEY_LEN]);

impl XorKey {
    pub(crate) fn new(bytes: [u8; XOR_KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// Read the key from `xor.dat` in `blocks_dir`.
    ///
    /// Returns `None` if the file doesn't exist (Bitcoin Core < 28.0) or if the key is all
    /// zeros (`-blocksxor=0`), in both cases block files are not obfuscated.
    pub(crate) fn read_from(blocks_dir: &Path) -> Result<Option<Self>, crate::Error> {
        let path = blocks_dir.join(XOR_FILE_NAME);
        Self::read_path(&path).map_err(|source| crate::Error::XorKey { path, source })
    }

    fn read_path(path: &Path) -> io::Result<Option<Self>> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        let bytes = bytes.try_into().map_err(|bytes: Vec<u8>| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("xor.dat unexpected length: {}", bytes.len()),
            )
        })?;
        if bytes == [0u8; XOR_KEY_LEN] {
            return Ok(None);
        }
        Ok(Some(Self::new(bytes)))
    }

    pub(crate) fn apply(self, offset: usize, bytes: &mut [u8]) {
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte ^= self.0[(i + offset) & (XOR_KEY_LEN - 1)];
        }
    }
}

/// A Bitcoin Core block file handle that transparently undoes blocksdir XOR encoding.
#[derive(Debug)]
pub(crate) struct XorFile {
    file: File,
    xor_key: Option<XorKey>,
}

impl XorFile {
    pub(crate) fn open(path: &Path, xor_key: Option<XorKey>) -> io::Result<Self> {
        Ok(Self {
            file: File::open(path)?,
            xor_key,
        })
    }

    pub(crate) fn read_to_end(&mut self, bytes: &mut Vec<u8>) -> io::Result<usize> {
        let offset = bytes.len();
        let read = self.file.read_to_end(bytes)?;
        if let Some(xor_key) = self.xor_key {
            xor_key.apply(offset, &mut bytes[offset..]);
        }
        Ok(read)
    }

    pub(crate) fn read_exact_at(&mut self, offset: usize, bytes: &mut [u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(offset as u64))?;
        self.file.read_exact(bytes)?;
        if let Some(xor_key) = self.xor_key {
            xor_key.apply(offset, bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::XorKey;
    use std::fs;

    #[test]
    fn xor_key_missing_file() {
        let tempdir = tempfile::TempDir::new().unwrap();
        assert_eq!(XorKey::read_from(tempdir.path()).unwrap(), None);
    }

    #[test]
    fn xor_key_all_zero() {
        let tempdir = tempfile::TempDir::new().unwrap();
        fs::write(tempdir.path().join("xor.dat"), [0u8; 8]).unwrap();
        assert_eq!(XorKey::read_from(tempdir.path()).unwrap(), None);
    }

    #[test]
    fn xor_key_valid() {
        let tempdir = tempfile::TempDir::new().unwrap();
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8];
        fs::write(tempdir.path().join("xor.dat"), bytes).unwrap();
        assert_eq!(
            XorKey::read_from(tempdir.path()).unwrap(),
            Some(XorKey::new(bytes))
        );
    }

    #[test]
    fn xor_key_wrong_length() {
        let tempdir = tempfile::TempDir::new().unwrap();
        for len in [0usize, 3, 7, 9, 16] {
            fs::write(tempdir.path().join("xor.dat"), vec![1u8; len]).unwrap();
            let err = XorKey::read_from(tempdir.path()).unwrap_err();
            match err {
                crate::Error::XorKey { source, .. } => {
                    assert_eq!(source.kind(), std::io::ErrorKind::InvalidData, "len {len}")
                }
                e => panic!("unexpected error {:?}", e),
            }
        }
    }

    #[test]
    fn xor_key_unreadable() {
        let tempdir = tempfile::TempDir::new().unwrap();
        // a directory named xor.dat can't be read as a file
        fs::create_dir(tempdir.path().join("xor.dat")).unwrap();
        let err = XorKey::read_from(tempdir.path()).unwrap_err();
        assert!(matches!(err, crate::Error::XorKey { .. }));
    }

    #[test]
    fn xor_key_applies_from_offset() {
        let key = XorKey::new([1, 2, 3, 4, 5, 6, 7, 8]);
        let mut whole = b"abcdefghijkl".to_vec();
        let mut slice = whole[3..10].to_vec();

        key.apply(0, &mut whole);
        key.apply(3, &mut slice);

        assert_eq!(slice, whole[3..10]);
    }
}
