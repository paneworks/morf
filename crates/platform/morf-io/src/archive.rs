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

/// Largest output a decompression may produce unless the caller says less.
pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024 * 1024;
/// Largest output a caller may ask a decompression for.
pub const MAX_OUTPUT: usize = 512 * 1024 * 1024;
/// Most tar members listed unless the caller says otherwise.
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;
/// Most tar members a caller may ask for.
pub const MAX_ENTRIES: usize = 1_000_000;

/// Inflates `bytes` as the format `name` spells (`None` or `"auto"`
/// detects it from the magic number).
pub fn inflate(bytes: &[u8], name: Option<&str>, max_output: usize) -> Result<Vec<u8>, String> {
    let format = match name {
        None | Some("auto") => detect(bytes).ok_or_else(|| {
            "unknown compression format (give one: gzip, zlib, deflate, zstd, xz, lzma)".to_string()
        })?,
        Some(name) => {
            Compression::parse(name).ok_or_else(|| format!("unknown compression format {name:?}"))?
        }
    };
    decompress(bytes, format, max_output)
}

/// A tar archive's bytes: inflated first when a magic number says so.
pub fn tar_bytes(bytes: &[u8], max_output: usize) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    match detect(bytes) {
        Some(format) => decompress(bytes, format, max_output).map(std::borrow::Cow::Owned),
        None => Ok(std::borrow::Cow::Borrowed(bytes)),
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
mod tests;
