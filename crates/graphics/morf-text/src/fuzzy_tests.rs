//! Tests of fuzzy matching: scores, positions, terms and ranking.

use super::scoring::BONUS_BOUNDARY_WHITE;
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
        per_query < std::time::Duration::from_millis(if cfg!(debug_assertions) { 400 } else { 16 })
    );
}
