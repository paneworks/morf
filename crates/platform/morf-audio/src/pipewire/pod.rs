//! SPA PODs: the self-describing binary values PipeWire parameters are made of.
//!
//! Every POD is a header — body size and type, two `u32`s — and a body
//! padded to eight bytes. Objects are a type, an id and a run of keyed
//! properties; arrays are one child header and then bare bodies. This is the
//! subset volumes, mutes, routes and audio formats need, written and read
//! without libspa's inline builders.

#![allow(dead_code)]

pub const TYPE_NONE: u32 = 1;
pub const TYPE_BOOL: u32 = 2;
pub const TYPE_ID: u32 = 3;
pub const TYPE_INT: u32 = 4;
pub const TYPE_LONG: u32 = 5;
pub const TYPE_FLOAT: u32 = 6;
pub const TYPE_DOUBLE: u32 = 7;
pub const TYPE_STRING: u32 = 8;
pub const TYPE_ARRAY: u32 = 13;
pub const TYPE_STRUCT: u32 = 14;
pub const TYPE_OBJECT: u32 = 15;
pub const TYPE_CHOICE: u32 = 19;

pub const OBJECT_PROPS: u32 = 0x40002;
pub const OBJECT_FORMAT: u32 = 0x40003;
pub const OBJECT_ROUTE: u32 = 0x40009;

/// `enum spa_prop`.
pub const PROP_MUTE: u32 = 0x10004;
pub const PROP_CHANNEL_VOLUMES: u32 = 0x10008;

/// `enum spa_param_route`.
pub const ROUTE_INDEX: u32 = 1;
pub const ROUTE_DIRECTION: u32 = 2;
pub const ROUTE_DEVICE: u32 = 3;
pub const ROUTE_PROPS: u32 = 10;
pub const ROUTE_SAVE: u32 = 13;

/// `enum spa_format`.
pub const FORMAT_MEDIA_TYPE: u32 = 1;
pub const FORMAT_MEDIA_SUBTYPE: u32 = 2;
pub const FORMAT_AUDIO_FORMAT: u32 = 0x10001;
pub const FORMAT_AUDIO_RATE: u32 = 0x10003;
pub const FORMAT_AUDIO_CHANNELS: u32 = 0x10004;
pub const FORMAT_AUDIO_POSITION: u32 = 0x10005;

pub const MEDIA_TYPE_AUDIO: u32 = 1;
pub const MEDIA_SUBTYPE_RAW: u32 = 1;
/// `SPA_AUDIO_FORMAT_F32_LE`.
pub const AUDIO_FORMAT_F32_LE: u32 = 0x11b;
pub const AUDIO_CHANNEL_FL: u32 = 3;
pub const AUDIO_CHANNEL_FR: u32 = 4;

/// A POD value, as written or as read.
#[derive(Clone, Debug, PartialEq)]
pub enum Pod {
    None,
    Bool(bool),
    Id(u32),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    /// Each element's type, then the elements.
    Array(u32, Vec<Pod>),
    Struct(Vec<Pod>),
    Object {
        kind: u32,
        id: u32,
        properties: Vec<(u32, Pod)>,
    },
    /// Anything this module does not read.
    Other(u32),
}

impl Pod {
    pub fn object(kind: u32, id: u32, properties: Vec<(u32, Pod)>) -> Self {
        Self::Object {
            kind,
            id,
            properties,
        }
    }

    pub fn floats(values: &[f32]) -> Self {
        Self::Array(TYPE_FLOAT, values.iter().copied().map(Pod::Float).collect())
    }

    pub fn ids(values: &[u32]) -> Self {
        Self::Array(TYPE_ID, values.iter().copied().map(Pod::Id).collect())
    }

