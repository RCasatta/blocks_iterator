use bitcoin::consensus::Decodable;
use bitcoin::hashes::hex::ToHex;
use bitcoin::BlockHash;
use bitcoin::OutPoint;
use blocks_iterator::read_true_var_int;
use env_logger::Env;
use std::io;
use std::{
    fs::File,
    io::{Cursor, Read},
};

#[derive(Debug)]
pub struct Coin {
    pub code: u64,
    pub compressed_amount: u64,
    pub compressed_script: CompressedScript,
    pub kind: u64,
}

pub enum CompressedScript {
    Comp20(u8, [u8; 20]),
    Comp32(u8, [u8; 32]),
    Other(Vec<u8>),
}

impl std::fmt::Debug for CompressedScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Comp20(arg0, arg1) => f
                .debug_tuple("Comp20")
                .field(arg0)
                .field(&arg1.to_hex())
                .finish(),
            Self::Comp32(arg0, arg1) => f
                .debug_tuple("Comp32")
                .field(arg0)
                .field(&arg1.to_hex())
                .finish(),
            Self::Other(arg0) => f.debug_tuple("Other").field(&arg0.to_hex()).finish(),
        }
    }
}

impl Decodable for Coin {
    fn consensus_decode<R: io::Read + ?Sized>(
        mut reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let code = read_true_var_int(&mut reader)?;
        let compressed_amount = read_true_var_int(&mut reader)?;
        let (kind, compressed_script) = read_compressed_script(&mut reader)?;

        Ok(Coin {
            code,
            compressed_amount,
            compressed_script,
            kind,
        })
    }
}

fn read_compressed_script(mut reader: impl Read) -> Result<(u64, CompressedScript), io::Error> {
    let kind = read_true_var_int(&mut reader)?;
    let compressed_script = match kind {
        0 | 1 => {
            let mut buf = [0u8; 20];
            reader.read_exact(&mut buf[..])?;
            CompressedScript::Comp20(kind as u8, buf)
        }
        2 | 3 | 4 | 5 => {
            let mut buf = [0u8; 32];
            reader.read_exact(&mut buf[..])?;
            CompressedScript::Comp32(kind as u8, buf)
        }
        kind if kind > 10_000 => {
            let mut buf = vec![0u8; 1];
            reader.read_exact(&mut buf[..])?;
            CompressedScript::Other(buf)
        }
        _ => {
            let to_read = kind as usize - 6usize;
            let mut buf = vec![0u8; to_read];
            reader.read_exact(&mut buf[..])?;
            CompressedScript::Other(buf)
        }
    };

    Ok((kind, compressed_script))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut file = File::open("/tmp/utxo.dat").unwrap();
    let mut buffer = vec![];
    file.read_to_end(&mut buffer).unwrap();
    log::info!("size:{}", buffer.len());
    let mut cursor = Cursor::new(buffer);

    let block_hash = BlockHash::consensus_decode(&mut cursor).unwrap();
    log::info!("block_hash {:?}", block_hash);
    let _coins_count = u32::consensus_decode(&mut cursor).unwrap();
    let _ = u32::consensus_decode(&mut cursor).unwrap();

    let mut total = 0;
    loop {
        let out_point = match OutPoint::consensus_decode(&mut cursor) {
            Ok(v) => v,
            Err(_) => break,
        };

        let coin = match Coin::consensus_decode(&mut cursor) {
            Ok(coin) => coin,
            Err(e) => {
                log::warn!("{:?} position:{}", e, cursor.position());
                break;
            }
        };
        if out_point.vout > 100_000 {
            log::warn!("{}: {:?} {:?}", total, out_point, coin);
        }
        if total % 100_000 == 0 {
            log::info!("{}: {:?} {:?}", total, out_point, coin);
        }
        total += 1;
    }
    log::info!("total: {}", total);

    Ok(())
}
