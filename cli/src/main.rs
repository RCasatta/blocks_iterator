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
    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));
    if std::env::var("LOG_AVOID_TIMESTAMP").is_ok() {
        builder.format(|buf, record| {
            writeln!(
                buf,
                "{:5} {} {}",
                record.level(),
                record.target(),
                record.args()
            )
        });
    }
    builder.init();
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
