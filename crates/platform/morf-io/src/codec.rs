//! Byte encodings and digests a configuration needs to talk to the world:
//! base64 and hex for payloads, percent-encoding for URLs, SHA-256 and
//! SHA-1 for content addresses and cache keys, CRC-32 for cheap change
//! detection, and random bytes from the kernel.
//!
//! Written out rather than pulled in: each is a page, fixed by a standard,
//! and tested here against that standard's own vectors.

use std::fmt::Write as _;

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64, standard or URL-safe alphabet, padded or not.
pub fn base64_encode(bytes: &[u8], url_safe: bool, pad: bool) -> String {
    let alphabet = if url_safe { URL_SAFE } else { STANDARD };
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = [
            alphabet[(n >> 18) as usize & 63],
            alphabet[(n >> 12) as usize & 63],
            alphabet[(n >> 6) as usize & 63],
            alphabet[n as usize & 63],
        ];
        let keep = chunk.len() + 1;
        for (index, c) in chars.iter().enumerate() {
            if index < keep {
                out.push(*c as char);
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

/// Either alphabet, padded or not, whitespace ignored.
pub fn base64_decode(text: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0;
    let mut padding = false;
    for (position, &c) in text.iter().enumerate() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => {
                padding = true;
                continue;
            }
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return Err(format!("base64: invalid byte {c:#04x} at {position}")),
        };
        if padding {
            return Err("base64: data after padding".into());
        }
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    if bits >= 6 {
        return Err("base64: truncated input".into());
    }
    Ok(out)
}

pub fn hex_encode(bytes: &[u8], upper: bool) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        if upper {
            let _ = write!(out, "{byte:02X}");
        } else {
            let _ = write!(out, "{byte:02x}");
        }
    }
    out
}

pub fn hex_decode(text: &[u8]) -> Result<Vec<u8>, String> {
    let digits = text
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if digits.len() % 2 != 0 {
        return Err("hex: odd number of digits".into());
    }
    digits
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16);
            let low = (pair[1] as char).to_digit(16);
            match (high, low) {
                (Some(high), Some(low)) => Ok((high * 16 + low) as u8),
                _ => Err(format!(
                    "hex: invalid digits {:?}",
                    String::from_utf8_lossy(pair)
                )),
            }
        })
        .collect()
}

/// Percent-encoding. `component` keeps only the unreserved set
/// (`A-Z a-z 0-9 - _ . ~`), as a query value needs; otherwise `/` and
/// `:` survive too, as a path does. `plus` writes a space as `+`, as a
/// form does.
pub fn url_encode(bytes: &[u8], component: bool, plus: bool) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (!component
                && matches!(
                    byte,
                    b'/' | b':'
                        | b'@'
                        | b'!'
                        | b'$'
                        | b'&'
                        | b'\''
                        | b'('
                        | b')'
                        | b'*'
                        | b'+'
                        | b','
                        | b';'
                        | b'='
                ));
        if keep {
            out.push(byte as char);
        } else if plus && byte == b' ' {
            out.push('+');
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Undoes percent-encoding; `plus` reads `+` as a space. A `%` not
/// followed by two hex digits is kept as it is.
pub fn url_decode(text: &[u8], plus: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let byte = text[index];
        if byte == b'%'
            && index + 2 < text.len()
            && let (Some(high), Some(low)) = (
                (text[index + 1] as char).to_digit(16),
                (text[index + 2] as char).to_digit(16),
            )
        {
            out.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        out.push(if plus && byte == b'+' { b' ' } else { byte });
        index += 1;
    }
    out
}

/// SHA-256 (FIPS 180-4).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in padded(bytes, false).chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_mut(4).zip(h) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// SHA-1 (FIPS 180-4). Broken for signatures; still what git and many
/// caches name content by.
pub fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for block in padded(bytes, false).chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (chunk, word) in out.chunks_mut(4).zip(h) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// The message, a one bit, zeros, and its length in bits: the padding
/// both digests share (big-endian length; `little` is for MD-style).
fn padded(bytes: &[u8], little: bool) -> Vec<u8> {
    let mut message = bytes.to_vec();
    let bits = (bytes.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    if little {
        message.extend_from_slice(&bits.to_le_bytes());
    } else {
        message.extend_from_slice(&bits.to_be_bytes());
    }
    message
}

/// CRC-32 (IEEE, as zlib and PNG use).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Bytes from the kernel's random source.
pub fn random_bytes(count: usize) -> std::io::Result<Vec<u8>> {
    let mut out = vec![0u8; count];
    let mut filled = 0;
    while filled < count {
        filled +=
            rustix::rand::getrandom(&mut out[filled..], rustix::rand::GetRandomFlags::empty())
                .map_err(std::io::Error::from)?;
    }
    Ok(out)
}

/// A random (version 4) UUID, lower-case and hyphenated.
pub fn uuid_v4() -> std::io::Result<String> {
    let mut bytes = random_bytes(16)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex_encode(&bytes, false);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_rfc4648_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(plain.as_bytes(), false, true), coded);
            assert_eq!(base64_decode(coded.as_bytes()).unwrap(), plain.as_bytes());
            assert_eq!(
                base64_decode(coded.trim_end_matches('=').as_bytes()).unwrap(),
                plain.as_bytes()
            );
        }
        assert_eq!(base64_encode(&[0xfb, 0xff], true, false), "-_8");
        assert_eq!(base64_decode(b"-_8").unwrap(), [0xfb, 0xff]);
        assert!(base64_decode(b"Zm9v!").is_err());
        assert!(base64_decode(b"Z").is_err());
    }

    #[test]
    fn hex_and_url_codecs() {
        assert_eq!(hex_encode(&[0, 171, 255], false), "00abff");
        assert_eq!(hex_decode(b"00ABff").unwrap(), [0, 171, 255]);
        assert!(hex_decode(b"abc").is_err());
        assert_eq!(
            url_encode("a b&c/é".as_bytes(), true, false),
            "a%20b%26c%2F%C3%A9"
        );
        assert_eq!(url_encode(b"a b/c", false, true), "a+b/c");
        assert_eq!(url_decode(b"a%20b%26c+d%zz%4", true), b"a b&c d%zz%4");
    }

    #[test]
    fn digests_match_the_standard_vectors() {
        assert_eq!(
            hex_encode(&sha256(b""), false),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_encode(&sha256(b"abc"), false),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_encode(
                &sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
                false
            ),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex_encode(&sha1(b"abc"), false),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex_encode(&sha1(b""), false),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            hex_encode(&sha256(&million), false),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn uuids_are_version_four() {
        let id = uuid_v4().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
        assert_ne!(id, uuid_v4().unwrap());
    }
}
