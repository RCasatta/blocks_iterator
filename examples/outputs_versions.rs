#![allow(non_snake_case)]

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

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let (send, recv) = sync_channel(100);
    let handle = blocks_iterator::iterate(config, send);
    let mut counters = [0usize; 17];
    while let Some(block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} {} counters:{:?}",
            block_extra.height,
            block_extra.block_hash,
            counters
        );

        if block_extra.height == 481824 {
            info!("segwit locked in");
        }
        if block_extra.height == 687456 {
            info!("taproot locked in");
        }

        for tx in block_extra.block.txdata {
            for (i, output) in tx.output.iter().enumerate() {
                if output.script_pubkey.is_witness_program() {
                    let mut version = output.script_pubkey.as_bytes()[0] as usize;
                    if version > 0x50 {
                        version -= 0x50;
                    }
                    counters[version] += 1;
                    if version >= 1 {
                        info!("tx:{} output:{} version:{}", tx.txid(), i, version);
                    }
                }
            }
        }
    }
    handle.join().expect("couldn't join");
    info!("counters: {:?}", counters);
    Ok(())
}
