use bitcoin::consensus::encode::MAX_VEC_SIZE;
use bitcoin::consensus::Encodable;
use blocks_iterator::{Config, PeriodCounter};
use env_logger::Env;
use log::info;
use std::error::Error;
use std::io;
use std::io::Write;
use std::sync::mpsc::sync_channel;
use std::time::Duration;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let config = Config::from_args();
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);
    let mut bench = PeriodCounter::new(Duration::from_secs(10));
    let mut buffer = [0u8; MAX_VEC_SIZE];
    while let Some(block_extra) = recv.recv()? {
        bench.count_block(&block_extra.block);
        if let Some(stats) = bench.period_elapsed() {
            info!(
                "# {:7} {} {:?}",
                block_extra.height,
                block_extra.block_hash,
                block_extra.fee()
            );
            info!("{}", stats);
        }
        let size = block_extra.consensus_encode(&mut buffer[..]).unwrap();
        io::stdout().write_all(&buffer[..size])?;
    }
    handle.join().expect("couldn't join");
    info!("end");
    Ok(())
}
