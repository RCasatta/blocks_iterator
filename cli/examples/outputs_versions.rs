#![allow(non_snake_case)]

use blocks_iterator::{Config, PeriodCounter};
use clap::Parser;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");
    let mut period = PeriodCounter::new(Duration::from_secs(10));

    let mut config = Config::parse();
    config.skip_prevout = true;
    let iter = blocks_iterator::iter(config);
    let mut counters = [0usize; 17];
    let mut output_file = File::create("outputs_versions.log").unwrap();
    for block_extra in iter {
        if period.period_elapsed().is_some() {
            info!(
                "# {:7} {} counters:{:?}",
                block_extra.height(),
                block_extra.block_hash(),
                counters
            );
        }

        if block_extra.height() == 481824 {
            info!("segwit locked in");
        }
        if block_extra.height() == 687456 {
            info!("taproot locked in");
        }

        for (txid, tx) in block_extra.iter_tx() {
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
                            txid,
                            i,
                            version,
                            block_extra.height()
                        );
                        info!("{}", log_line);
                        output_file.write_all(log_line.as_bytes()).unwrap();
                        output_file.write_all(b"\n").unwrap();
                    }
                }
            }
        }
    }
    info!("counters: {:?}", counters);
    Ok(())
}
