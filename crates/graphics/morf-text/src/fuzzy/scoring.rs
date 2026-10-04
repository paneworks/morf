//! What a match is worth: character classes, the score and bonus
//! constants, and the folded form characters are compared in.

/// What a character is, for deciding what a match on it is worth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Class {
    White,
    /// Separates the parts of a path or a list: `/ , : ; |`.
    Delimiter,
    /// Punctuation that is not a delimiter: `- _ .` and the like.
    NonWord,
    Lower,
    Upper,
    /// A letter with no case.
    Letter,
    Number,
}

pub(super) fn class_of(character: char) -> Class {
    if character.is_ascii() {
        match character {
            'a'..='z' => Class::Lower,
            'A'..='Z' => Class::Upper,
            '0'..='9' => Class::Number,
            ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c' => Class::White,
            '/' | ',' | ':' | ';' | '|' => Class::Delimiter,
            _ => Class::NonWord,
        }
    } else if character.is_lowercase() {
        Class::Lower
    } else if character.is_uppercase() {
        Class::Upper
    } else if character.is_alphabetic() {
        Class::Letter
    } else if character.is_numeric() {
        Class::Number
    } else if character.is_whitespace() {
        Class::White
    } else {
        Class::NonWord
    }
}

/// [`class_of`] for every ASCII byte, looked up rather than worked out.
pub(super) const ASCII_CLASSES: [Class; 128] = {
    let mut table = [Class::NonWord; 128];
    let mut byte = 0;
    while byte < 128 {
        table[byte] = match byte as u8 {
            b'a'..=b'z' => Class::Lower,
            b'A'..=b'Z' => Class::Upper,
            b'0'..=b'9' => Class::Number,
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => Class::White,
            b'/' | b',' | b':' | b';' | b'|' => Class::Delimiter,
            _ => Class::NonWord,
        };
        byte += 1;
    }
    table
};

const fn is_word(class: Class) -> bool {
    matches!(
        class,
        Class::Lower | Class::Upper | Class::Letter | Class::Number
    )
}

/// What one matched character earns.
pub(super) const SCORE_MATCH: i32 = 16;
/// The first character skipped after a match, and each one after it.
pub(super) const GAP_START: i32 = -3;
pub(super) const GAP_EXTENSION: i32 = -1;
/// A match that starts a word after punctuation.
pub(super) const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2;
/// ... after a space, or at the very start.
pub(super) const BONUS_BOUNDARY_WHITE: i32 = BONUS_BOUNDARY + 2;
/// ... after a path or list separator.
const BONUS_BOUNDARY_DELIMITER: i32 = BONUS_BOUNDARY + 1;
/// A match on punctuation itself.
const BONUS_NON_WORD: i32 = SCORE_MATCH / 2;
/// A camelCase hump, or where digits start after letters.
const BONUS_CAMEL: i32 = BONUS_BOUNDARY + GAP_EXTENSION;
/// The least a character in a run of matches earns: whatever a gap would
/// have cost, so a run is never worse than the same matches spread out.
pub(super) const BONUS_CONSECUTIVE: i32 = -(GAP_START + GAP_EXTENSION);
/// The query's first character counts its bonus this many times.
pub(super) const FIRST_CHAR_MULTIPLIER: i32 = 2;

/// Beyond this many cells (query length × window) the alignment is not
/// run and the greedy match is scored instead.
pub(super) const MAX_CELLS: usize = 1 << 20;

const fn bonus_for(previous: Class, current: Class) -> i32 {
    if is_word(current) {
        match previous {
            Class::White => BONUS_BOUNDARY_WHITE,
            Class::Delimiter => BONUS_BOUNDARY_DELIMITER,
            Class::NonWord => BONUS_BOUNDARY,
            Class::Lower if matches!(current, Class::Upper) => BONUS_CAMEL,
            Class::Lower | Class::Upper | Class::Letter if matches!(current, Class::Number) => {
                BONUS_CAMEL
            }
            _ => 0,
        }
    } else {
        match current {
            Class::White => BONUS_BOUNDARY_WHITE,
            Class::Delimiter | Class::NonWord => BONUS_NON_WORD,
            _ => 0,
        }
    }
}

const CLASSES: [Class; 7] = [
    Class::White,
    Class::Delimiter,
    Class::NonWord,
    Class::Lower,
    Class::Upper,
    Class::Letter,
    Class::Number,
];

/// [`bonus_for`] for every pair, indexed by the classes' discriminants.
pub(super) const BONUS_TABLE: [[i32; 7]; 7] = {
    let mut table = [[0; 7]; 7];
    let mut previous = 0;
    while previous < 7 {
        let mut current = 0;
        while current < 7 {
            table[previous][current] = bonus_for(CLASSES[previous], CLASSES[current]);
            current += 1;
        }
        previous += 1;
    }
    table
};

/// The folded form characters are compared in.
pub(super) fn fold(character: char, case_sensitive: bool) -> char {
    if case_sensitive {
        character
    } else if character.is_ascii() {
        character.to_ascii_lowercase()
    } else {
        character.to_lowercase().next().unwrap_or(character)
    }
}
