//! Compressed streams and tar archives, read from memory with caps.
//!
//! Everything here takes bytes that came from outside (a package manager's
//! sync database, a download) and refuses rather than trusts: a stream that
//! would inflate past the caller's cap is an error, not an allocation, and a
//! tar header whose checksum or sizes do not add up ends the listing with an
//! error instead of reading past the buffer.

use std::io::Read;

/// A compression format [`decompress`] understands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Compression {
    /// RFC 1952, what `.gz` files are; concatenated members are read as one.
    Gzip,
    /// RFC 1950, a deflate stream with a two-byte header.
    Zlib,
    /// RFC 1951, a bare deflate stream.
    Deflate,
    /// Zstandard frames, concatenated frames included.
    Zstd,
    /// The `.xz` container.
    Xz,
    /// The legacy `.lzma` ("lzma_alone") container.
    Lzma,
}

impl Compression {
    /// Reads a format's name as a configuration spells it.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "gzip" | "gz" => Self::Gzip,
            "zlib" => Self::Zlib,
            "deflate" => Self::Deflate,
            "zstd" | "zst" => Self::Zstd,
            "xz" => Self::Xz,
            "lzma" => Self::Lzma,
            _ => return None,
        })
    }

    /// The name [`Compression::parse`] reads back.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Zlib => "zlib",
            Self::Deflate => "deflate",
            Self::Zstd => "zstd",
            Self::Xz => "xz",
            Self::Lzma => "lzma",
        }
    }
}

/// The format a buffer's magic number says it is in, if any. Bare deflate
/// and lzma have no magic and are never detected; zlib is detected by its
/// header checksum.
pub fn detect(bytes: &[u8]) -> Option<Compression> {
    match bytes {
        [0x1f, 0x8b, ..] => Some(Compression::Gzip),
        [0x28, 0xb5, 0x2f, 0xfd, ..] => Some(Compression::Zstd),
        [0xfd, b'7', b'z', b'X', b'Z', 0x00, ..] => Some(Compression::Xz),
        [cmf, flg, ..]
            if cmf & 0x0f == 8
                && cmf >> 4 <= 7
                && (u16::from(*cmf) << 8 | u16::from(*flg)) % 31 == 0 =>
        {
            Some(Compression::Zlib)
        }
        _ => None,
    }
}

/// Inflates `bytes` as `format`, refusing an output longer than `max_output`.
pub fn decompress(bytes: &[u8], format: Compression, max_output: usize) -> Result<Vec<u8>, String> {
    let too_big = || format!("{} output exceeds {max_output} bytes", format.name());
    match format {
        Compression::Gzip => {
            read_capped(flate2::read::MultiGzDecoder::new(bytes), max_output, format)
        }
        Compression::Zlib => read_capped(flate2::read::ZlibDecoder::new(bytes), max_output, format),
        Compression::Deflate => {
            read_capped(flate2::read::DeflateDecoder::new(bytes), max_output, format)
        }
        Compression::Zstd => {
            let decoder = ruzstd::decoding::StreamingDecoder::new(bytes)
                .map_err(|error| format!("zstd: {error}"))?;
            // A streaming decoder reads one frame; concatenated frames follow.
            let mut out = read_capped(decoder, max_output, format)?;
            let mut rest = frames_after_first(bytes);
            while let Some(tail) = rest.filter(|tail| !tail.is_empty()) {
                let mut decoder = ruzstd::decoding::StreamingDecoder::new(tail)
                    .map_err(|error| format!("zstd: {error}"))?;
                let room = max_output.saturating_sub(out.len());
                let mut more = Vec::new();
                (&mut decoder)
                    .take(room as u64 + 1)
                    .read_to_end(&mut more)
                    .map_err(|error| format!("zstd: {error}"))?;
                if more.len() > room {
                    return Err(too_big());
                }
                out.extend_from_slice(&more);
                rest = frames_after_first(tail);
            }
            Ok(out)
        }
        Compression::Xz | Compression::Lzma => {
            let mut input = bytes;
            let mut out = CappedWriter {
                data: Vec::new(),
                cap: max_output,
                over: false,
            };
            let result = if format == Compression::Xz {
                lzma_rs::xz_decompress(&mut input, &mut out)
            } else {
                lzma_rs::lzma_decompress(&mut input, &mut out)
            };
            if out.over {
                return Err(too_big());
            }
            result.map_err(|error| format!("{}: {error}", format.name()))?;
            Ok(out.data)
        }
    }
}

