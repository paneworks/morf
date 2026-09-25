//! Fuzzy matching: how well a short query picks out a string, and which
//! characters it picked.
//!
//! The scoring follows the shape fzf's second algorithm made familiar (a
//! Smith-Waterman-style alignment over the characters of the text): every
//! matched character earns a fixed amount, a match that starts a word, a
//! camelCase hump or a path component earns a bonus, a run of consecutive
//! matches keeps the bonus its first character earned, and every character
//! skipped between two matches costs a little — the first more than the
//! rest. The query's first character counts its bonus twice, so a match at
//! the start of a word, and most of all at the start of the text, wins.
//! This is an original implementation of that idea; no code is taken from
//! fzf.
//!
//! Matching is case-insensitive unless the query holds an uppercase letter
//! ("smart case"). It works on characters, not bytes, and reports positions
//! as byte offsets into the text, which is what a text span wants.
//!
//! A query with spaces is several terms, each of which must match; the
//! scores add up and the positions merge.

/// What a character is, for deciding what a match on it is worth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Class {
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

fn class_of(character: char) -> Class {
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
const ASCII_CLASSES: [Class; 128] = {
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
const SCORE_MATCH: i32 = 16;
/// The first character skipped after a match, and each one after it.
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
/// A match that starts a word after punctuation.
const BONUS_BOUNDARY: i32 = SCORE_MATCH / 2;
/// ... after a space, or at the very start.
const BONUS_BOUNDARY_WHITE: i32 = BONUS_BOUNDARY + 2;
/// ... after a path or list separator.
const BONUS_BOUNDARY_DELIMITER: i32 = BONUS_BOUNDARY + 1;
/// A match on punctuation itself.
const BONUS_NON_WORD: i32 = SCORE_MATCH / 2;
/// A camelCase hump, or where digits start after letters.
const BONUS_CAMEL: i32 = BONUS_BOUNDARY + GAP_EXTENSION;
/// The least a character in a run of matches earns: whatever a gap would
/// have cost, so a run is never worse than the same matches spread out.
const BONUS_CONSECUTIVE: i32 = -(GAP_START + GAP_EXTENSION);
/// The query's first character counts its bonus this many times.
const FIRST_CHAR_MULTIPLIER: i32 = 2;

/// Beyond this many cells (query length × window) the alignment is not
/// run and the greedy match is scored instead.
const MAX_CELLS: usize = 1 << 20;

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
const BONUS_TABLE: [[i32; 7]; 7] = {
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
fn fold(character: char, case_sensitive: bool) -> char {
    if case_sensitive {
        character
    } else if character.is_ascii() {
        character.to_ascii_lowercase()
    } else {
        character.to_lowercase().next().unwrap_or(character)
    }
}

/// One match: its score, and the byte offsets of the characters matched,
/// ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    pub positions: Vec<usize>,
}

/// A query, prepared once and matched against many texts. Keeps its
/// scratch space between texts, so matching a list allocates nothing per
/// item after the first few.
pub struct FuzzyMatcher {
    terms: Vec<Vec<char>>,
    case_sensitive: bool,
    // Scratch, per text.
    text: Vec<char>,
    offsets: Vec<usize>,
    /// The loaded text is ASCII: offsets are indices, and not stored.
    ascii: bool,
    bonus: Vec<i32>,
    first: Vec<usize>,
    score: Vec<i32>,
    run: Vec<u16>,
    took: Vec<bool>,
}

/// Stands for a cell no alignment can reach.
const UNREACHABLE: i32 = i32::MIN / 2;

impl FuzzyMatcher {
    /// Prepares `query`. Smart case: case-sensitive only when the query has
    /// an uppercase letter.
    pub fn new(query: &str) -> Self {
        let case_sensitive = query.chars().any(char::is_uppercase);
        let terms = query
            .split_whitespace()
            .map(|term| term.chars().map(|c| fold(c, case_sensitive)).collect())
            .collect();
        Self {
            terms,
            case_sensitive,
            text: Vec::new(),
            offsets: Vec::new(),
            ascii: true,
            bonus: Vec::new(),
            first: Vec::new(),
            score: Vec::new(),
            run: Vec::new(),
            took: Vec::new(),
        }
    }

    /// True when the query has no terms: everything matches, with score 0.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// The score alone, or `None` when the text does not match.
    pub fn score(&mut self, text: &str) -> Option<i32> {
        self.run_all(text, false).map(|found| found.score)
    }

    /// The score and the byte offsets of the characters matched.
    pub fn find(&mut self, text: &str) -> Option<FuzzyMatch> {
        self.run_all(text, true)
    }

    fn run_all(&mut self, text: &str, positions: bool) -> Option<FuzzyMatch> {
        if self.terms.is_empty() {
            return Some(FuzzyMatch::default());
        }
        // A cheap refusal first: each term's characters in order, somewhere.
        if !self
            .terms
            .iter()
            .all(|term| subsequence(term, text, self.case_sensitive))
        {
            return None;
        }
        self.load(text);
        let mut total = FuzzyMatch::default();
        for index in 0..self.terms.len() {
            let term = std::mem::take(&mut self.terms[index]);
            let found = self.align(&term, positions);
            self.terms[index] = term;
            let found = found?;
            total.score += found.score;
            total.positions.extend(found.positions);
        }
        if self.terms.len() > 1 {
            total.positions.sort_unstable();
            total.positions.dedup();
        }
        Some(total)
    }

    /// Folds the text into characters, their byte offsets and the bonus a
    /// match on each would earn.
    fn load(&mut self, text: &str) {
        self.text.clear();
        self.offsets.clear();
        self.bonus.clear();
        let mut previous = Class::White;
        self.ascii = text.is_ascii();
        if self.ascii {
            let bytes = text.as_bytes();
            let case_sensitive = self.case_sensitive;
            self.text.extend(bytes.iter().map(|&byte| {
                char::from(if case_sensitive {
                    byte
                } else {
                    byte.to_ascii_lowercase()
                })
            }));
            self.bonus.extend(bytes.iter().map(|&byte| {
                let class = ASCII_CLASSES[usize::from(byte)];
                let bonus = BONUS_TABLE[previous as usize][class as usize];
                previous = class;
                bonus
            }));
            return;
        }
        for (offset, character) in text.char_indices() {
            let class = class_of(character);
            self.text.push(fold(character, self.case_sensitive));
            self.offsets.push(offset);
            self.bonus
                .push(BONUS_TABLE[previous as usize][class as usize]);
            previous = class;
        }
    }

    /// The byte offset of a loaded character.
    fn offset(&self, index: usize) -> usize {
        if self.ascii {
            index
        } else {
            self.offsets[index]
        }
    }

    /// The best alignment of one term against the loaded text.
    fn align(&mut self, term: &[char], positions: bool) -> Option<FuzzyMatch> {
        let m = term.len();
        if m == 0 {
            return Some(FuzzyMatch::default());
        }
        // The earliest place each query character can match, greedily: no
        // alignment puts character i before first[i].
        self.first.clear();
        let mut at = 0;
        for &wanted in term {
            let found = self.text[at..].iter().position(|&c| c == wanted)? + at;
            self.first.push(found);
            at = found + 1;
        }
        // And the latest place the last one can.
        let last = self.text.iter().rposition(|&c| c == term[m - 1])?;
        let start = self.first[0];
        let width = last + 1 - start;
        if m * width > MAX_CELLS {
            return Some(self.greedy(term));
        }
        self.score.clear();
        self.score.resize(m * width, UNREACHABLE);
        self.run.clear();
        self.run.resize(m * width, 0);
        if positions {
            self.took.clear();
            self.took.resize(m * width, false);
        }
        let text = &self.text[start..=last];
        let bonuses = &self.bonus[start..=last];
        for (row, &wanted) in term.iter().enumerate() {
            let from = self.first[row] - start;
            let (above, rest) = self.score.split_at_mut(row * width);
            let current = &mut rest[..width];
            let (runs_above, runs_rest) = self.run.split_at_mut(row * width);
            let runs = &mut runs_rest[..width];
            let above = if row > 0 {
                &above[(row - 1) * width..]
            } else {
                &above[..0]
            };
            let runs_above = if row > 0 {
                &runs_above[(row - 1) * width..]
            } else {
                &runs_above[..0]
            };
            let mut took_row = if positions {
                Some(&mut self.took[row * width..(row + 1) * width])
            } else {
                None
            };
            // Every cell from `from` on is reachable: `from` is a match,
            // and each later cell at worst extends a gap. So is every
            // diagonal: `from` is past the previous row's first match.
            // Unreachable is far enough below zero that adding a gap to it
            // stays unreachable without a check.
            let mut gap_cost = GAP_START;
            let mut left = UNREACHABLE;
            for column in from..width {
                let gap = left + gap_cost;
                let mut value = gap;
                let mut run = 0u16;
                if text[column] == wanted {
                    let mut bonus = bonuses[column];
                    let matched = if row == 0 {
                        run = 1;
                        SCORE_MATCH + bonus * FIRST_CHAR_MULTIPLIER
                    } else {
                        let previous_run = runs_above[column - 1];
                        run = previous_run.saturating_add(1);
                        if previous_run > 0 {
                            // A run keeps what its first character earned,
                            // unless this one starts something better,
                            // which begins a new run.
                            let head = bonuses[column + 1 - usize::from(run)];
                            if bonus >= BONUS_BOUNDARY && bonus > head {
                                run = 1;
                            } else {
                                bonus = bonus.max(head).max(BONUS_CONSECUTIVE);
                            }
                        }
                        above[column - 1] + SCORE_MATCH + bonus
                    };
                    if matched >= gap {
                        value = matched;
                    } else {
                        run = 0;
                    }
                }
                current[column] = value;
                runs[column] = run;
                if let Some(took) = took_row.as_deref_mut() {
                    took[column] = run > 0;
                }
                gap_cost = if run > 0 { GAP_START } else { GAP_EXTENSION };
                left = value;
            }
        }
        // The best cell of the last row.
        let row = m - 1;
        let (best_column, best) = (self.first[row]..=last)
            .map(|column| (column, self.score[row * width + column - start]))
            .fold((last, UNREACHABLE), |kept, next| {
                if next.1 > kept.1 { next } else { kept }
            });
        if best == UNREACHABLE {
            return None;
        }
        let mut found = FuzzyMatch {
            score: best,
            positions: Vec::new(),
        };
        if positions {
            found.positions.resize(m, 0);
            let (mut row, mut column) = (m as isize - 1, best_column);
            while row >= 0 {
                let cell = row as usize * width + column - start;
                if self.took[cell] {
                    found.positions[row as usize] = self.offset(column);
                    row -= 1;
                }
                if row < 0 || column == start {
                    break;
                }
                column -= 1;
            }
        }
        Some(found)
    }

    /// For a text too long to align: the leftmost match, pulled as tight as
    /// it goes from the right, and scored the same way.
    fn greedy(&self, term: &[char]) -> FuzzyMatch {
        let mut picks = vec![0usize; term.len()];
        // Walk back from the forward match's end for the tightest window.
        let mut want = term.len();
        let mut column = self.first[term.len() - 1] + 1;
        while want > 0 && column > 0 {
            column -= 1;
            if self.text[column] == term[want - 1] {
                want -= 1;
                picks[want] = column;
            }
        }
        let mut score = 0;
        let mut run_head_bonus = 0;
        let mut previous: Option<usize> = None;
        for (index, &column) in picks.iter().enumerate() {
            let mut bonus = self.bonus[column];
            match previous {
                Some(prior) if prior + 1 == column => {
                    bonus = bonus.max(run_head_bonus).max(BONUS_CONSECUTIVE);
                }
                Some(prior) => {
                    let gap = (column - prior - 1) as i32;
                    score += GAP_START + GAP_EXTENSION * (gap - 1);
                    run_head_bonus = bonus;
                }
                None => run_head_bonus = bonus,
            }
            score += SCORE_MATCH
                + if index == 0 {
                    bonus * FIRST_CHAR_MULTIPLIER
                } else {
                    bonus
                };
            previous = Some(column);
        }
        FuzzyMatch {
            score,
            positions: picks.iter().map(|&column| self.offset(column)).collect(),
        }
    }
}

/// Whether `term` (already folded) appears in order in `text`.
fn subsequence(term: &[char], text: &str, case_sensitive: bool) -> bool {
    if text.is_ascii() {
        let mut bytes = text.as_bytes().iter();
        return term.iter().all(|&wanted| {
            if !wanted.is_ascii() {
                return false;
            }
            let wanted = wanted as u8;
            bytes.any(|&byte| {
                let byte = if case_sensitive {
                    byte
                } else {
                    byte.to_ascii_lowercase()
                };
                byte == wanted
            })
        });
    }
    let mut wanted = term.iter();
    let Some(mut next) = wanted.next() else {
        return true;
    };
    for character in text.chars() {
        if fold(character, case_sensitive) == *next {
            match wanted.next() {
                Some(following) => next = following,
                None => return true,
            }
        }
    }
    false
}

/// One-off: the score and matched byte offsets of `query` in `text`.
pub fn fuzzy_match(query: &str, text: &str) -> Option<FuzzyMatch> {
    FuzzyMatcher::new(query).find(text)
}

/// One entry of a ranked list: which item, its score, and what matched.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    /// Index into the candidates handed in.
    pub index: usize,
    pub score: f64,
    /// Which of an item's texts the match was in.
    pub key: usize,
    pub positions: Vec<usize>,
}

/// Ranks candidates by how well `query` matches them, best first, keeping
/// at most `limit`. Each candidate is one or more texts with a weight each
/// (a launcher entry's name and its description, say); a candidate scores
/// its best weighted text. Ties go to the shorter text, then to the
/// earlier candidate. An empty query keeps every candidate in order.
pub fn rank<S: AsRef<str>>(query: &str, candidates: &[Vec<(S, f64)>], limit: usize) -> Vec<Ranked> {
    let mut matcher = FuzzyMatcher::new(query);
    // Scores first, with no positions: most candidates never need them.
    // Each entry: (score, length of the text, candidate, key).
    let mut scored: Vec<(f64, usize, usize, usize)> = Vec::new();
    for (index, texts) in candidates.iter().enumerate() {
        let mut best: Option<(f64, usize, usize)> = None;
        for (key, (text, weight)) in texts.iter().enumerate() {
            let text = text.as_ref();
            if let Some(score) = matcher.score(text) {
                let weighted = f64::from(score) * weight;
                if best.is_none_or(|(kept, ..)| weighted > kept) {
                    best = Some((weighted, key, text.len()));
                }
            }
        }
        if let Some((score, key, length)) = best {
            scored.push((score, length, index, key));
        }
    }
    type Entry = (f64, usize, usize, usize);
    let order = |a: &Entry, b: &Entry| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2));
    let limit = limit.min(scored.len());
    if matcher.is_empty() {
        // Already in order.
    } else if limit > 0 && limit < scored.len() {
        scored.select_nth_unstable_by(limit - 1, order);
        scored.truncate(limit);
        scored.sort_by(order);
    } else {
        scored.sort_by(order);
    }
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|(score, _, index, key)| Ranked {
            index,
            score,
            key,
            positions: matcher
                .find(candidates[index][key].0.as_ref())
                .map(|found| found.positions)
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(query: &str, text: &str) -> i32 {
        fuzzy_match(query, text)
            .unwrap_or_else(|| panic!("{query} should match {text}"))
            .score
    }

    fn positions(query: &str, text: &str) -> Vec<usize> {
        fuzzy_match(query, text).unwrap().positions
    }

    #[test]
    fn a_query_matches_its_characters_in_order_only() {
        assert!(fuzzy_match("abc", "a-b-c").is_some());
        assert!(fuzzy_match("abc", "acb").is_none());
        assert!(fuzzy_match("abcd", "abc").is_none());
        assert!(fuzzy_match("", "anything").is_some());
        assert!(fuzzy_match("x", "").is_none());
    }

    #[test]
    fn smart_case() {
        assert!(fuzzy_match("ff", "FireFox").is_some());
        assert!(fuzzy_match("FF", "FireFox").is_some());
        assert!(fuzzy_match("FF", "firefox").is_none());
        assert!(fuzzy_match("Ff", "FireFox").is_none());
    }

    #[test]
    fn exact_scores_follow_the_rules() {
        // "f" at the very start: a match, and the start-of-text bonus twice.
        assert_eq!(score("f", "foo"), SCORE_MATCH + BONUS_BOUNDARY_WHITE * 2);
        // A run keeps its head's bonus.
        assert_eq!(
            score("fo", "foo"),
            2 * SCORE_MATCH + BONUS_BOUNDARY_WHITE * 3
        );
        // A gap of two costs a start and one extension.
        assert_eq!(
            score("fb", "f__b"),
            2 * SCORE_MATCH + BONUS_BOUNDARY_WHITE * 2 + BONUS_BOUNDARY + GAP_START + GAP_EXTENSION
        );
    }

    #[test]
    fn word_starts_beat_the_middle_of_words() {
        assert!(score("gc", "git commit") > score("gc", "logic"));
        assert!(score("fb", "FooBar") > score("fb", "afoobar"));
        assert!(score("sp", "src/path") > score("sp", "sharp"));
        // A prefix beats the same run later on.
        assert!(score("fire", "firefox") > score("fire", "campfire"));
        // Consecutive beats scattered.
        assert!(score("abc", "xabcx") > score("abc", "xaxbxcx"));
    }

    #[test]
    fn the_best_alignment_is_found_not_the_first() {
        // Greedy would take the first `a`; the word start later is better.
        assert_eq!(positions("ab", "xa-ab"), vec![3, 4]);
        assert_eq!(positions("tst", "the_test"), vec![4, 6, 7]);
    }

    #[test]
    fn positions_are_byte_offsets_on_char_boundaries() {
        let text = "Ünïcödé naïve";
        let found = fuzzy_match("üna", text).unwrap();
        for position in &found.positions {
            assert!(text.is_char_boundary(*position));
        }
        assert_eq!(found.positions[0], 0);
        assert_eq!(&text[found.positions[1]..found.positions[1] + 1], "n");
        // Case folds beyond ASCII.
        assert!(fuzzy_match("ÉCOLE", "école").is_none());
        assert!(fuzzy_match("éco", "ÉCOLE").is_some());
        // Wide characters.
        assert_eq!(positions("日本", "日本語"), vec![0, 3]);
    }

    #[test]
    fn terms_all_match_and_their_positions_merge() {
        // "fox" takes the word-start `f` (worth more than the run "fox"
        // costs), and "fire" overlaps it: one position each.
        let found = fuzzy_match("fox fire", "firefox").unwrap();
        assert_eq!(found.positions, vec![0, 1, 2, 3, 5, 6]);
        assert_eq!(
            found.score,
            score("fox", "firefox") + score("fire", "firefox")
        );
        assert!(fuzzy_match("fox dog", "firefox").is_none());
    }

    #[test]
    fn very_long_texts_fall_back_to_the_greedy_match() {
        let text = format!("{}needle", "x".repeat(MAX_CELLS));
        let found = fuzzy_match("needle", &text).unwrap();
        assert_eq!(found.positions.len(), 6);
        assert_eq!(found.positions[0], MAX_CELLS);
    }

    #[test]
    fn rank_orders_limits_and_weighs() {
        let items = ["logic", "git commit", "gc", "nothing", "g_c"];
        let single: Vec<Vec<(&str, f64)>> = items.iter().map(|text| vec![(*text, 1.0)]).collect();
        let ranked = rank("gc", &single, 3);
        let order: Vec<usize> = ranked.iter().map(|entry| entry.index).collect();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], 2);
        assert!(!order.contains(&3));
        // A weight lifts a weaker text over a stronger one.
        let ranked = rank(
            "ab",
            &[vec![("xaxb", 1.0)], vec![("ab", 0.1), ("xaxb", 3.0)]],
            10,
        );
        assert_eq!(ranked[0].index, 1);
        assert_eq!(ranked[0].key, 1);
        // An empty query keeps every candidate, in order.
        let ranked = rank("", &single, 10);
        assert_eq!(ranked.len(), 5);
        assert_eq!(ranked[4].index, 4);
    }

    #[test]
    fn ten_thousand_items_rank_within_a_frame() {
        let words = [
            "firefox",
            "terminal",
            "settings",
            "files",
            "calculator",
            "text editor",
            "system monitor",
            "image viewer",
            "disk usage",
            "Network Manager",
        ];
        let items: Vec<String> = (0..10_000)
            .map(|index| {
                format!(
                    "{} {} /usr/share/applications/org.example.{index}.desktop",
                    words[index % words.len()],
                    words[(index * 7 + 3) % words.len()]
                )
            })
            .collect();
        let items: Vec<Vec<(&str, f64)>> = items
            .iter()
            .map(|text| vec![(text.as_str(), 1.0)])
            .collect();
        let start = std::time::Instant::now();
        let rounds = 5;
        for query in ["f", "fi", "fir", "sm", "orgex", "nm"]
            .iter()
            .cycle()
            .take(rounds * 6)
        {
            let ranked = rank(query, &items, 50);
            assert!(!ranked.is_empty());
        }
        let per_query = start.elapsed() / (rounds as u32 * 6);
        eprintln!("fuzzy: 10k items, {per_query:?} per query");
        // Generous for a debug build on a loaded machine; a release build is
        // well under a millisecond or two.
        assert!(
            per_query
                < std::time::Duration::from_millis(if cfg!(debug_assertions) { 400 } else { 16 })
        );
    }
}
