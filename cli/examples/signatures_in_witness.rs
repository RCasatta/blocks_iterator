#![allow(non_snake_case)]

use bitcoin::consensus::{deserialize, encode, Decodable};
use bitcoin::sighash::EcdsaSighashType;
use blocks_iterator::{Config, PeriodCounter};
use clap::Parser;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::parse();
    let iter = blocks_iterator::iter(config);
    let mut signatures_in_witness = 0;
    let mut block_with_witness;
    let mut blocks_with_witness = 0;
    let mut period = PeriodCounter::new(Duration::from_secs(10));

    for block_extra in iter {
        block_with_witness = false;
        if period.period_elapsed().is_some() {
            info!(
                "# {:7} {} {:?} sig_wit:{} blk_wit:{}",
                block_extra.height(),
                block_extra.block_hash(),
                block_extra.fee(),
                signatures_in_witness,
                blocks_with_witness
            );
        }

        for tx in &block_extra.block().txdata {
            for input in &tx.input {
                for witness in input.witness.iter() {
                    if let Ok(_sig) = deserialize::<ParsedSignature>(witness) {
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
    info!(
        "signatures_in_witness: {} blocks_with_witness: {}",
        signatures_in_witness, blocks_with_witness
    );
    Ok(())
}

struct ParsedSignature {
    pub _sighash: EcdsaSighashType,
    pub _R: Vec<u8>,
    pub _s: Vec<u8>,
}

impl Decodable for ParsedSignature {
    fn consensus_decode<D: bitcoin::io::Read + ?Sized>(d: &mut D) -> Result<Self, encode::Error> {
        //TODO fix for schnorr signatures!
        let first = u8::consensus_decode(d)?;
        if first != 0x30 {
            return Err(encode::Error::ParseFailed("Signature must start with 0x30"));
        }
        let _ = u8::consensus_decode(d)?;
        let integer_header = u8::consensus_decode(d)?;
        if integer_header != 0x02 {
            return Err(encode::Error::ParseFailed("No integer header"));
        }

        let R = <Vec<u8>>::consensus_decode(d)?;
        let integer_header = u8::consensus_decode(d)?;
        if integer_header != 0x02 {
            return Err(encode::Error::ParseFailed("No integer header"));
        }
        let s = <Vec<u8>>::consensus_decode(d)?;
        let sighash_u8 = u8::consensus_decode(d)?;
        let sighash = EcdsaSighashType::from_consensus(sighash_u8 as u32);

        Ok(ParsedSignature {
            _sighash: sighash,
            _R: R,
            _s: s,
        })
    }
}
