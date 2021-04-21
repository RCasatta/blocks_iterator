use bitcoin::Network;
use blocks_iterator::Config;
use env_logger::Env;
use log::{info, debug};
use std::sync::mpsc::sync_channel;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let config = Config {
        blocks_dir: "/Volumes/Transcend/bitcoin-testnet/testnet3/blocks".into(),
        network: Network::Testnet,
    };

    let (send, recv) = sync_channel(1);

    info!("start");
    let handle = blocks_iterator::iterate(config, send);
    loop {
        let received = recv.recv().expect("cannot receive blob");
        match received {
            Some(block_extra) => {
                debug!(
                    "# {:7} {} {:10}",
                    block_extra.height,
                    block_extra.block_hash,
                    block_extra.fee()
                );
                if block_extra.height % 100_000 == 0 {
                    info!("processed {}", block_extra.height);
                }
            }
            None => break,
        }
    }
    handle.join().unwrap();
    info!("end");
}
