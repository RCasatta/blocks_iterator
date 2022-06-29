use crate::bitcoin::consensus::{encode, Decodable, Encodable};
use crate::bitcoin::{Block, BlockHash, OutPoint, Transaction, TxOut};
use crate::FsBlock;
use bitcoin::Txid;
use log::debug;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::ops::DerefMut;

/// The bitcoin block and additional metadata returned by the [crate::iter()] method
#[derive(Debug, Eq, PartialEq)]
pub struct BlockExtra {
    /// Serialization format version
    pub version: u8,
    /// The bitcoin block
    pub block: Block,
    /// The bitcoin block hash, same as `block.block_hash()` but result from hashing is cached
    pub block_hash: BlockHash,
    /// The byte size of the block, as returned by in `serialize(block).len()`
    pub size: u32,
    /// Hash of the blocks following this one, it's a vec because during reordering they may be more
    /// than one because of reorgs, as a result from the iteration, it's just one.
    pub next: Vec<BlockHash>,
    /// The height of the current block, number of blocks between this one and the genesis block
    pub height: u32,
    /// All the previous outputs of this block. Allowing to validate the script or computing the fee
    /// Note that when configuration `skip_script_pubkey` is true, the script is empty,
    /// when `skip_prevout` is true, this map is empty.
    pub outpoint_values: HashMap<OutPoint, TxOut>,
    /// Total number of transaction inputs in this block
    pub block_total_inputs: u32,
    /// Total number of transaction outputs in this block
    pub block_total_outputs: u32,
    /// Precomputed transaction hashes such that `txids[i]=block.txdata[i].txid()`
    pub txids: Vec<Txid>,
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
        let mut reader = BufReader::new(file);
        let block =
            Block::consensus_decode(&mut reader).map_err(|e| err(e.to_string(), &fs_block))?;

        let txs = &block.txdata;
        let block_total_inputs = txs.iter().fold(0usize, |acc, tx| acc + tx.input.len());
        let block_total_outputs = txs.iter().fold(0usize, |acc, tx| acc + tx.output.len());

        Ok(BlockExtra {
            version: 0,
            block,
            block_hash: fs_block.hash,
            size: (fs_block.end - fs_block.start) as u32,
            next: fs_block.next,
            height: 0,
            outpoint_values: HashMap::with_capacity(block_total_inputs),
            block_total_inputs: block_total_inputs as u32,
            block_total_outputs: block_total_outputs as u32,
            txids: vec![],
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

    /// Return the base block reward in satoshi
    pub fn base_reward(&self) -> u64 {
        let initial = 50 * 100_000_000u64;
        let division = self.height as u64 / 210_000u64;
        initial >> division
    }

    /// Iterate transactions of blocks together with their txids
    pub fn iter_tx(&self) -> impl Iterator<Item = (&Txid, &Transaction)> {
        self.txids.iter().zip(self.block.txdata.iter())
    }
}

impl Encodable for BlockExtra {
    fn consensus_encode<W: Write + ?Sized>(&self, writer: &mut W) -> Result<usize, std::io::Error> {
        let mut written = 0;
        written += self.version.consensus_encode(writer)?;
        written += self.block.consensus_encode(writer)?;
        written += self.block_hash.consensus_encode(writer)?;
        written += self.size.consensus_encode(writer)?;
        written += self.next.consensus_encode(writer)?;
        written += self.height.consensus_encode(writer)?;
        written += (self.outpoint_values.len() as u32).consensus_encode(writer)?;
        for (out_point, tx_out) in self.outpoint_values.iter() {
            written += out_point.consensus_encode(writer)?;
            written += tx_out.consensus_encode(writer)?;
        }
        written += self.block_total_inputs.consensus_encode(writer)?;
        written += self.block_total_outputs.consensus_encode(writer)?;
        written += (self.txids.len() as u32).consensus_encode(writer)?;
        for txid in self.txids.iter() {
            written += txid.consensus_encode(writer)?;
        }
        Ok(written)
    }
}

impl Decodable for BlockExtra {
    fn consensus_decode<D: Read + ?Sized>(d: &mut D) -> Result<Self, encode::Error> {
        Ok(BlockExtra {
            version: Decodable::consensus_decode(d)?,
            block: Decodable::consensus_decode(d)?,
            block_hash: Decodable::consensus_decode(d)?,
            size: Decodable::consensus_decode(d)?,
            next: Decodable::consensus_decode(d)?,
            height: Decodable::consensus_decode(d)?,
            outpoint_values: {
                let len = u32::consensus_decode(d)?;
                let mut m = HashMap::with_capacity(len as usize);
                for _ in 0..len {
                    m.insert(
                        Decodable::consensus_decode(d)?,
                        Decodable::consensus_decode(d)?,
                    );
                }
                m
            },
            block_total_inputs: Decodable::consensus_decode(d)?,
            block_total_outputs: Decodable::consensus_decode(d)?,
            txids: {
                let len = u32::consensus_decode(d)?;
                let mut v = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    v.push(Decodable::consensus_decode(d)?);
                }
                v
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
    use bitcoin::consensus::encode::serialize_hex;
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
            version: 0,
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
            block_total_inputs: 0,
            block_total_outputs: 0,
            txids: vec![],
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

    #[test]
    fn test_hex() {
        let be = block_extra();
        let hex = serialize_hex(&be);
        assert_eq!(hex, "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");
    }
}
