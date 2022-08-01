use bitcoin::hashes::hex::FromHex;
use bitcoin::Txid;
use blocks_iterator::utxo::{decode_utxo_pair, DbUtxo};
use env_logger::Env;
use std::{collections::HashSet, fs::File, io::Write, path::PathBuf};
use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
struct Config {
    /// Specify the directory where ther is the UTXO database created by `blocks_iterator`
    #[structopt(short, long)]
    pub utxo_db: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let config = Config::from_args();
    let genesis_coinbase =
        Txid::from_hex("4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b").unwrap();
    log::info!("start");

    let db = DbUtxo::new(config.utxo_db).unwrap();
    log::info!("db loaded");

    let mut scripts_set = HashSet::new();
    let mut txid_set = HashSet::new();

    let mut total_amount = 0u64;
    let mut total_elements = 0u32;
    let mut total_ser_size = 0usize;

    let mut ser = File::create("/tmp/utxo.bin").unwrap();
    for utxo_pair in db.iter_utxo_bytes() {
        total_ser_size += utxo_pair.serialized_len();
        ser.write_all(utxo_pair.out_point_bytes()).unwrap();
        ser.write_all(utxo_pair.tx_out_bytes()).unwrap();

        let (out_point, tx_out) = decode_utxo_pair(&utxo_pair).unwrap();
        log::debug!("{:?} {:?}", out_point, tx_out);
        total_elements += 1;
        if out_point.txid == genesis_coinbase {
            log::info!("excluding genesis coinbase tx from total_amount");
        } else {
            total_amount += tx_out.value;
        }
        scripts_set.insert(tx_out.script_pubkey);
        txid_set.insert(out_point.txid);
    }

    let total_unique_script_size: usize = scripts_set.iter().map(|s| s.len()).sum();
    let total_unique_txid_size: usize = txid_set.iter().map(|s| s.len()).sum();

    log::info!(
        "total_amount:{} total_elements:{} stats:{} ser_size:{} total_unique_script_size:{} total_unique_txid_size:{}",
        total_amount,
        total_elements,
        db.stat(),
        total_ser_size,
        total_unique_script_size,
        total_unique_txid_size
    );
    Ok(())
}
