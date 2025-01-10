use bitcoin::consensus::encode::MAX_VEC_SIZE;
use bitcoin::consensus::Encodable;
use blocks_iterator::Config;
use clap::Parser;
use env_logger::Env;
use log::info;
use std::error::Error;
use std::io;
use std::io::Write;

fn main() -> Result<(), Box<dyn Error>> {
    init_logging();
    info!("start");

    let config = Config::parse();

    let blocks_iter = blocks_iterator::iter(config);
    let mut buffer = [0u8; MAX_VEC_SIZE];
    for block_extra in blocks_iter {
        let size = block_extra.consensus_encode(&mut &mut buffer[..]).unwrap();
        io::stdout().write_all(&buffer[..size])?;
    }
    info!("end");
    Ok(())
}

fn init_logging() {
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    if let Ok(s) = std::env::var("RUST_LOG_STYLE") {
        if s == "SYSTEMD" {
            builder.format(|buf, record| {
                let level = match record.level() {
                    log::Level::Error => 3,
                    log::Level::Warn => 4,
                    log::Level::Info => 6,
                    log::Level::Debug => 7,
                    log::Level::Trace => 7,
                };
                writeln!(buf, "<{}>{}: {}", level, record.target(), record.args())
            });
        }
    }

    builder.init();
}
