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

mod scoring;

use scoring::{
    ASCII_CLASSES, BONUS_BOUNDARY, BONUS_CONSECUTIVE, BONUS_TABLE, Class, FIRST_CHAR_MULTIPLIER,
    GAP_EXTENSION, GAP_START, MAX_CELLS, SCORE_MATCH, class_of, fold,
};

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
#[path = "fuzzy_tests.rs"]
mod tests;
