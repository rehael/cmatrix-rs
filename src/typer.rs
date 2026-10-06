//! Typewriter overlay for `-F`: lines are typed one at a time into a box over the rain.
//!
//! Sequence: once the rain has reached the middle of the screen in half the lanes, the
//! box appears and the cursor blinks for 5 s. Each line is typed at 50 ms per character
//! (the cursor cell scrambles through rain glyphs at random lightness, then the
//! character settles), then fades out over 5 s. The cursor blinks for 1 s before each following line. After the
//! last line the box goes away for 10 s, then the sequence starts again.

use crate::rng::Rng;

const FIRST_BLINK: f32 = 5.0;
const NEXT_BLINK: f32 = 1.0;
const CHAR_TIME: f32 = 0.05;
const FADE: f32 = 5.0;
const REST: f32 = 10.0;
/// Cursor blink half-period.
const BLINK: f32 = 0.3;
/// Lightness range of the scramble glyphs, as a fraction of the rain colour.
const SCRAMBLE_LIGHTNESS: (f32, f32) = (0.25, 1.0);
/// Cells around the text: 2 rain and 2 padding on each side, plus the cursor after the last character.
const MARGIN: usize = 9;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Wait,
    FirstBlink,
    Type,
    Fade,
    NextBlink,
    Rest,
}

/// What the overlay shows in one frame.
#[derive(Debug, PartialEq)]
pub struct TextFrame<'a> {
    pub text: &'a [char],
    /// Characters already typed.
    pub shown: usize,
    /// Glyph and its lightness (0 to 1) in the cursor cell while a character is being typed.
    pub scramble: Option<(char, f32)>,
    pub cursor: bool,
    /// Text brightness: 1 while typing, falling to 0 during the fade.
    pub level: f32,
}

pub struct Typer {
    lines: Vec<Vec<char>>,
    /// Lines wrapped to the current width, each with the index of its source line.
    pieces: Vec<(usize, Vec<char>)>,
    width: usize,
    piece: usize,
    /// Source line of the current piece; survives re-wrapping.
    line: usize,
    phase: Phase,
    t: f32,
    scramble: (char, f32),
    glyphs: Vec<char>,
    rng: Rng,
}

impl Typer {
    /// `None` when `text` has nothing to type. `glyphs` must not be empty.
    pub fn new(text: &str, glyphs: Vec<char>, rng: Rng) -> Option<Self> {
        let lines = parse(text);
        if lines.is_empty() {
            return None;
        }
        Some(Self {
            lines,
            pieces: Vec::new(),
            width: 0,
            piece: 0,
            line: 0,
            phase: Phase::Wait,
            t: 0.0,
            scramble: (glyphs[0], 1.0),
            glyphs,
            rng,
        })
    }

    /// Re-wraps for a new screen width; a line being shown starts over.
    pub fn set_screen_width(&mut self, screen_width: usize) {
        let width = screen_width.saturating_sub(MARGIN);
        if width == self.width {
            return;
        }
        self.width = width;
        self.pieces = self
            .lines
            .iter()
            .enumerate()
            .flat_map(|(i, line)| wrap(line, width).into_iter().map(move |p| (i, p)))
            .collect();
        self.piece = self
            .pieces
            .iter()
            .position(|p| p.0 == self.line)
            .unwrap_or(0);
        if matches!(self.phase, Phase::Type | Phase::Fade | Phase::NextBlink) {
            self.phase = Phase::NextBlink;
            self.t = 0.0;
        }
    }

    /// True until the rain is ready for the first box.
    pub fn waiting(&self) -> bool {
        self.phase == Phase::Wait
    }

    /// Advances by `dt` seconds. Nothing moves while the text does not fit.
    pub fn update(&mut self, dt: f32, rain_ready: bool) {
        if self.pieces.is_empty() {
            return;
        }
        if self.phase == Phase::Wait {
            if rain_ready {
                self.set_phase(Phase::FirstBlink, 0);
            }
            return;
        }
        self.t += dt;
        while let Some(d) = self.duration().filter(|&d| self.t >= d) {
            self.t -= d;
            self.advance();
        }
        if self.phase == Phase::Type {
            let (lo, hi) = SCRAMBLE_LIGHTNESS;
            self.scramble = (self.rng.pick(&self.glyphs), self.rng.range(lo, hi));
        }
    }

