use crate::bitcoin::consensus::encode::Error;
use crate::bitcoin::consensus::{Decodable, Encodable};
use crate::bitcoin::{Block, BlockHash, OutPoint, Transaction, TxOut};
use crate::FsBlock;
use log::debug;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::ops::DerefMut;

/// The bitcoin block and additional metadata returned by the [iterate] method
#[derive(Debug, Eq, PartialEq)]
pub struct BlockExtra {
    /// The bitcoin block
    pub block: Block,
    /// The bitcoin block hash, same as `block.block_hash()` but result from hashing is cached
    pub block_hash: BlockHash,
    /// The byte size of the block, as returned by in `serialize(block).len()`
    pub size: u32,
    /// Hash of the blocks following this one, it's a vec because during reordering they may be more
    /// than one because of reorgs, as a result from [iterate], it's just one.
    pub next: Vec<BlockHash>,
    /// The height of the current block, number of blocks between this one and the genesis block
    pub height: u32,
    /// All the previous outputs of this block. Allowing to validate the script or computing the fee
    /// Note that when configuration `skip_script_pubkey` is true, the script is empty,
    /// when `skip_prevout` is true, this map is empty.
    pub outpoint_values: HashMap<OutPoint, TxOut>,
}

impl TryFrom<FsBlock> for BlockExtra {
    type Error = String;

    fn try_from(fs_block: FsBlock) -> Result<Self, Self::Error> {
        let err = |e: String, f: &FsBlock| -> String { format!("{:?} {:?}", e, f) };
        let mut guard = fs_block
            .file
            .lock()
            .map_err(|e| err(e.to_string(), &fs_block))?;
        let file = guard.deref_mut();
        file.seek(SeekFrom::Start(fs_block.start as u64))
            .map_err(|e| err(e.to_string(), &fs_block))?;
        debug!("going to read: {:?}", file);
        let reader = BufReader::new(file);
        Ok(BlockExtra {
            block: Block::consensus_decode(reader).map_err(|e| err(e.to_string(), &fs_block))?,
            block_hash: fs_block.hash,
            size: (fs_block.end - fs_block.start) as u32,
            next: fs_block.next,
            height: 0,
            outpoint_values: Default::default(),
        })
    }
}

impl BlockExtra {
    /// Returns the average transaction fee in the block
    pub fn average_fee(&self) -> Option<f64> {
        Some(self.fee()? as f64 / self.block.txdata.len() as f64)
    }

    /// Returns the total fee of the block
    pub fn fee(&self) -> Option<u64> {
        let mut total = 0u64;
        for tx in self.block.txdata.iter() {
            total += self.tx_fee(tx)?;
        }
        Some(total)
    }

    /// Returns the fee of a transaction contained in the block
    pub fn tx_fee(&self, tx: &Transaction) -> Option<u64> {
        let output_total: u64 = tx.output.iter().map(|el| el.value).sum();
        let mut input_total = 0u64;
        for input in tx.input.iter() {
            input_total += self.outpoint_values.get(&input.previous_output)?.value;
        }
        Some(input_total - output_total)
    }

    /// return the base block reward in satoshi
    pub fn base_reward(&self) -> u64 {
        let initial = 50 * 100_000_000u64;
        let division = self.height as u64 / 210_000u64;
        initial >> division
    }
}

impl Encodable for BlockExtra {
    fn consensus_encode<W: Write>(&self, mut writer: W) -> Result<usize, std::io::Error> {
        let mut written = 0;
        written += self.block.consensus_encode(&mut writer)?;
        written += self.block_hash.consensus_encode(&mut writer)?;
        written += self.size.consensus_encode(&mut writer)?;
        written += self.next.consensus_encode(&mut writer)?;
        written += self.height.consensus_encode(&mut writer)?;
        written += (self.outpoint_values.len() as u32).consensus_encode(&mut writer)?;
        for (out_point, tx_out) in self.outpoint_values.iter() {
            written += out_point.consensus_encode(&mut writer)?;
            written += tx_out.consensus_encode(&mut writer)?;
        }
        Ok(written)
    }
}

impl Decodable for BlockExtra {
    fn consensus_decode<D: Read>(mut d: D) -> Result<Self, Error> {
        Ok(BlockExtra {
            block: Decodable::consensus_decode(&mut d)?,
            block_hash: Decodable::consensus_decode(&mut d)?,
            size: Decodable::consensus_decode(&mut d)?,
            next: Decodable::consensus_decode(&mut d)?,
            height: Decodable::consensus_decode(&mut d)?,
            outpoint_values: {
                let len = u32::consensus_decode(&mut d)?;
                let mut m = HashMap::with_capacity(len as usize);
                for _ in 0..len {
                    m.insert(
                        Decodable::consensus_decode(&mut d)?,
                        Decodable::consensus_decode(&mut d)?,
                    );
                }
                m
            },
        })
    }
}

#[cfg(test)]
mod test {
    use crate::bitcoin::consensus::serialize;
    use crate::bitcoin::{Block, BlockHeader, OutPoint, TxOut};
    use crate::BlockExtra;
    use bitcoin::consensus::deserialize;
    use std::collections::HashMap;

    #[test]
    fn block_extra_round_trip() {
        let be = block_extra();
        let ser = serialize(&be);
        let deser = deserialize(&ser).unwrap();
        assert_eq!(be, deser);
    }

    fn block_extra() -> BlockExtra {
        BlockExtra {
            block: Block {
                header: BlockHeader {
                    version: 0,
                    prev_blockhash: Default::default(),
                    merkle_root: Default::default(),
                    time: 0,
                    bits: 0,
                    nonce: 0,
                },
                txdata: vec![],
            },
            block_hash: Default::default(),
            size: 0,
            next: vec![Default::default()],
            height: 0,
            outpoint_values: {
                let mut m = HashMap::new();
                m.insert(OutPoint::default(), TxOut::default());
                m
            },
        }
    }

    #[test]
    fn test_block_reward() {
        let mut be = block_extra();
        assert_eq!(be.base_reward(), 50 * 100_000_000);
        be.height = 209_999;
        assert_eq!(be.base_reward(), 50 * 100_000_000);
        be.height = 210_000;
        assert_eq!(be.base_reward(), 25 * 100_000_000);
        be.height = 420_000;
        assert_eq!(be.base_reward(), 1_250_000_000);
        be.height = 630_000;
        assert_eq!(be.base_reward(), 625_000_000);
    }
}
