#![allow(non_snake_case)]

use blocks_iterator::Config;
use blocks_iterator::PeriodCounter;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::sync::mpsc::sync_channel;
use std::time::Duration;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);
    let mut bench = PeriodCounter::new(Duration::from_secs(1));

    while let Some(item) = recv.recv()? {
        bench.count_block(&item.block);

        if let Some(stats) = bench.period_elapsed() {
            info!("{}", stats);
        }
    }

    handle.join().expect("couldn't join");
    Ok(())
}
