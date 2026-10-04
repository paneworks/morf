//! What a shader's uniform block looks like to both sides: the compiler
//! (morf-shader, which packs a shader's parameters after it) and the
//! renderer (morf-render, which writes it). Here so neither depends on the
//! other.

/// The bytes before a shader's own parameters in its uniform block:
/// `resolution` then `time`, padded to sixteen. Fixed rather than packed with
/// the rest so a host writing the clock does not have to know what a particular
/// shader declared.
pub const HEADER_BYTES: u32 = 16;
