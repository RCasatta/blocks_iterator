use bitcoin::{Address, Script, ScriptBuf};
use blocks_iterator::Config;
use env_logger::Env;
use fxhash::FxHasher64;
use log::info;
use std::collections::HashMap;
use std::error::Error;
use std::hash::Hasher;
use structopt::StructOpt;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("start");

    let mut config = Config::from_args();
    config.skip_prevout = true;
    let mut m: HashMap<u64, u64> = HashMap::new();
    let mut mapping_over_1000: HashMap<u64, ScriptBuf> = HashMap::new();
    let network = config.network;

    for b in blocks_iterator::iter(config) {
        for tx_out in b.iter_tx().into_iter().flat_map(|e| e.1.output.iter()) {
            let h = script_hash(&tx_out.script_pubkey);
            let val = m.entry(h).or_default();
            if *val > 1000 && !mapping_over_1000.contains_key(val) {
                mapping_over_1000.insert(h, tx_out.script_pubkey.clone());
            }

            *val += 1;
        }
    }

    let mut vec: Vec<_> = m.into_iter().collect();
    vec.sort_by(|a, b| a.1.cmp(&b.1));
    println!("mapping over 1000 len: {}", mapping_over_1000.len());

    for a in vec.iter().take(10) {
        if let Some(script) = mapping_over_1000.get(&a.0) {
            let address = Address::from_script(script, network)?;
            println!("{} {}", address, a.1);
        }
    }
    Ok(())
}

fn script_hash(script: &Script) -> u64 {
    let mut hasher = FxHasher64::default();
    hasher.write(script.as_bytes());
    hasher.finish()
}
