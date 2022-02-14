#![allow(non_snake_case)]

use blocks_iterator::{Config, PeriodCounter};
use env_logger::Env;
use log::info;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc::sync_channel;
use std::time::Duration;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");
    let mut period = PeriodCounter::new(Duration::from_secs(10));

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let (send, recv) = sync_channel(config.channels_size.into());
    let handle = blocks_iterator::iterate(config, send);
    let mut counters = [0usize; 17];
    let mut output_file = File::create("outputs_versions.log").unwrap();
    while let Some(block_extra) = recv.recv()? {
        if period.period_elapsed().is_some() {
            info!(
                "# {:7} {} counters:{:?}",
                block_extra.height, block_extra.block_hash, counters
            );
        }

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
                        let log_line = format!(
                            "tx:{} output:{} version:{} height:{}",
                            tx.txid(),
                            i,
                            version,
                            block_extra.height
                        );
                        info!("{}", log_line);
                        output_file.write(log_line.as_bytes()).unwrap();
                        output_file.write(b"\n").unwrap();
                    }
                }
            }
        }
    }
    handle.join().expect("couldn't join");
    info!("counters: {:?}", counters);
    Ok(())
}
