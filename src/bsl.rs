use bitcoin_slices::{bsl, Parse, ParseResult};
use bitcoin_slices::{number::U32, number::U8, read_slice, SResult, Visit, Visitor};

struct BlockExtra<'a> {
    slice: &'a [u8],
}

impl<'a> AsRef<[u8]> for BlockExtra<'a> {
    fn as_ref(&self) -> &[u8] {
        self.slice
    }
}

impl<'a> Visit<'a> for BlockExtra<'a> {
    fn visit<'b, V: Visitor>(slice: &'a [u8], visit: &'b mut V) -> SResult<'a, Self> {
        let version = U8::parse(slice)?;
        let mut consumed = 1;

        let block = bsl::Block::visit(version.remaining(), visit)?;
        consumed += block.consumed();

        let block_hash = read_slice(block.remaining(), 32)?;
        consumed += 4;

        let block_size = U32::parse(block_hash.remaining())?;
        consumed += 32;

        let next_len = bsl::parse_len(block_size.remaining())?;
        consumed += next_len.consumed();

        let mut current = &block_size.remaining()[consumed..];
        for _ in 0..next_len.n() {
            let block_hash = read_slice(current, 32)?;
            current = block_hash.remaining();
            consumed += 32;
        }

        let height = U32::parse(current)?;
        consumed += 4;

        let map_len = U32::parse(height.remaining())?;
        consumed += 4;

        let mut current = map_len.remaining();
        for _ in 0u32..map_len.parsed().into() {
            // add visit extra call
            let outpoint = bsl::OutPoint::parse(current)?;
            consumed += outpoint.consumed();
            let txout = bsl::TxOut::parse(outpoint.remaining())?;
            consumed += txout.consumed();
            current = txout.remaining();
        }
        let block_total_inputs = U32::parse(current)?;
        let block_total_outputs = U32::parse(block_total_inputs.remaining())?;
        let txids_len = U32::parse(block_total_outputs.remaining())?;
        consumed += 12;

        let mut current = txids_len.remaining();
        for _ in 0u32..txids_len.parsed().into() {
            let txid = read_slice(current, 32)?;
            current = txid.remaining();
            consumed += 32;
        }
        let (slice, remaining) = slice.split_at(consumed);
        let block_extra = BlockExtra { slice };
        Ok(ParseResult::new(remaining, block_extra))
    }
}

#[cfg(test)]
mod test {

    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::hashes::hex::FromHex;
    use bitcoin_slices::Parse;

    #[test]
    fn test_bsl_block_extra() {
        let be = crate::block_extra::test::block_extra();
        let hex = serialize_hex(&be);
        assert_eq!(hex, "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");
        let bytes = Vec::<u8>::from_hex(&hex).unwrap();
        let block_extra = super::BlockExtra::parse(&bytes[..]).unwrap();
        assert_eq!(block_extra.consumed(), 216);
        assert_eq!(block_extra.remaining(), &[]);
    }
}
