use blocks_iterator::bitcoin::consensus::serialize;
use blocks_iterator::{periodic_log_level, Config};
use env_logger::Env;
use log::{debug, info, log};
use std::error::Error;
use std::io::Cursor;
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let (send, recv) = sync_channel(0);
    let handle = blocks_iterator::iterate(config, send);
    let mut total_bytes = 0;
    let mut total_compressed_bytes = 0;
    while let Some(block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} total:{} compressed:{}",
            block_extra.height,
            total_bytes,
            total_compressed_bytes
        );
        let data = serialize(&block_extra.block);
        let data_len = data.len();
        let mut data_cursor = Cursor::new(data);
        let mut buffer = vec![];
        let mut wtr = snap::write::FrameEncoder::new(&mut buffer);
        std::io::copy(&mut data_cursor, &mut wtr).unwrap();
        drop(wtr);

        debug!(
            "{} read:{} write:{}",
            block_extra.block_hash,
            data_len,
            buffer.len()
        );
        total_bytes += data_len;
        total_compressed_bytes += buffer.len();
    }
    handle.join().expect("couldn't join");
    info!("end");
    Ok(())
}