    /// One property of an object.
    pub fn property(&self, key: u32) -> Option<&Pod> {
        match self {
            Self::Object { properties, .. } => properties
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Id(value) => i32::try_from(*value).ok(),
            Self::Long(value) => i32::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_id(&self) -> Option<u32> {
        match self {
            Self::Id(value) => Some(*value),
            Self::Int(value) => u32::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_floats(&self) -> Option<Vec<f32>> {
        match self {
            Self::Array(_, values) => values
                .iter()
                .map(|value| match value {
                    Pod::Float(value) => Some(*value),
                    Pod::Double(value) => Some(*value as f32),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    fn type_id(&self) -> u32 {
        match self {
            Self::None => TYPE_NONE,
            Self::Bool(_) => TYPE_BOOL,
            Self::Id(_) => TYPE_ID,
            Self::Int(_) => TYPE_INT,
            Self::Long(_) => TYPE_LONG,
            Self::Float(_) => TYPE_FLOAT,
            Self::Double(_) => TYPE_DOUBLE,
            Self::String(_) => TYPE_STRING,
            Self::Array(..) => TYPE_ARRAY,
            Self::Struct(_) => TYPE_STRUCT,
            Self::Object { .. } => TYPE_OBJECT,
            Self::Other(kind) => *kind,
        }
    }

    /// The body alone, unpadded.
    fn body(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::None | Self::Other(_) => {}
            Self::Bool(value) => out.extend_from_slice(&i32::from(*value).to_ne_bytes()),
            Self::Id(value) => out.extend_from_slice(&value.to_ne_bytes()),
            Self::Int(value) => out.extend_from_slice(&value.to_ne_bytes()),
            Self::Long(value) => out.extend_from_slice(&value.to_ne_bytes()),
            Self::Float(value) => out.extend_from_slice(&value.to_ne_bytes()),
            Self::Double(value) => out.extend_from_slice(&value.to_ne_bytes()),
            Self::String(value) => {
                out.extend_from_slice(value.as_bytes());
                out.push(0);
            }
            Self::Array(kind, values) => {
                let child_size = values.first().map_or(4, |value| value.body().len());
                out.extend_from_slice(&(child_size as u32).to_ne_bytes());
                out.extend_from_slice(&kind.to_ne_bytes());
                for value in values {
                    out.extend_from_slice(&value.body());
                }
            }
            Self::Struct(values) => {
                for value in values {
                    value.write(&mut out);
                }
            }
            Self::Object {
                kind,
                id,
                properties,
            } => {
                out.extend_from_slice(&kind.to_ne_bytes());
                out.extend_from_slice(&id.to_ne_bytes());
                for (key, value) in properties {
                    out.extend_from_slice(&key.to_ne_bytes());
                    out.extend_from_slice(&0u32.to_ne_bytes());
                    value.write(&mut out);
                }
            }
        }
        out
    }

    fn write(&self, out: &mut Vec<u8>) {
        let body = self.body();
        out.extend_from_slice(&(body.len() as u32).to_ne_bytes());
        out.extend_from_slice(&self.type_id().to_ne_bytes());
        out.extend_from_slice(&body);
        while !out.len().is_multiple_of(8) {
            out.push(0);
        }
    }

    /// The whole POD, header and padding, in eight-byte words so a pointer
    /// to it is aligned the way libspa reads it.
    pub fn encode(&self) -> Encoded {
        let mut bytes = Vec::new();
        self.write(&mut bytes);
        let words = bytes
            .chunks(8)
            .map(|chunk| {
                let mut word = [0u8; 8];
                word[..chunk.len()].copy_from_slice(chunk);
                u64::from_ne_bytes(word)
            })
            .collect();
        Encoded { words }
    }

    /// Reads one POD from the start of `bytes`. `None` when it is cut short.
    pub fn decode(bytes: &[u8]) -> Option<Pod> {
        let (size, kind) = header(bytes)?;
        let body = bytes.get(8..8 + size)?;
        decode_body(kind, body, 0)
    }
}

/// An encoded POD, aligned.
pub struct Encoded {
    words: Vec<u64>,
}

impl Encoded {
    pub fn as_ptr(&self) -> *const super::ffi::SpaPod {
        self.words.as_ptr().cast()
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|word| word.to_ne_bytes())
            .collect()
    }
}

fn header(bytes: &[u8]) -> Option<(usize, u32)> {
    let size = u32::from_ne_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    let kind = u32::from_ne_bytes(bytes.get(4..8)?.try_into().ok()?);
    Some((size, kind))
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn padded(size: usize) -> usize {
    size.div_ceil(8) * 8
}

/// How deep PODs may nest before a malformed one is refused.
const MAX_DEPTH: usize = 16;

fn decode_body(kind: u32, body: &[u8], depth: usize) -> Option<Pod> {
    if depth > MAX_DEPTH {
        return None;
    }
    let long = |body: &[u8]| body.get(0..8).and_then(|bytes| bytes.try_into().ok());
    Some(match kind {
        TYPE_NONE => Pod::None,
        TYPE_BOOL => Pod::Bool(word(body, 0)? != 0),
        TYPE_ID => Pod::Id(word(body, 0)?),
        TYPE_INT => Pod::Int(word(body, 0)? as i32),
        TYPE_FLOAT => Pod::Float(f32::from_bits(word(body, 0)?)),
        TYPE_LONG => Pod::Long(i64::from_ne_bytes(long(body)?)),
        TYPE_DOUBLE => Pod::Double(f64::from_ne_bytes(long(body)?)),
        TYPE_STRING => {
            let end = body
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(body.len());
            Pod::String(String::from_utf8_lossy(&body[..end]).into_owned())
        }
        TYPE_ARRAY => {
            let (child_size, child_kind) = header(body)?;
            let mut values = Vec::new();
            if child_size > 0 {
                for element in body.get(8..)?.chunks_exact(child_size) {
                    values.push(decode_body(child_kind, element, depth + 1)?);
                }
            }
            Pod::Array(child_kind, values)
        }
        TYPE_CHOICE => {
            // A choice reads as its default, the first of its values.
            let (child_size, child_kind) = header(body.get(8..)?)?;
            let first = body.get(16..16 + child_size)?;
            decode_body(child_kind, first, depth + 1)?
        }
        TYPE_STRUCT => {
            let mut values = Vec::new();
            let mut at = 0;
            while at + 8 <= body.len() {
                let (size, kind) = header(&body[at..])?;
                values.push(decode_body(
                    kind,
                    body.get(at + 8..at + 8 + size)?,
                    depth + 1,
                )?);
                at += 8 + padded(size);
            }
            Pod::Struct(values)
        }
        TYPE_OBJECT => {
            let object_kind = word(body, 0)?;
            let id = word(body, 4)?;
            let mut properties = Vec::new();
            let mut at = 8;
            while at + 16 <= body.len() {
                let key = word(body, at)?;
                let (size, kind) = header(&body[at + 8..])?;
                let value = body.get(at + 16..at + 16 + size)?;
                properties.push((key, decode_body(kind, value, depth + 1)?));
                at += 16 + padded(size);
            }
            Pod::Object {
                kind: object_kind,
                id,
                properties,
            }
        }
        other => Pod::Other(other),
    })
}

/// Reads a POD the library handed over.
///
/// # Safety
/// `pod` must be null or point at a complete POD.
pub unsafe fn read(pod: *const super::ffi::SpaPod) -> Option<Pod> {
    if pod.is_null() {
        return None;
    }
    // SAFETY: a POD is its header followed by `size` bytes of body.
    let bytes = unsafe {
        let size = (*pod).size as usize;
        std::slice::from_raw_parts(pod.cast::<u8>(), 8 + size)
    };
    Pod::decode(bytes)
}