/// The input after the first zstd frame, or `None` when it cannot be told.
fn frames_after_first(bytes: &[u8]) -> Option<&[u8]> {
    let mut cursor = bytes;
    let mut frame = ruzstd::decoding::FrameDecoder::new();
    // Skippable frames and real ones alike: find where this one ends by
    // decoding it into nothing; ruzstd has no cheaper way to measure one.
    frame.reset(&mut cursor).ok()?;
    frame
        .decode_blocks(&mut cursor, ruzstd::decoding::BlockDecodingStrategy::All)
        .ok()?;
    Some(cursor)
}

fn read_capped(
    reader: impl Read,
    max_output: usize,
    format: Compression,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    reader
        .take(max_output as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|error| format!("{}: {error}", format.name()))?;
    if out.len() > max_output {
        return Err(format!(
            "{} output exceeds {max_output} bytes",
            format.name()
        ));
    }
    Ok(out)
}

struct CappedWriter {
    data: Vec<u8>,
    cap: usize,
    over: bool,
}

impl std::io::Write for CappedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.data.len() + buf.len() > self.cap {
            self.over = true;
            return Err(std::io::Error::other("output cap reached"));
        }
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// One member of a tar archive; its bytes are `data` of the archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TarEntry {
    /// The path, with a pax or GNU long name applied.
    pub name: String,
    /// `file`, `directory`, `symlink`, `hardlink`, `char`, `block`, `fifo`.
    pub kind: &'static str,
    /// The permission bits.
    pub mode: u32,
    /// Seconds since the epoch.
    pub mtime: i64,
    /// The size of the member's data in bytes.
    pub size: u64,
    /// A link's target; empty for anything else.
    pub link: String,
    /// Where the member's data sits in the archive.
    pub data: std::ops::Range<usize>,
}

const BLOCK: usize = 512;

