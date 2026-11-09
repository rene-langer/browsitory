//! Standard base64 (RFC 4648 §4, with `=` padding), for sending file bytes as JSON text (the
//! image preview's `data:` URLs, patch files: #448). Hand-rolled to avoid a direct dependency.

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(char::from(TABLE[(n >> 18) as usize & 63]));
        out.push(char::from(TABLE[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(TABLE[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(TABLE[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

/// The bytes of standard, padded base64 `text`; `None` for anything else. ASCII whitespace
/// (a wrapped encoding) is skipped.
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| TABLE.iter().position(|&t| t == c).map(|v| v as u32);
    let symbols: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !symbols.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(symbols.len() / 4 * 3);
    let quads = symbols.len() / 4;
    for (index, quad) in symbols.chunks(4).enumerate() {
        let pad = quad.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && index + 1 != quads) {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in quad[..4 - pad].iter().enumerate() {
            n |= value(c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn decode_round_trips_and_refuses_garbage() {
        for len in 0..40u8 {
            let bytes: Vec<u8> = (0..len).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(super::decode(&encode(&bytes)), Some(bytes));
        }
        assert_eq!(super::decode("Zm9v\nYmFy"), Some(b"foobar".to_vec()));
        assert_eq!(super::decode("Zm9"), None);
        assert_eq!(super::decode("Zm=v"), None);
        assert_eq!(super::decode("Zg==Zm9v"), None);
        assert_eq!(super::decode("Z!=="), None);
    }

    /// A straightforward decoder, only to check round trips.
    fn decode(text: &str) -> Vec<u8> {
        let value = |c: u8| super::TABLE.iter().position(|&t| t == c).unwrap() as u32;
        assert_eq!(text.len() % 4, 0, "length of {text:?}");
        let mut out = Vec::new();
        for quad in text.as_bytes().chunks(4) {
            let pad = quad.iter().filter(|&&c| c == b'=').count();
            assert!(
                quad[..4 - pad].iter().all(|&c| c != b'='),
                "padding inside {text:?}"
            );
            let n = quad[..4 - pad]
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &c)| n | value(c) << (18 - 6 * i));
            out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8][..3 - pad]);
        }
        out
    }

    #[test]
    fn matches_the_rfc_4648_test_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(input.as_bytes()), expected, "{input:?}");
        }
    }

    #[test]
    fn uses_the_standard_alphabet_for_the_last_two_symbols() {
        assert_eq!(encode(&[0xff, 0xfe, 0xfd]), "//79");
        assert_eq!(encode(&[0xfb, 0xff]), "+/8=");
        assert_eq!(encode(&[0x00]), "AA==");
        assert_eq!(encode(&[0x00, 0x00]), "AAA=");
        assert_eq!(encode(&[0x00, 0x00, 0x00]), "AAAA");
        assert_eq!(encode(&[0xff]), "/w==");
    }

    #[test]
    fn padding_follows_the_length_mod_3() {
        for len in 0..12 {
            let encoded = encode(&vec![0xa5; len]);
            assert_eq!(encoded.len(), len.div_ceil(3) * 4, "len {len}");
            let pad = encoded.chars().rev().take_while(|&c| c == '=').count();
            assert_eq!(pad, (3 - len % 3) % 3, "len {len}");
        }
    }

    #[test]
    fn round_trips_every_byte_value_at_every_alignment() {
        let all: Vec<u8> = (0..=255u8).collect();
        for start in 0..3 {
            for end in all.len() - 3..=all.len() {
                let slice = &all[start..end];
                let encoded = encode(slice);
                assert!(encoded
                    .bytes()
                    .all(|c| c == b'=' || super::TABLE.contains(&c)));
                assert_eq!(decode(&encoded), slice, "{start}..{end}");
            }
        }
        // A pseudo-random buffer of a few KB.
        let mut state = 0x2545_f491_u32;
        let noise: Vec<u8> = (0..4099)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        assert_eq!(decode(&encode(&noise)), noise);
    }
}
