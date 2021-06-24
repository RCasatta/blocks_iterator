#![allow(non_snake_case)]

use bitcoin::consensus::{deserialize, encode, Decodable};
use bitcoin::SigHashType;
use blocks_iterator::periodic_log_level;
use blocks_iterator::Config;
use env_logger::Env;
use log::{info, log};
use std::error::Error;
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(100);
    let handle = blocks_iterator::iterate(config, send);
    let mut signatures_in_witness = 0;
    let mut block_with_witness;
    let mut blocks_with_witness = 0;
    while let Some(block_extra) = recv.recv()? {
        block_with_witness = false;
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} {} {:?} sig_wit:{} blk_wit:{}",
            block_extra.height,
            block_extra.block_hash,
            block_extra.fee(),
            signatures_in_witness,
            blocks_with_witness
        );

        for tx in block_extra.block.txdata {
            for input in tx.input {
                for witness in input.witness {
                    if let Ok(_sig) = deserialize::<ParsedSignature>(&witness) {
                        signatures_in_witness += 1;
                        block_with_witness = true;
                    }
                }
            }
        }
        if block_with_witness {
            blocks_with_witness += 1;
        }
    }
    handle.join().expect("couldn't join");
    info!(
        "signatures_in_witness: {} blocks_with_witness: {}",
        signatures_in_witness, blocks_with_witness
    );
    Ok(())
}

struct ParsedSignature {
    pub sighash: SigHashType,
    pub R: Vec<u8>,
    pub s: Vec<u8>,
}

impl Decodable for ParsedSignature {
    fn consensus_decode<D: std::io::Read>(mut d: D) -> Result<Self, encode::Error> {
        let first = u8::consensus_decode(&mut d)?;
        if first != 0x30 {
            return Err(encode::Error::ParseFailed("Signature must start with 0x30"));
        }
        let _ = u8::consensus_decode(&mut d)?;
        let integer_header = u8::consensus_decode(&mut d)?;
        if integer_header != 0x02 {
            return Err(encode::Error::ParseFailed("No integer header"));
        }

        let R = <Vec<u8>>::consensus_decode(&mut d)?;
        let integer_header = u8::consensus_decode(&mut d)?;
        if integer_header != 0x02 {
            return Err(encode::Error::ParseFailed("No integer header"));
        }
        let s = <Vec<u8>>::consensus_decode(&mut d)?;
        let sighash_u8 = u8::consensus_decode(&mut d)?;
        let sighash = SigHashType::from_u32_consensus(sighash_u8 as u32);

        Ok(ParsedSignature { sighash, R, s })
    }
}
