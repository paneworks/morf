//! Tests of CAM16, tone and the solver against Material Color Utilities'
//! published values.

use super::solver::solver;
use super::*;

fn srgb(hex: u32) -> [f64; 3] {
    [16, 8, 0].map(|shift| f64::from((hex >> shift) & 0xff) / 255.0)
}

fn hex(rgb: [f64; 3]) -> u32 {
    rgb.iter().fold(0, |acc, channel| {
        (acc << 8) | (channel * 255.0).round() as u32
    })
}

fn cam(hex: u32) -> Cam16 {
    Cam16::from_linrgb(srgb(hex).map(linearized), ViewingConditions::standard())
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} is not {expected} ± {tolerance}"
    );
}

#[test]
fn cam16_of_the_primaries_matches_the_reference() {
    // Published in Material Color Utilities' cam16 tests.
    let red = cam(0xff0000);
    close(red.hue, 27.408, 0.001, "red hue");
    close(red.chroma, 113.357, 0.001, "red chroma");
    close(red.j, 46.445, 0.001, "red J");
    close(red.m, 89.494, 0.001, "red M");
    close(red.s, 91.889, 0.001, "red s");
    close(red.q, 105.988, 0.001, "red Q");
    let green = cam(0x00ff00);
    close(green.hue, 142.139, 0.001, "green hue");
    close(green.chroma, 108.410, 0.001, "green chroma");
    close(green.j, 79.331, 0.001, "green J");
    close(green.m, 85.587, 0.001, "green M");
    close(green.s, 78.604, 0.001, "green s");
    close(green.q, 138.520, 0.001, "green Q");
    let blue = cam(0x0000ff);
    close(blue.hue, 282.788, 0.001, "blue hue");
    close(blue.chroma, 87.230, 0.001, "blue chroma");
    close(blue.j, 25.465, 0.001, "blue J");
    close(blue.m, 68.867, 0.001, "blue M");
    close(blue.s, 93.674, 0.001, "blue s");
    close(blue.q, 78.481, 0.001, "blue Q");
    let white = cam(0xffffff);
    close(white.hue, 209.492, 0.001, "white hue");
    close(white.chroma, 2.869, 0.001, "white chroma");
    close(white.j, 100.0, 0.001, "white J");
    let black = cam(0x000000);
    close(black.chroma, 0.0, 0.001, "black chroma");
    close(black.j, 0.0, 0.001, "black J");
}

#[test]
fn tone_is_lstar() {
    close(hct_from_srgb(srgb(0xff0000))[2], 53.233, 0.01, "red tone");
    close(hct_from_srgb(srgb(0x00ff00))[2], 87.737, 0.01, "green tone");
    close(hct_from_srgb(srgb(0x0000ff))[2], 32.303, 0.01, "blue tone");
    close(hct_from_srgb(srgb(0x808080))[2], 53.585, 0.01, "grey tone");
}

#[test]
fn derived_solver_matrices_match_the_published_ones() {
    // The constants Material Color Utilities' HctSolver ships with.
    let published = [
        [
            0.001200833568784504,
            0.002389694492170889,
            0.0002795742885861124,
        ],
        [
            0.0005891086651375999,
            0.0029785502573438758,
            0.0003270666104008398,
        ],
        [
            0.00010146692491640572,
            0.0005364214359186694,
            0.0032979401770712076,
        ],
    ];
    let inverse = [
        [1373.2198709594231, -1100.4251190754821, -7.278681089101213],
        [-271.815969077903, 559.6580465940733, -32.46047482791194],
        [1.9622899599665666, -57.173814538844006, 308.7233197812385],
    ];
    let solver = solver();
    for row in 0..3 {
        for column in 0..3 {
            let ours = solver.scaled_discount_from_linrgb[row][column];
            let theirs = published[row][column];
            assert!(
                ((ours - theirs) / theirs).abs() < 1e-6,
                "[{row}][{column}] {ours} vs {theirs}"
            );
            let ours = solver.linrgb_from_scaled_discount[row][column];
            let theirs = inverse[row][column];
            assert!(
                ((ours - theirs) / theirs).abs() < 1e-6,
                "inverse [{row}][{column}] {ours} vs {theirs}"
            );
        }
    }
    close(
        solver.critical_planes[0],
        0.015176349177441876,
        1e-12,
        "first plane",
    );
}

#[test]
fn every_srgb_colour_round_trips() {
    // Through HCT and back to the same 8-bit colour, across the cube.
    for r in (0..=255).step_by(15) {
        for g in (0..=255).step_by(15) {
            for b in (0..=255).step_by(15) {
                let colour = (r << 16) | (g << 8) | b;
                let [h, c, t] = hct_from_srgb(srgb(colour));
                let back = hex(solve(h, c, t));
                assert_eq!(
                    back, colour,
                    "{colour:06x} via hct {h} {c} {t} came back {back:06x}"
                );
            }
        }
    }
}

#[test]
fn asking_for_too_much_chroma_keeps_hue_and_tone() {
    // Material Color Utilities' own check: over hues, chromas and
    // tones, the colour made has the tone asked for, the hue asked for
    // (when it has any chroma to show it), and no more chroma.
    for hue in (15..360).step_by(30) {
        for chroma in (0..=100).step_by(10) {
            for tone in (20..=80).step_by(10) {
                let (hue, chroma, tone) = (f64::from(hue), f64::from(chroma), f64::from(tone));
                let [h, c, t] = hct_from_srgb(solve(hue, chroma, tone));
                if chroma > 0.0 {
                    let difference = (h - hue).abs().min(360.0 - (h - hue).abs());
                    assert!(difference <= 4.0, "hue {hue} {chroma} {tone} came back {h}");
                }
                assert!(
                    c <= chroma + 2.5,
                    "chroma {hue} {chroma} {tone} came back {c}"
                );
                close(t, tone, 0.5, "tone");
            }
        }
    }
}

#[test]
fn tonal_palette_of_blue_matches_the_reference() {
    // Material Color Utilities' palettes test: the tones of #0000ff.
    let [hue, chroma, _] = hct_from_srgb(srgb(0x0000ff));
    let expected = [
        (100, 0xffffff),
        (95, 0xf1efff),
        (90, 0xe0e0ff),
        (80, 0xbec2ff),
        (70, 0x9da3ff),
        (60, 0x7c84ff),
        (50, 0x5a64ff),
        (40, 0x343dff),
        (30, 0x0000ef),
        (20, 0x0001ac),
        (10, 0x00006e),
        (0, 0x000000),
    ];
    for (tone, colour) in expected {
        let made = hex(solve(hue, chroma, f64::from(tone)));
        assert_eq!(made, colour, "tone {tone}: {made:06x} not {colour:06x}");
    }
}