    pub fn frame(&self) -> Option<TextFrame<'_>> {
        let text = &self.pieces.get(self.piece)?.1;
        let blink_on = ((self.t / BLINK) as u32).is_multiple_of(2);
        let (shown, scramble, cursor, level) = match self.phase {
            Phase::Wait | Phase::Rest => return None,
            Phase::FirstBlink | Phase::NextBlink => (0, None, blink_on, 1.0),
            Phase::Type => {
                let shown = ((self.t / CHAR_TIME) as usize).min(text.len());
                let scramble = (shown < text.len()).then_some(self.scramble);
                (shown, scramble, true, 1.0)
            }
            Phase::Fade => (text.len(), None, false, 1.0 - self.t / FADE),
        };
        Some(TextFrame {
            text,
            shown,
            scramble,
            cursor,
            level,
        })
    }

    fn duration(&self) -> Option<f32> {
        Some(match self.phase {
            Phase::Wait => return None,
            Phase::FirstBlink => FIRST_BLINK,
            Phase::Type => self.pieces[self.piece].1.len() as f32 * CHAR_TIME,
            Phase::Fade => FADE,
            Phase::NextBlink => NEXT_BLINK,
            Phase::Rest => REST,
        })
    }

    fn advance(&mut self) {
        match self.phase {
            Phase::FirstBlink | Phase::NextBlink => self.phase = Phase::Type,
            Phase::Type => self.phase = Phase::Fade,
            Phase::Fade if self.piece + 1 < self.pieces.len() => {
                self.set_phase(Phase::NextBlink, self.piece + 1)
            }
            Phase::Fade => self.phase = Phase::Rest,
            Phase::Rest | Phase::Wait => self.set_phase(Phase::FirstBlink, 0),
        }
    }

    fn set_phase(&mut self, phase: Phase, piece: usize) {
        self.phase = phase;
        self.piece = piece;
        self.line = self.pieces[piece].0;
    }
}

