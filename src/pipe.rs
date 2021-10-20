use crate::bitcoin::consensus::{Decodable, Encodable};
use crate::BlockExtra;
use std::io;
use std::io::Write;

const MAX_BLOCK_EXTRA_SIZE: usize = 10 * 1024 * 1024;

/// Iterator to use un Unix-style pipe composition when receiving BlockExtra from stdin and
/// propogating those to stdout
pub struct PipeIterator {
    stdin: io::Stdin, // from docs, stdin is buffered, non need to wrap in BufReader
    stdout: io::Stdout,
    buffer: Vec<u8>,
}

impl PipeIterator {
    /// Creates new PipeIterator from stdin and stdout
    pub fn new(stdin: io::Stdin, stdout: io::Stdout) -> Self {
        PipeIterator {
            stdin,
            stdout,
            buffer: Vec::with_capacity(MAX_BLOCK_EXTRA_SIZE),
        }
    }
}

impl Iterator for PipeIterator {
    type Item = BlockExtra;

    fn next(&mut self) -> Option<Self::Item> {
        let block_extra = BlockExtra::consensus_decode(&mut self.stdin).ok()?;

        // using StreamReader we can't send received bytes directly to stdout, thus we need to
        // re-serialize back

        let len = block_extra.consensus_encode(&mut self.buffer[..]).unwrap(); // buffer is big enough, we can unwrap
        self.stdout.write_all(&self.buffer[..len]).unwrap();
        Some(block_extra)
    }
}
