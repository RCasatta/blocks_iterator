#![allow(non_snake_case)]

use blocks_iterator::periodic_log_level;
use blocks_iterator::Config;
use env_logger::Env;
use log::{info, log, error};
use std::error::Error;
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;
use std::collections::HashSet;
use blocks_iterator::bitcoin::hashes::Hash;
use std::convert::TryInto;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);
    let mut set = HashSet::new();
    while let Some(block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height, 10_000),
            "# {:7} {}",
            block_extra.height,
            block_extra.block_hash,
        );
        for tx in block_extra.block.txdata {
            let d = u64::from_ne_bytes(tx.txid().into_inner()[..8].try_into().unwrap() );
            if !set.insert(d) {
                error!("Collision on {}", tx.txid());
            }
        }

    }
    handle.join().expect("couldn't join");
    Ok(())
}
