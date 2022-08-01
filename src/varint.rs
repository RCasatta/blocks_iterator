use std::io::{self, Read};

/**
 * Variable-length integers: bytes are a MSB base-128 encoding of the number.
 * The high bit in each byte signifies whether another digit follows. To make
 * sure the encoding is one-to-one, one is subtracted from all but the last digit.
 * Thus, the byte sequence a[] with length len, where all but the last byte
 * has bit 128 set, encodes the number:
 *
 *  (a[len-1] & 0x7F) + sum(i=1..len-1, 128^i*((a[len-i-1] & 0x7F)+1))
 *
 * Properties:
 * * Very small (0-127: 1 byte, 128-16511: 2 bytes, 16512-2113663: 3 bytes)
 * * Every integer has exactly one encoding
 * * Encoding does not depend on size of original integer type
 * * No redundancy: every (infinite) byte sequence corresponds to a list
 *   of encoded integers.
 *
 */

pub fn read_true_var_int(mut reader: impl Read) -> Result<u64, io::Error> {
    let mut buf = [0u8; 1];
    let mut acc = 0u64;

    // TODO handle greater than u64
    loop {
        reader.read_exact(&mut buf)?;
        let next_byte = buf[0];
        acc = (acc << 7) | ((next_byte & 0b01111111) as u64);
        if next_byte.leading_ones() > 0 {
            acc += 1;
        } else {
            return Ok(acc);
        }
    }

    // 137 bytes->
    // 5121030b3810fd20fd3771517b2b8847d225791035ea06768e17c733a5756b6005bf55210222b6e887bb4d4bca08f97348e6b8561e6d11e0ed96dec0584b34d709078cd4a54104289699814d1c9ef35ae45cfb41116501c15b0141430a481226aa19bcb8806c7223802d24f2638d8ce14378137dd52114d1d965e2969b5b3ac011c25e2803eb5753ae
}

#[cfg(test)]
mod test {
    use super::read_true_var_int;

    #[test]
    fn test_varint() {
        assert_eq!(read_true_var_int(&[0][..]).unwrap(), 0);
        assert_eq!(read_true_var_int(&[1][..]).unwrap(), 1);
        assert_eq!(read_true_var_int(&[0x7f][..]).unwrap(), 127);
        assert_eq!(read_true_var_int(&[0x80, 0x00][..]).unwrap(), 128);
        assert_eq!(read_true_var_int(&[0x80, 0x7F][..]).unwrap(), 255);
        assert_eq!(read_true_var_int(&[0x81, 0x00][..]).unwrap(), 256);
        assert_eq!(read_true_var_int(&[0xfe, 0x7F][..]).unwrap(), 16383);
        assert_eq!(read_true_var_int(&[0xff, 0x00][..]).unwrap(), 16384);
        assert_eq!(read_true_var_int(&[0xff, 0x7F][..]).unwrap(), 16511);
        assert_eq!(read_true_var_int(&[0x82, 0xfe, 0x7F][..]).unwrap(), 65535);
        assert_eq!(
            read_true_var_int(&[0x8e, 0xfe, 0xfe, 0xff, 0x00][..]).unwrap(),
            2u64.pow(32)
        );

        /*
         * 0:         [0x00]  256:        [0x81 0x00]
         * 1:         [0x01]  16383:      [0xFE 0x7F]
         * 127:       [0x7F]  16384:      [0xFF 0x00]
         * 128:  [0x80 0x00]  16511:      [0xFF 0x7F]
         * 255:  [0x80 0x7F]  65535: [0x82 0xFE 0x7F]
         * 2^32:           [0x8E 0xFE 0xFE 0xFF 0x00]
         */
    }
}
