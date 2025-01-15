use crate::bitcoin::consensus::{encode, Decodable, Encodable};
use crate::bitcoin::{Block, BlockHash, OutPoint, Transaction, TxOut};
use crate::FsBlock;
use bitcoin::Txid;
use log::debug;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::{BufReader, Seek, SeekFrom};
use std::ops::DerefMut;

/// The bitcoin block and additional metadata returned by the [crate::iter()] method
#[derive(Debug, Eq, PartialEq)]
pub struct BlockExtra {
    /// Serialization format version
    pub(crate) version: u8,
    /// The bitcoin block
    block: Block,
    /// The bitcoin block hash, same as `block.block_hash()` but result from hashing is cached
    pub(crate) block_hash: BlockHash,
    /// The byte size of the block, as returned by in `serialize(block).len()`
    pub(crate) size: u32,
    /// Hash of the blocks following this one, it's a vec because during reordering they may be more
    /// than one because of reorgs, as a result from the iteration, it's just one.
    pub(crate) next: Vec<BlockHash>,
    /// The height of the current block, number of blocks between this one and the genesis block
    pub(crate) height: u32,
    /// All the previous outputs of this block. Allowing to validate the script or computing the fee
    /// Note that when configuration `skip_script_pub(crate)key` is true, the script is empty,
    /// when `skip_prevout` is true, this map is empty.
    pub(crate) outpoint_values: HashMap<OutPoint, TxOut>,
    /// Total number of transaction inputs in this block
    pub(crate) block_total_inputs: u32,
    /// Total number of transaction outputs in this block
    pub(crate) block_total_outputs: u32,
    /// Precomputed transaction hashes such that `txids[i]=block.txdata[i].txid()`
    pub(crate) txids: Vec<Txid>,
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
            version: fs_block.serialization_version,
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
    pub fn version(&self) -> u8 {
        self.version
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    pub fn block_hash(&self) -> BlockHash {
        self.block_hash
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn next(&self) -> &Vec<BlockHash> {
        &self.next
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn outpoint_values(&self) -> &HashMap<OutPoint, TxOut> {
        &self.outpoint_values
    }

    pub fn block_total_inputs(&self) -> usize {
        self.block_total_inputs as usize
    }

    pub fn block_total_outputs(&self) -> usize {
        self.block_total_outputs as usize
    }

    pub fn txids(&self) -> &Vec<Txid> {
        &self.txids
    }

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
        let output_total: u64 = tx.output.iter().map(|el| el.value.to_sat()).sum();
        let mut input_total = 0u64;
        for input in tx.input.iter() {
            input_total += self
                .outpoint_values
                .get(&input.previous_output)?
                .value
                .to_sat();
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
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut written = 0;
        written += self.version.consensus_encode(writer)?;
        if self.version == 1 {
            written += self.size.consensus_encode(writer)?;
        }
        written += self.block.consensus_encode(writer)?;
        written += self.block_hash.consensus_encode(writer)?;
        if self.version == 0 {
            written += self.size.consensus_encode(writer)?;
        }
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
    fn consensus_decode<D: bitcoin::io::Read + ?Sized>(d: &mut D) -> Result<Self, encode::Error> {
        let version = Decodable::consensus_decode(d)?;
        let (size, block, block_hash) = match version {
            0 => {
                let block = Decodable::consensus_decode(d)?;
                let block_hash = Decodable::consensus_decode(d)?;
                let size = Decodable::consensus_decode(d)?;
                (size, block, block_hash)
            }
            1 => {
                let size = Decodable::consensus_decode(d)?;
                let block = Decodable::consensus_decode(d)?;
                let block_hash = Decodable::consensus_decode(d)?;
                (size, block, block_hash)
            }
            _ => {
                return Err(encode::Error::ParseFailed(
                    "Only version 0 and 1 are supported",
                ));
            }
        };
        Ok(BlockExtra {
            version,
            block,
            block_hash,
            size,
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
pub mod test {
    use crate::bitcoin::consensus::serialize;
    use crate::bitcoin::{Block, OutPoint, TxOut};
    use crate::BlockExtra;
    use bitcoin::block::{Header, Version};
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::consensus::{deserialize, Decodable};
    use bitcoin::hash_types::TxMerkleNode;
    use bitcoin::hashes::Hash;
    use bitcoin::{BlockHash, CompactTarget};
    use std::collections::HashMap;

    #[test]
    fn block_extra_round_trip() {
        let be = block_extra();
        let ser = serialize(&be);
        let deser = deserialize(&ser).unwrap();
        assert_eq!(be, deser);

        let mut be1 = be;
        be1.version = 1;
        let ser = serialize(&be1);
        let deser = deserialize(&ser).unwrap();
        assert_eq!(be1, deser);
    }

    pub fn block_extra() -> BlockExtra {
        let block = Block {
            header: Header {
                version: Version::from_consensus(0),
                prev_blockhash: BlockHash::all_zeros(),
                merkle_root: TxMerkleNode::all_zeros(),
                time: 0,
                bits: CompactTarget::from_consensus(0),
                nonce: 0,
            },
            txdata: vec![],
        };
        let size = serialize(&block).len() as u32;
        BlockExtra {
            version: 0,
            block,
            block_hash: BlockHash::all_zeros(),
            size,
            next: vec![BlockHash::all_zeros()],
            height: 0,
            outpoint_values: {
                let mut m = HashMap::new();
                m.insert(OutPoint::default(), TxOut::NULL);
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
        assert_eq!(hex, "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005100000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");

        let mut be1 = be;
        be1.version = 1;
        let hex1 = serialize_hex(&be1);
        assert_eq!(hex1, "0151000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");
        assert_ne!(hex, hex1);
    }

    #[test]
    fn block_extra_unsupported_version() {
        assert_eq!(
            "parse failed: Only version 0 and 1 are supported",
            BlockExtra::consensus_decode(&mut &[2u8][..])
                .unwrap_err()
                .to_string()
        );
    }
}