/// Lists a tar archive (ustar, GNU long names, pax paths), at most
/// `max_entries` members. Global pax headers are skipped.
pub fn tar_entries(bytes: &[u8], max_entries: usize) -> Result<Vec<TarEntry>, String> {
    let mut entries = Vec::new();
    let mut at = 0usize;
    let mut long_name: Option<String> = None;
    let mut long_link: Option<String> = None;
    let mut pax: Vec<(String, String)> = Vec::new();
    while at + BLOCK <= bytes.len() {
        let header = &bytes[at..at + BLOCK];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        if !checksum_ok(header) {
            return Err(format!("tar: bad header checksum at byte {at}"));
        }
        let size = octal(&header[124..136]).ok_or_else(|| format!("tar: bad size at byte {at}"))?;
        let start = at + BLOCK;
        let size_usize = usize::try_from(size).map_err(|_| "tar: member too large".to_string())?;
        let end = start
            .checked_add(size_usize)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| format!("tar: member at byte {at} runs past the end"))?;
        let data = start..end;
        at = start + size_usize.div_ceil(BLOCK) * BLOCK;
        let flag = header[156];
        match flag {
            b'L' => {
                long_name = Some(c_string(&bytes[data]));
                continue;
            }
            b'K' => {
                long_link = Some(c_string(&bytes[data]));
                continue;
            }
            b'x' => {
                pax = pax_records(&bytes[data]);
                continue;
            }
            b'g' => continue,
            _ => {}
        }
        let kind = match flag {
            b'0' | 0 | b'7' => "file",
            b'1' => "hardlink",
            b'2' => "symlink",
            b'3' => "char",
            b'4' => "block",
            b'5' => "directory",
            b'6' => "fifo",
            _ => "other",
        };
        let mut name = c_string(&header[0..100]);
        if &header[257..262] == b"ustar" {
            let prefix = c_string(&header[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        let mut link = c_string(&header[157..257]);
        let mut size = size;
        let mut data = data;
        if let Some(long) = long_name.take() {
            name = long;
        }
        if let Some(long) = long_link.take() {
            link = long;
        }
        for (key, value) in pax.drain(..) {
            match key.as_str() {
                "path" => name = value,
                "linkpath" => link = value,
                "size" => {
                    // A pax size overrides the header's (for members over 8 GiB).
                    if let Ok(pax_size) = value.parse::<u64>() {
                        let pax_end = usize::try_from(pax_size)
                            .ok()
                            .and_then(|len| data.start.checked_add(len))
                            .filter(|end| *end <= bytes.len())
                            .ok_or_else(|| "tar: pax size runs past the end".to_string())?;
                        size = pax_size;
                        at = data.start + (pax_end - data.start).div_ceil(BLOCK) * BLOCK;
                        data = data.start..pax_end;
                    }
                }
                _ => {}
            }
        }
        if kind == "directory" && !name.ends_with('/') {
            name.push('/');
        }
        if entries.len() == max_entries {
            return Err(format!("tar: more than {max_entries} members"));
        }
        entries.push(TarEntry {
            name,
            kind,
            mode: octal(&header[100..108]).unwrap_or(0) as u32,
            mtime: octal(&header[136..148]).unwrap_or(0) as i64,
            size,
            link,
            data,
        });
    }
    Ok(entries)
}

fn checksum_ok(header: &[u8]) -> bool {
    let Some(stored) = octal(&header[148..156]) else {
        return false;
    };
    let sum: u64 = header
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                32
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    sum == stored
}

/// A numeric header field: octal digits, NUL- or space-terminated, or GNU's
/// base-256 form (high bit set) for values that do not fit.
fn octal(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        let mut value = u64::from(field[0] & 0x7f);
        for byte in &field[1..] {
            value = value.checked_mul(256)?.checked_add(u64::from(*byte))?;
        }
        return Some(value);
    }
    let text = field
        .iter()
        .skip_while(|byte| **byte == b' ')
        .take_while(|byte| **byte != 0 && **byte != b' ')
        .map(|byte| *byte as char)
        .collect::<String>();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(&text, 8).ok()
}

fn c_string(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// `"<len> key=value\n"` records.
fn pax_records(mut data: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let Some(space) = data.iter().position(|byte| *byte == b' ') else {
            break;
        };
        let Some(length) = std::str::from_utf8(&data[..space])
            .ok()
            .and_then(|text| text.parse::<usize>().ok())
            .filter(|length| *length > space + 1 && *length <= data.len())
        else {
            break;
        };
        let record = &data[space + 1..length];
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        if let Some(equals) = record.iter().position(|byte| *byte == b'=') {
            out.push((
                String::from_utf8_lossy(&record[..equals]).into_owned(),
                String::from_utf8_lossy(&record[equals + 1..]).into_owned(),
            ));
        }
        data = &data[length..];
    }
    out
}

#[cfg(test)]
mod tests {
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
        0x28, 0xb5, 0x2f, 0xfd, 0x04, 0x58, 0x59, 0x00, 0x00, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20,
        0x7a, 0x73, 0x74, 0x64, 0x0a, 0x6c, 0x57, 0xf9, 0x51,
    ];

    // `printf 'hello xz\n' | xz -c | xxd -i`
    const XZ_HELLO: &[u8] = &[
        0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x04, 0xc0, 0x0d,
        0x09, 0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5f, 0x4f,
        0x33, 0xe4, 0x01, 0x00, 0x08, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x78, 0x7a, 0x0a, 0x00,
        0x00, 0x00, 0x00, 0xc1, 0x49, 0x3a, 0xfa, 0x63, 0x52, 0x14, 0x5a, 0x00, 0x01, 0x29, 0x09,
        0x64, 0x92, 0x1c, 0x1d, 0x1f, 0xb6, 0xf3, 0x7d, 0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59,
        0x5a,
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
}
