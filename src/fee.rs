use crate::truncmap::TruncMap;
use crate::BlockExtra;
use bitcoin::consensus::{encode, Decodable, Encodable};
use bitcoin::{OutPoint, Script, Transaction, TxOut, Txid, VarInt};
use core::mem;
use log::{debug, info, trace};
use std::io;
use std::io::Error;
use std::ops::{Index, IndexMut};
use std::slice::Iter;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

pub struct Fee {
    skip_prevout: bool,
    skip_script_pubkey: bool,
    receiver: Receiver<Option<BlockExtra>>,
    sender: SyncSender<Option<BlockExtra>>,
    utxo: Utxo,
}

struct Utxo(TruncMap);

impl Utxo {
    pub fn new() -> Self {
        Utxo(TruncMap::default())
    }

    pub fn add(&mut self, tx: &Transaction) -> Txid {
        let txid = tx.txid();
        let vec: Vec<Status> = tx
            .output
            .iter()
            .map(|o| Status::Unspent(o.script_pubkey.clone(), o.value))
            .collect();
        self.insert(txid, VecStatus(vec))
    }

    pub fn insert(&mut self, txid: Txid, vec_status: VecStatus) -> Txid {
        self.0.insert(txid, vec_status);
        txid
    }

    pub fn remove(&mut self, txid: &Txid) -> VecStatus {
        self.0.remove(txid).unwrap()
    }
}

impl Fee {
    pub fn new(
        skip_prevout: bool,
        skip_script_pubkey: bool,
        receiver: Receiver<Option<BlockExtra>>,
        sender: SyncSender<Option<BlockExtra>>,
    ) -> Fee {
        Fee {
            skip_prevout,
            skip_script_pubkey,
            sender,
            receiver,
            utxo: Utxo::new(),
        }
    }

    pub fn start(&mut self) {
        info!("starting fee processer");
        let mut busy_time = 0u128;
        let mut total_txs = 0u64;
        loop {
            let received = self.receiver.recv().unwrap();
            let now = Instant::now();
            match received {
                Some(mut block_extra) => {
                    trace!("fee received: {}", block_extra.block_hash);
                    total_txs += block_extra.block.txdata.len() as u64;
                    if !self.skip_prevout {
                        if block_extra.height % 10_000 == 0 {
                            info!("tx in utxo: {:?}", self.utxo.0.len())
                        }
                        for tx in block_extra.block.txdata.iter() {
                            let txid = self.utxo.add(tx);
                            block_extra.tx_hashes.insert(txid);
                        }

                        for tx in block_extra.block.txdata.iter().skip(1) {
                            for input in tx.input.iter() {
                                let mut outputs = self.utxo.remove(&input.previous_output.txid);

                                match outputs[input.previous_output.vout as usize].take() {
                                    Some(txout) => {
                                        block_extra
                                            .outpoint_values
                                            .insert(input.previous_output, txout);
                                    }
                                    None => panic!("found spent"),
                                }
                                if outputs.iter().any(|e| e.is_unspent()) {
                                    self.utxo.insert(input.previous_output.txid, outputs);
                                }
                            }
                        }
                        let coin_base_output_value = block_extra.block.txdata[0]
                            .output
                            .iter()
                            .map(|el| el.value)
                            .sum();
                        block_extra.outpoint_values.insert(
                            OutPoint::default(),
                            TxOut {
                                script_pubkey: Script::new(),
                                value: coin_base_output_value,
                            },
                        );

                        debug!(
                            "#{:>6} {} size:{:>7} txs:{:>4} total_txs:{:>9} fee:{:>9}",
                            block_extra.height,
                            block_extra.block_hash,
                            block_extra.size,
                            block_extra.block.txdata.len(),
                            total_txs,
                            block_extra.fee(),
                        );
                    }
                    busy_time += now.elapsed().as_nanos();
                    self.sender.send(Some(block_extra)).unwrap();
                }
                None => break,
            }
        }

        self.sender.send(None).expect("fee: cannot send none");

        info!(
            "ending fee processer total tx {}, busy time: {}s",
            total_txs,
            busy_time / 1_000_000_000
        );
    }
}
#[derive(Eq, PartialEq, Debug)]
pub enum Status {
    Unspent(Script, u64),
    Spent,
}

