use std::convert::TryInto;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const XOR_KEY_LEN: usize = 8;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct XorKey([u8; XOR_KEY_LEN]);

impl XorKey {
    pub(crate) fn new(bytes: [u8; XOR_KEY_LEN]) -> Self {
        Self(bytes)
    }

    pub(crate) fn read_from(blocks_dir: &Path) -> io::Result<Option<Self>> {
        let path = blocks_dir.join("xor.dat");
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
pub struct XorFile {
    file: File,
    xor_key: Option<XorKey>,
}

impl From<File> for XorFile {
    fn from(file: File) -> Self {
        Self {
            file,
            xor_key: None,
        }
    }
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
