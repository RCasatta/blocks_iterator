use blocks_iterator::bitcoin::consensus::serialize;
use blocks_iterator::{periodic_log_level, Config};
use env_logger::Env;
use log::{debug, info, log};
use std::error::Error;
use std::io::{Cursor, Read};
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;
use xz2::bufread::XzEncoder;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let (send, recv) = sync_channel(0);
    let handle = blocks_iterator::iterate(config, send);
    let mut total_bytes = 0;
    let mut total_snap_bytes = 0;
    let mut total_min_snap_bytes = 0;
    let mut total_xz2_bytes = 0;
    let mut total_min_xz2_bytes = 0;
    while let Some(block_extra) = recv.recv()? {
        log!(
            periodic_log_level(block_extra.height),
            "# {:7} total:{} snap:{} snap_min:{} xz:{} xz_min:{}",
            block_extra.height,
            total_bytes / (1024 * 1024),
            total_snap_bytes / (1024 * 1024),
            total_min_snap_bytes / (1024 * 1024),
            total_xz2_bytes / (1024 * 1024),
            total_min_xz2_bytes / (1024 * 1024)
        );
        let data = serialize(&block_extra.block);
        let data_len = data.len();
        let data_copy = data.clone();
        let mut data_cursor = Cursor::new(data);

        let mut buffer = vec![];
        let mut wtr = snap::write::FrameEncoder::new(&mut buffer);
        std::io::copy(&mut data_cursor, &mut wtr).unwrap();
        drop(wtr);
        let snap_bytes = buffer.len();

        buffer.clear();
        let mut compressor = XzEncoder::new(&data_copy[..], 6);
        compressor.read_to_end(&mut buffer).unwrap();
        let xz_bytes = buffer.len();

        debug!(
            "{} read:{} snap:{} xz:{}",
            block_extra.block_hash, data_len, snap_bytes, xz_bytes
        );
        total_bytes += data_len;
        total_snap_bytes += snap_bytes;
        total_min_snap_bytes += snap_bytes.min(data_len);
        total_xz2_bytes += xz_bytes;
        total_min_xz2_bytes += xz_bytes.min(data_len);
    }
    handle.join().expect("couldn't join");
    info!("end");
    Ok(())
}
