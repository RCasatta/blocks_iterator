use blocks_iterator::Config;
use env_logger::Env;
use log::{debug, info, log};
use std::sync::mpsc::sync_channel;
use structopt::StructOpt;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let config = Config::from_args();
    let (send, recv) = sync_channel(100);

    info!("start");
    let handle = blocks_iterator::iterate(config, send);
    loop {
        let received = recv.recv().expect("cannot receive blob");
        match received {
            Some(block_extra) => {
                log!(
                    periodic_log_level(block_extra.height),
                    "# {:7} {} {:10}",
                    block_extra.height,
                    block_extra.block_hash,
                    block_extra.fee()
                );
            }
            None => break,
        }
    }
    handle.join().unwrap();
    info!("end");
}