impl Status {
    pub fn take(&mut self) -> Option<TxOut> {
        let val = mem::replace(self, Status::Spent);
        match val {
            Status::Spent => None,
            Status::Unspent(script_pubkey, value) => Some(TxOut {
                script_pubkey,
                value,
            }),
        }
    }
    pub fn is_unspent(&self) -> bool {
        match self {
            Status::Spent => false,
            Status::Unspent(_, _) => true,
        }
    }
}

impl Encodable for Status {
    fn consensus_encode<W: io::Write>(&self, mut writer: W) -> Result<usize, Error> {
        match self {
            Status::Unspent(script, value) => {
                0u8.consensus_encode(&mut writer)?;
                let mut len = 1;
                len += script.consensus_encode(&mut writer)?;
                len += VarInt(*value).consensus_encode(writer)?;
                Ok(len)
            }
            Status::Spent => 1u8.consensus_encode(writer),
        }
    }
}

impl Decodable for Status {
    fn consensus_decode<D: io::Read>(mut d: D) -> Result<Self, encode::Error> {
        let a = u8::consensus_decode(&mut d)?;
        Ok(if a == 0 {
            let script = Script::consensus_decode(&mut d)?;
            let value = VarInt::consensus_decode(&mut d)?.0;
            Status::Unspent(script, value)
        } else {
            Status::Spent
        })
    }
}

const MAX_OBJECT_SIZE: usize = 4_000_000;

#[derive(Debug, Eq, PartialEq)]
pub struct VecStatus(Vec<Status>);

impl VecStatus {
    pub fn iter(&self) -> Iter<'_, Status> {
        self.0.iter()
    }
}

impl Index<usize> for VecStatus {
    type Output = Status;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for VecStatus {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Encodable for VecStatus {
    #[inline]
    fn consensus_encode<S: io::Write>(&self, mut s: S) -> Result<usize, io::Error> {
        let mut len = 0;
        len += VarInt(self.0.len() as u64).consensus_encode(&mut s)?;
        for c in self.0.iter() {
            len += c.consensus_encode(&mut s)?;
        }
        Ok(len)
    }
}
impl Decodable for VecStatus {
    #[inline]
    fn consensus_decode<D: io::Read>(mut d: D) -> Result<Self, encode::Error> {
        let len = VarInt::consensus_decode(&mut d)?.0;
        let byte_size = (len as usize)
            .checked_mul(mem::size_of::<Status>())
            .unwrap();
        if byte_size > MAX_OBJECT_SIZE {
            panic!();
        }
        let mut ret = Vec::with_capacity(len as usize);
        let mut d = d.take(MAX_OBJECT_SIZE as u64);
        for _ in 0..len {
            ret.push(Decodable::consensus_decode(&mut d)?);
        }
        Ok(VecStatus(ret))
    }
}

#[cfg(test)]
mod test {
    use crate::fee::{Status, VecStatus};
    use bitcoin::blockdata::constants::genesis_block;
    use bitcoin::consensus::{deserialize, serialize};
    use bitcoin::{Network, Script, TxOut};

    #[test]
    fn test_size() {
        assert_eq!(
            std::mem::size_of::<Option<TxOut>>(),
            std::mem::size_of::<TxOut>()
        );
    }

    #[test]
    fn test_status() {
        let s = Status::Unspent(Script::default(), 0);
        let v = serialize(&s);
        let back = deserialize(&v).unwrap();
        assert_eq!(s, back);

        let s1 = Status::Spent;
        let v1 = serialize(&s1);
        let back1 = deserialize(&v1).unwrap();
        assert_eq!(s1, back1);

        let s2 = VecStatus(vec![s, s1]);
        let v2 = serialize(&s2);
        let back2 = deserialize(&v2).unwrap();
        assert_eq!(s2, back2);
        assert_eq!(5, v2.len());
    }
}
