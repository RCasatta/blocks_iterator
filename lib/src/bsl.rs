use bitcoin_slices::bsl::parse_len;
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
        let version_int: u8 = version.parsed().into();
        dbg!(version_int);
        let mut consumed = 1;

        let block_size = if version_int == 0 {
            let block = bsl::Block::visit(&slice[consumed..], visit)?;
            consumed += block.consumed();
            None
        } else if version_int == 1 {
            let block_size = U32::parse(&slice[consumed..])?;
            consumed += 4;
            let block = bsl::Block::visit(&slice[consumed..], visit)?;
            consumed += block.consumed();
            Some(block_size)
        } else {
            panic!("invalid version")
        };
        dbg!(consumed);

        let block_hash = read_slice(&slice[consumed..], 32)?;
        consumed += 32;

        if block_size.is_none() {
            let _ = U32::parse(block_hash.remaining())?;
            consumed += 4;
        }

        dbg!(consumed);
        let next_len = parse_len(&slice[consumed..])?;
        consumed += next_len.consumed();

        for _ in 0..next_len.n() {
            let _ = read_slice(&slice[consumed..], 32)?;
            consumed += 32;
        }

        let _ = U32::parse(&slice[consumed..])?;
        consumed += 4;

        let map_len = U32::parse(&slice[consumed..])?;
        consumed += 4;

        for _ in 0u32..map_len.parsed().into() {
            // add visit extra call
            let outpoint = bsl::OutPoint::parse(&slice[consumed..])?;
            consumed += outpoint.consumed();
            let txout = bsl::TxOut::parse(&slice[consumed..])?;
            consumed += txout.consumed();
        }
        let _ = U32::parse(&slice[consumed..])?;
        consumed += 4;
        let _ = U32::parse(&slice[consumed..])?;
        consumed += 4;
        let txids_len = U32::parse(&slice[consumed..])?;
        consumed += 4;

        for _ in 0u32..txids_len.parsed().into() {
            let _ = read_slice(&slice[consumed..], 32)?;
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
        let be0 = crate::block_extra::test::block_extra();
        let hex0 = serialize_hex(&be0);
        assert_eq!(hex0, "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000005100000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");
        let bytes0 = Vec::<u8>::from_hex(&hex0).unwrap();
        let block_extra0 = super::BlockExtra::parse(&bytes0[..]).unwrap();
        assert_eq!(block_extra0.consumed(), 216);
        assert_eq!(block_extra0.remaining(), &[]);

        let mut be1 = be0;
        be1.version = 1;
        let hex1 = serialize_hex(&be1);
        assert_eq!(hex1, "0151000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffff00000000000000000000000000");
        assert_ne!(hex0, hex1);
        let bytes1 = Vec::<u8>::from_hex(&hex1).unwrap();
        let block_extra1 = super::BlockExtra::parse(&bytes1[..]).unwrap();
        assert_eq!(block_extra1.consumed(), 216);
        assert_eq!(block_extra1.remaining(), &[]);
    }
}
