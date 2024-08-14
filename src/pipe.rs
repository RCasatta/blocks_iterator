use crate::bitcoin::consensus::{Decodable, Encodable};
use crate::BlockExtra;
use std::io::{self, Read, Write};

const MAX_BLOCK_EXTRA_SIZE: usize = 10 * 1024 * 1024;

/// Iterator to use un Unix-style pipe composition when receiving BlockExtra from stdin and
/// optionally propogating those to stdout
pub struct PipeIterator {
    stdin: io::Stdin, // from docs, stdin is buffered, non need to wrap in BufReader
    stdout: Option<io::Stdout>,
    buffer: Vec<u8>,
}

impl PipeIterator {
    /// Creates new PipeIterator from stdin and stdout
    pub fn new(stdin: io::Stdin, stdout: Option<io::Stdout>) -> Self {
        let buffer = if stdout.is_some() {
            vec![0u8; MAX_BLOCK_EXTRA_SIZE]
        } else {
            Vec::new()
        };
        PipeIterator {
            stdin,
            stdout,
            buffer,
        }
    }
}

impl bitcoin::io::Read for PipeIterator {
    fn read(&mut self, buf: &mut [u8]) -> bitcoin::io::Result<usize> {
        Ok(self.stdin.read(buf)?)
    }
}

impl bitcoin::io::BufRead for PipeIterator {
    fn fill_buf(&mut self) -> bitcoin::io::Result<&[u8]> {
        let bytes_read = self.stdin.read(&mut self.buffer)?;
        Ok(&self.buffer[..bytes_read])
    }

    fn consume(&mut self, _amount: usize) {
        todo!()
    }
}

impl Iterator for PipeIterator {
    type Item = BlockExtra;

    fn next(&mut self) -> Option<Self::Item> {
        let block_extra = BlockExtra::consensus_decode(self).ok()?;

        if let Some(stdout) = self.stdout.as_mut() {
            // using StreamReader we can't send received bytes directly to stdout, thus we need to
            // re-serialize back
            let len = block_extra
                .consensus_encode(&mut &mut self.buffer[..])
                .unwrap(); // buffer is big enough, we can unwrap
            stdout.write_all(&self.buffer[..len]).unwrap();
        }

        Some(block_extra)
    }
}