/// Terminal cells taken by `c`: 0 for combining and zero-width marks, 2 for East Asian
/// wide characters and common emoji, 1 otherwise. An approximation of Unicode East
/// Asian Width that covers the usual cases without a dependency.
pub fn char_width(c: char) -> usize {
    match c as u32 {
        0x0300..=0x036F | 0x200B..=0x200F | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFEFF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// Lines to type. Tabs become 4 spaces; control and zero-width characters are dropped;
/// trailing spaces and blank lines are removed.
fn parse(text: &str) -> Vec<Vec<char>> {
    text.lines()
        .map(|line| {
            let mut out = Vec::new();
            for c in line.chars() {
                match c {
                    '\t' => out.extend([' '; 4]),
                    c if c.is_control() || char_width(c) == 0 => {}
                    c => out.push(c),
                }
            }
            while out.last() == Some(&' ') {
                out.pop();
            }
            out
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// Splits `line` into pieces of at most `width` cells, breaking at spaces where possible.
/// A wide character always gets its own piece, even when `width` is 1.
fn wrap(line: &[char], width: usize) -> Vec<Vec<char>> {
    let mut pieces = Vec::new();
    if width == 0 {
        return pieces;
    }
    let mut rest = line;
    while !rest.is_empty() {
        let mut cells = 0;
        let fit = rest
            .iter()
            .take_while(|&&c| {
                cells += char_width(c);
                cells <= width
            })
            .count()
            .max(1);
        if fit >= rest.len() {
            pieces.push(rest.to_vec());
            break;
        }
        let cut = rest[..=fit]
            .iter()
            .rposition(|&c| c == ' ')
            .filter(|&i| i > 0)
            .unwrap_or(fit);
        let piece = trim_end(&rest[..cut]);
        if !piece.is_empty() {
            pieces.push(piece.to_vec());
        }
        rest = trim_start(&rest[cut..]);
    }
    pieces
}

fn trim_start(s: &[char]) -> &[char] {
    &s[s.iter().position(|&c| c != ' ').unwrap_or(s.len())..]
}

fn trim_end(s: &[char]) -> &[char] {
    &s[..s.iter().rposition(|&c| c != ' ').map_or(0, |i| i + 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rain::movie_glyphs;

    fn s(pieces: &[Vec<char>]) -> Vec<String> {
        pieces.iter().map(|p| p.iter().collect()).collect()
    }

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn typer(text: &str) -> Typer {
        let mut t = Typer::new(text, movie_glyphs(), Rng::new(5)).unwrap();
        t.set_screen_width(40);
        t
    }

    fn text(frame: &TextFrame) -> String {
        frame.text.iter().collect()
    }

    #[test]
    fn parse_cleans_lines() {
        let lines = parse("\u{FEFF}Wake up,\tNeo  \r\n\n   \nZaz\u{0307}o\u{1}\u{200B}lc\n");
        let lines: Vec<String> = lines.iter().map(|l| l.iter().collect()).collect();
        assert_eq!(lines, ["Wake up,    Neo", "Zazolc"]);
        assert!(Typer::new("\n \t\n", movie_glyphs(), Rng::new(1)).is_none());
    }

    #[test]
    fn wrap_breaks_at_spaces_then_hard() {
        let line = chars("hello world foo");
        assert_eq!(s(&wrap(&line, 15)), ["hello world foo"]);
        assert_eq!(s(&wrap(&line, 11)), ["hello world", "foo"]);
        assert_eq!(s(&wrap(&line, 5)), ["hello", "world", "foo"]);
        assert_eq!(s(&wrap(&line, 7)), ["hello", "world", "foo"]);
        assert_eq!(s(&wrap(&chars("abcdefgh"), 3)), ["abc", "def", "gh"]);
        assert_eq!(
            s(&wrap(&chars("  indented text"), 10)),
            ["  indented", "text"]
        );
        assert!(wrap(&line, 0).is_empty());
    }

    #[test]
    fn wrap_counts_wide_characters_as_two_cells() {
        assert_eq!(char_width('漢'), 2);
        assert_eq!(char_width('ą'), 1);
        assert_eq!(char_width('ﾊ'), 1);
        assert_eq!(s(&wrap(&chars("漢字漢字"), 5)), ["漢字", "漢字"]);
        assert_eq!(s(&wrap(&chars("漢字"), 1)), ["漢", "字"]);
        for width in 1..20 {
            for piece in wrap(&chars("Zażółć gęślą jaźń 漢字 and a verylongword"), width)
            {
                let cells: usize = piece.iter().map(|&c| char_width(c)).sum();
                assert!(cells <= width.max(2) && !piece.is_empty());
            }
        }
    }

    #[test]
    fn sequence_follows_the_timeline() {
        let mut t = typer("ab\ncd e");
        t.update(1.0, false);
        assert!(t.waiting() && t.frame().is_none(), "no box before the rain");

        t.update(0.016, true);
        let f = t.frame().unwrap();
        assert_eq!(
            (text(&f), f.shown, f.cursor, f.scramble),
            ("ab".into(), 0, true, None)
        );
        t.update(0.4, true);
        assert!(!t.frame().unwrap().cursor, "blink off after 300 ms");

        t.update(4.625, true); // 5.025 s: typing, first character scrambling
        let f = t.frame().unwrap();
        assert_eq!((f.shown, f.cursor), (0, true));
        let (_, lightness) = f.scramble.unwrap();
        assert!((0.25..=1.0).contains(&lightness));
        t.update(0.05, true);
        assert_eq!(t.frame().unwrap().shown, 1);

        t.update(0.05, true); // typed both: fading
        let f = t.frame().unwrap();
        assert_eq!((f.shown, f.cursor, f.scramble), (2, false, None));
        t.update(2.475, true);
        assert!((t.frame().unwrap().level - 0.5).abs() < 0.01);

        t.update(2.525, true); // faded: 1 s cursor before the next line
        let f = t.frame().unwrap();
        assert_eq!((text(&f), f.shown, f.cursor), ("cd e".into(), 0, true));
        t.update(1.0, true);
        assert!(t.frame().unwrap().scramble.is_some(), "typing line 2");
        t.update(0.2, true);
        assert_eq!(t.frame().unwrap().shown, 4);

        t.update(5.0, true); // last line faded: box gone for 10 s
        assert!(t.frame().is_none() && !t.waiting());
        t.update(10.0, true); // and the sequence repeats
        let f = t.frame().unwrap();
        assert_eq!((text(&f), f.shown, f.cursor), ("ab".into(), 0, true));
    }

    #[test]
    fn resize_restarts_the_current_line() {
        let mut t = typer("ab\ncd e");
        t.update(0.0, true);
        t.update(5.0 + 0.1 + 5.0 + 1.0 + 0.075, true); // typing line 2
        assert_eq!(text(&t.frame().unwrap()), "cd e");

        t.set_screen_width(12); // 3 cells: "cd e" becomes "cd", "e"
        let f = t.frame().unwrap();
        assert_eq!((text(&f), f.shown, f.cursor), ("cd".into(), 0, true));

        t.set_screen_width(5); // nothing fits: hidden and frozen
        assert!(t.frame().is_none());
        t.update(100.0, true);
        t.set_screen_width(40);
        assert_eq!(text(&t.frame().unwrap()), "cd e");
    }

    #[test]
    fn large_steps_are_safe() {
        let mut t = typer("one\ntwo\nthree");
        t.update(0.0, true);
        for _ in 0..50 {
            t.update(7.3, true);
            if let Some(f) = t.frame() {
                assert!(f.shown <= f.text.len() && (0.0..=1.0).contains(&f.level));
            }
        }
    }
}
