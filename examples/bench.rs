#![allow(non_snake_case)]

use blocks_iterator::Config;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::sync::mpsc::sync_channel;
use std::time::Duration;
use std::time::Instant;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);

    let start = Instant::now();
    let mut last = Instant::now();
    let mut blocks = 0;
    let mut txs = 0;
    let mut blocks_total = 0;
    let mut txs_total = 0;
    while let Some(item) = recv.recv()? {
        blocks += 1;
        blocks_total += 1;
        txs += item.block.txdata.len() as u64;
        txs_total += item.block.txdata.len() as u64;

        let now = Instant::now();
        let period = Duration::from_secs(1);
        let current_duration = now.duration_since(last);

        if current_duration >= period {
            let total_duration = now.duration_since(start);
            eprintln!(
                "Current: {:>5} blk/s; {:>6} txs/s; Total: {:>5} blk/s; {:>6} tx/s;",
                blocks / current_duration.as_secs(),
                txs / current_duration.as_secs(),
                blocks_total / total_duration.as_secs(),
                txs_total / total_duration.as_secs(),
            );

            last = now;
            txs = 0;
            blocks = 0;
        }
    }

    handle.join().expect("couldn't join");
    Ok(())
}
