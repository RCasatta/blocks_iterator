#![feature(test)]

extern crate test;

use bitcoin::hashes::sha256::Midstate;
use bitcoin::hashes::{sha256, Hash, HashEngine};
use bitcoin::OutPoint;
use rocksdb::{Options, WriteBatch, DB};
use sha2::{Digest, Sha256};
use test::{black_box, Bencher};

#[bench]
fn bench_blake3(b: &mut Bencher) {
    let outpoint = OutPoint::default();
    let salt = [0u8; 12];

    b.iter(|| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&salt[..]);
        hasher.update(outpoint.txid.as_ref());
        hasher.update(&outpoint.vout.to_ne_bytes());
        let hash = hasher.finalize();
        let mut result = [0u8; 12];
        result.copy_from_slice(&hash.as_bytes()[..12]);
        result
    });
}

#[bench]
fn bench_bitcoin_hashes_sha(b: &mut Bencher) {
    let outpoint = OutPoint::default();
    let salt = [0u8; 12];

    b.iter(|| {
        let mut engine = sha256::Hash::engine();
        engine.input(&salt);
        engine.input(&outpoint.txid.as_ref());
        engine.input(&outpoint.vout.to_ne_bytes()[..]);
        let hash = sha256::Hash::from_engine(engine);
        let mut result = [0u8; 12];
        result.copy_from_slice(&hash.as_byte_array()[..12]);
        black_box(result);
    });
}

#[bench]
fn bench_bitcoin_hashes_sha_midstate(b: &mut Bencher) {
    let outpoint = OutPoint::default();
    let salt = [0u8; 32];
    let midstate = Midstate(salt);
    let midstate_engine = sha256::HashEngine::from_midstate(midstate, 64);
    b.iter(|| {
        let mut engine = midstate_engine.clone();
        engine.input(&outpoint.txid.as_ref());
        engine.input(&outpoint.vout.to_ne_bytes()[..]);
        let hash = sha256::Hash::from_engine(engine);
        let mut result = [0u8; 12];
        result.copy_from_slice(&hash.as_byte_array()[..12]);
        black_box(result);
    });
}

#[bench]
fn bench_sha2_crate(b: &mut Bencher) {
    let outpoint = OutPoint::default();
    let salt = [0u8; 12];

    b.iter(|| {
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(&outpoint.txid.as_byte_array());
        hasher.update(&outpoint.vout.to_ne_bytes()[..]);
        let hash = hasher.finalize();
        let mut result = [0u8; 12];
        result.copy_from_slice(&hash[..12]);
        black_box(result);
    });
}

#[bench]
fn bench_bitcoin_hashes_sha_long(b: &mut Bencher) {
    let a: Vec<_> = (0u8..255).cycle().take(1000).collect();
    b.iter(|| {
        let mut engine = sha256::Hash::engine();
        engine.input(&a);
        let hash = sha256::Hash::from_engine(engine);
        black_box(hash);
    });
}

#[bench]
fn bench_sha2_crate_long(b: &mut Bencher) {
    let a: Vec<_> = (0u8..255).cycle().take(1000).collect();
    b.iter(|| {
        let mut hasher = Sha256::new();
        hasher.update(&a);
        let hash = hasher.finalize();
        black_box(hash);
    });
}

#[bench]
fn bench_fxhash(b: &mut Bencher) {
    let outpoint = OutPoint::default();
    let salt = [0u8; 12];

    b.iter(|| {
        let a = fxhash::hash32(&(&outpoint, &salt));
        let b = fxhash::hash64(&(&outpoint, &salt));
        let mut result = [0u8; 12];

        result[..4].copy_from_slice(&a.to_ne_bytes()[..]);
        result[4..].copy_from_slice(&b.to_ne_bytes()[..]);
        black_box(result);
    });
}

#[bench]
fn bench_db_batch(b: &mut Bencher) {
    let tempdir = tempfile::TempDir::new().unwrap();
    let mut options = Options::default();
    options.create_if_missing(true);
    let db = DB::open(&options, &tempdir).unwrap();

    b.iter(|| {
        let mut key = [0u8; 32];
        let value = [0u8; 32];
        let mut batch = WriteBatch::default();
        for i in 0..200 {
            key[i as usize % 32] = i;
            batch.put(key, value);
        }
        db.write(batch).unwrap();
        db.flush().unwrap();
    });
}

#[bench]
fn bench_db_no_batch(b: &mut Bencher) {
    let tempdir = tempfile::TempDir::new().unwrap();
    let mut options = Options::default();
    options.create_if_missing(true);
    let db = DB::open(&options, &tempdir).unwrap();
    b.iter(|| {
        let mut key = [0u8; 32];
        let value = [0u8; 32];
        for i in 0..200 {
            key[i as usize % 32] = i;
            db.put(key, value).unwrap();
        }
        db.flush().unwrap();
    });
}
