use bitcoin::ScriptBuf;
use blocks_iterator::Config;
use env_logger::Env;
use log::info;
use std::collections::HashMap;
use std::error::Error;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let mut m: HashMap<ScriptBuf, u64> = HashMap::new();

    for b in blocks_iterator::iter(config) {
        for h in b
            .iter_tx()
            .into_iter()
            .flat_map(|e| e.1.output.iter())
            .map(|e| e.script_pubkey.clone())
        {
            *m.entry(h).or_default() += 1;
        }
    }

    Ok(())
}
