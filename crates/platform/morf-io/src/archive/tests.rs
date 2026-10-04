//! Tests for the archive readers: tar listing, corruption, and capped decoding.

use super::*;
use std::io::Write;

fn header(name: &str, flag: u8, size: usize) -> [u8; BLOCK] {
    let mut block = [0u8; BLOCK];
    block[..name.len()].copy_from_slice(name.as_bytes());
    block[100..107].copy_from_slice(b"0000644");
    block[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
    block[136..147].copy_from_slice(b"14700000000");
    block[156] = flag;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    block[148..156].copy_from_slice(b"        ");
    let sum: u32 = block.iter().map(|byte| u32::from(*byte)).sum();
    block[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
    block
}

fn member(out: &mut Vec<u8>, name: &str, flag: u8, data: &[u8]) {
    out.extend_from_slice(&header(name, flag, data.len()));
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(BLOCK) * BLOCK, 0);
}

fn sample_tar() -> Vec<u8> {
    let mut tar = Vec::new();
    member(&mut tar, "pkg-1.0-1/", b'5', b"");
    member(
        &mut tar,
        "pkg-1.0-1/desc",
        b'0',
        b"%NAME%\npkg\n\n%VERSION%\n1.0-1\n",
    );
    let long = "a/".repeat(80) + "deep";
    member(
        &mut tar,
        "././@LongLink",
        b'L',
        format!("{long}\0").as_bytes(),
    );
    member(&mut tar, "truncated", b'0', b"x");
    let pax = "20 path=pax/renamed\n";
    member(&mut tar, "PaxHeader", b'x', pax.as_bytes());
    member(&mut tar, "short", b'0', b"yz");
    tar.extend_from_slice(&[0u8; BLOCK * 2]);
    tar
}

#[test]
fn tar_lists_members_with_long_names() {
    let tar = sample_tar();
    let entries = tar_entries(&tar, 100).unwrap();
    let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names[0], "pkg-1.0-1/");
    assert_eq!(entries[0].kind, "directory");
    assert_eq!(names[1], "pkg-1.0-1/desc");
    assert_eq!(
        &tar[entries[1].data.clone()],
        b"%NAME%\npkg\n\n%VERSION%\n1.0-1\n"
    );
    assert_eq!(entries[1].mode, 0o644);
    assert!(names[2].ends_with("deep") && names[2].len() == 164);
    assert_eq!(&tar[entries[2].data.clone()], b"x");
    assert_eq!(names[3], "pax/renamed");
    assert_eq!(entries.len(), 4);
    assert!(tar_entries(&tar, 3).unwrap_err().contains("more than 3"));
}

#[test]
fn tar_refuses_corruption() {
    let mut tar = sample_tar();
    tar[10] ^= 1;
    assert!(tar_entries(&tar, 100).unwrap_err().contains("checksum"));
    // A size field claiming more than there is.
    let mut bad = Vec::new();
    bad.extend_from_slice(&header("big", b'0', 4096));
    bad.extend_from_slice(&[0u8; BLOCK]);
    assert!(tar_entries(&bad, 100).unwrap_err().contains("past the end"));
}

#[test]
fn gzip_and_zlib_round_trip_and_cap() {
    let text = b"hello hello hello hello".repeat(100);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&text).unwrap();
    let gz = gz.finish().unwrap();
    assert_eq!(detect(&gz), Some(Compression::Gzip));
    assert_eq!(decompress(&gz, Compression::Gzip, 1 << 20).unwrap(), text);
    assert!(
        decompress(&gz, Compression::Gzip, 100)
            .unwrap_err()
            .contains("exceeds")
    );

    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&text).unwrap();
    let z = z.finish().unwrap();
    assert_eq!(detect(&z), Some(Compression::Zlib));
    assert_eq!(decompress(&z, Compression::Zlib, 1 << 20).unwrap(), text);
    assert!(decompress(b"not gzip", Compression::Gzip, 100).is_err());
    assert_eq!(detect(b"plain text"), None);
}

// `printf 'hello zstd\n' | zstd -c | xxd -i`
const ZSTD_HELLO: &[u8] = &[
    0x28, 0xb5, 0x2f, 0xfd, 0x04, 0x58, 0x59, 0x00, 0x00, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x7a,
    0x73, 0x74, 0x64, 0x0a, 0x6c, 0x57, 0xf9, 0x51,
];

// `printf 'hello xz\n' | xz -c | xxd -i`
const XZ_HELLO: &[u8] = &[
    0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x04, 0xc0, 0x0d, 0x09,
    0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5f, 0x4f, 0x33, 0xe4,
    0x01, 0x00, 0x08, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x78, 0x7a, 0x0a, 0x00, 0x00, 0x00, 0x00,
    0xc1, 0x49, 0x3a, 0xfa, 0x63, 0x52, 0x14, 0x5a, 0x00, 0x01, 0x29, 0x09, 0x64, 0x92, 0x1c, 0x1d,
    0x1f, 0xb6, 0xf3, 0x7d, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59, 0x5a,
];

#[test]
fn zstd_and_xz_decode_and_cap() {
    assert_eq!(detect(ZSTD_HELLO), Some(Compression::Zstd));
    assert_eq!(
        decompress(ZSTD_HELLO, Compression::Zstd, 100).unwrap(),
        b"hello zstd\n"
    );
    assert!(
        decompress(ZSTD_HELLO, Compression::Zstd, 4)
            .unwrap_err()
            .contains("exceeds")
    );
    let twice = [ZSTD_HELLO, ZSTD_HELLO].concat();
    assert_eq!(
        decompress(&twice, Compression::Zstd, 100).unwrap(),
        b"hello zstd\nhello zstd\n"
    );

    assert_eq!(detect(XZ_HELLO), Some(Compression::Xz));
    assert_eq!(
        decompress(XZ_HELLO, Compression::Xz, 100).unwrap(),
        b"hello xz\n"
    );
    assert!(
        decompress(XZ_HELLO, Compression::Xz, 4)
            .unwrap_err()
            .contains("exceeds")
    );
    assert!(decompress(&XZ_HELLO[..20], Compression::Xz, 100).is_err());
}
