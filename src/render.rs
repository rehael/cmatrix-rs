//! Frame composition and diff-based VT output: only cells that changed since the
//! last frame are sent, so output stays small at any terminal size.

use std::fmt::Write as _;
use std::io::{self, Write};

use crate::rain::Rain;
use crate::typer::{TextFrame, char_width};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Glyph {
    pub ch: char,
    pub rgb: [u8; 3],
    pub bold: bool,
    pub bg: [u8; 3],
    /// Underline colour; `None` is no underline.
    pub ul: Option<[u8; 3]>,
}

impl Glyph {
    pub const fn new(ch: char, rgb: [u8; 3], bold: bool) -> Self {
        Self {
            ch,
            rgb,
            bold,
            bg: BLACK,
            ul: None,
        }
    }
}

const BLACK: [u8; 3] = [0; 3];
pub const BLANK: Glyph = Glyph::new(' ', BLACK, false);
/// Right half of a two-cell character; the terminal draws it, so it is never sent.
const WIDE_TAIL: char = '\0';

const MESSAGE_RGB: [u8; 3] = [0xF0; 3];
/// Brightness of the rain behind the `-F` box.
const BOX_DIM: f32 = 0.25;
/// Brightness steps; quantising keeps slow fades from resending a cell every frame.
const LEVELS: f32 = 32.0;
/// Synchronized output (DEC mode 2026): the terminal presents each frame whole.
const BEGIN_FRAME: &str = "\x1b[?2026h";
const END_FRAME: &str = "\x1b[?2026l";

pub struct Screen {
    width: usize,
    back: Vec<Glyph>,
    /// What the terminal shows; `None` = unknown.
    front: Vec<Option<Glyph>>,
    clear: bool,
    /// Terminal colour state carried between frames: foreground with weight,
    /// underline, background.
    pen: Option<([u8; 3], bool)>,
    pen_ul: Option<[u8; 3]>,
    pen_bg: [u8; 3],
    buf: String,
}

impl Screen {
    pub fn new(width: usize, height: usize) -> Self {
        let mut screen = Self {
            width: 0,
            back: Vec::new(),
            front: Vec::new(),
            clear: true,
            pen: None,
            pen_ul: None,
            pen_bg: BLACK,
            buf: String::new(),
        };
        screen.resize(width, height);
        screen
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.back = vec![BLANK; width * height];
        self.front = vec![None; width * height];
        self.clear = true;
    }

    fn height(&self) -> usize {
        self.back.len().checked_div(self.width).unwrap_or(0)
    }

    pub fn put(&mut self, x: usize, y: usize, glyph: Glyph) {
        if x < self.width && y < self.height() {
            self.back[y * self.width + x] = glyph;
        }
    }

    /// Writes the difference between the composed frame and the terminal in one write.
    pub fn flush(&mut self, out: &mut impl Write) -> io::Result<()> {
        let buf = &mut self.buf;
        buf.clear();
        buf.push_str(BEGIN_FRAME);
        if self.clear {
            // Erase paints with the current background, giving a black canvas.
            buf.push_str("\x1b[0m\x1b[48;2;0;0;0m\x1b[2J");
            self.front.fill(Some(BLANK));
            self.pen = None;
            self.pen_ul = None;
            self.pen_bg = BLACK;
            self.clear = false;
        }
        let mut cursor = None;
        for (i, (&glyph, shown)) in self.back.iter().zip(self.front.iter_mut()).enumerate() {
            if *shown == Some(glyph) {
                continue;
            }
            *shown = Some(glyph);
            if glyph.ch == WIDE_TAIL {
                continue;
            }
            let (x, y) = (i % self.width, i / self.width);
            if cursor != Some((x, y)) {
                _ = write!(buf, "\x1b[{};{}H", y + 1, x + 1);
            }
            let fg = glyph.ch != ' ' && self.pen != Some((glyph.rgb, glyph.bold));
            let ul = self.pen_ul != glyph.ul;
            let bg = self.pen_bg != glyph.bg;
            if fg || ul || bg {
                buf.push_str("\x1b[");
                let mut sep = "";
                if fg {
                    let [r, g, b] = glyph.rgb;
                    let weight = if glyph.bold { 1 } else { 22 };
                    _ = write!(buf, "{weight};38;2;{r};{g};{b}");
                    self.pen = Some((glyph.rgb, glyph.bold));
                    sep = ";";
                }
                if ul {
                    // Underline colour uses the colon form (ITU T.416), as Windows Terminal documents it.
                    match glyph.ul {
                        Some([r, g, b]) => _ = write!(buf, "{sep}4;58:2::{r}:{g}:{b}"),
                        None => _ = write!(buf, "{sep}24"),
                    }
                    self.pen_ul = glyph.ul;
                    sep = ";";
                }
                if bg {
                    let [r, g, b] = glyph.bg;
                    _ = write!(buf, "{sep}48;2;{r};{g};{b}");
                    self.pen_bg = glyph.bg;
                }
                buf.push('m');
            }
            buf.push(glyph.ch);
            cursor = Some((x + char_width(glyph.ch), y));
        }
        if buf.len() == BEGIN_FRAME.len() {
            return Ok(());
        }
        buf.push_str(END_FRAME);
        out.write_all(buf.as_bytes())?;
        out.flush()
    }
}

/// Draws the rain, then the `-F` text box or the `-M` message, into the back buffer.
pub fn compose(
    screen: &mut Screen,
    rain: &Rain,
    rgb: [u8; 3],
    rainbow: bool,
    message: Option<&str>,
    text: Option<&TextFrame>,
) {
    screen.back.fill(BLANK);
    let (width, _) = rain.size();
    let base = |x: usize| if rainbow { rainbow_rgb(x / 2) } else { rgb };
    let cells = rain.cells();
    for (i, cell) in cells.iter().enumerate().filter(|(_, c)| c.bright > 0.0) {
        let (x, y) = (i % width, i / width);
        let rgb = trail_rgb(base(x), cell.bright);
        screen.put(x, y, Glyph::new(cell.glyph, rgb, false));
    }
    for (x, y) in rain.heads() {
        let ch = cells[y * width + x].glyph;
        screen.put(x, y, Glyph::new(ch, head_rgb(base(x)), true));
    }
    if let Some(text) = text {
        overlay_text(screen, text, base);
    }
    if let Some(message) = message {
        overlay_message(screen, message);
    }
}

/// The `-F` box: 3 rows centred vertically, full width except 2 cells of rain on each
/// side. The rain inside is dimmed; text starts after 2 cells of padding. The cursor
/// takes the rain colour of its column: a block while idle, an underline while typing.
fn overlay_text(screen: &mut Screen, frame: &TextFrame, base: impl Fn(usize) -> [u8; 3]) {
    let (width, height) = (screen.width, screen.height());
    if width < 10 || height < 3 {
        return;
    }
    let y0 = height / 2 - 1;
    for y in y0..y0 + 3 {
        for glyph in &mut screen.back[y * width + 2..y * width + width - 2] {
            glyph.rgb = scale(glyph.rgb, BOX_DIM);
        }
    }
    let y = y0 + 1;
    let rgb = [(255.0 * frame.level.clamp(0.0, 1.0)).round() as u8; 3];
    let mut x = 4;
    for &ch in &frame.text[..frame.shown] {
        if ch != ' ' {
            screen.put(x, y, Glyph::new(ch, rgb, true));
            if char_width(ch) == 2 {
                screen.put(x + 1, y, Glyph::new(WIDE_TAIL, rgb, true));
            }
        }
        x += char_width(ch);
    }
    if frame.cursor {
        let bright = base(x);
        let glyph = match frame.scramble {
            Some((ch, lightness)) => Glyph {
                ul: Some(bright),
                ..Glyph::new(ch, scale(bright, lightness), false)
            },
            None => Glyph {
                bg: bright,
                ..BLANK
            },
        };
        screen.put(x, y, glyph);
    }
}

fn scale(rgb: [u8; 3], k: f32) -> [u8; 3] {
    rgb.map(|c| (f32::from(c) * k).round() as u8)
}

/// Three-row box centred on screen, clipped to fit, like cmatrix -M.
fn overlay_message(screen: &mut Screen, message: &str) {
    let (width, height) = (screen.width, screen.height());
    let text: Vec<char> = format!("  {message}  ")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(width)
        .collect();
    let x0 = (width - text.len()) / 2;
    let y0 = (height / 2).saturating_sub(1);
    for (dy, row) in [None, Some(&text), None].into_iter().enumerate() {
        for dx in 0..text.len() {
            let ch = row.map_or(' ', |t| t[dx]);
            let glyph = if ch == ' ' {
                BLANK
            } else {
                Glyph::new(ch, MESSAGE_RGB, true)
            };
            screen.put(x0 + dx, y0 + dy, glyph);
        }
    }
}

/// Dims with age; the freshest cells, right behind the head, lean towards white.
pub fn trail_rgb(base: [u8; 3], bright: f32) -> [u8; 3] {
    let q = (bright.clamp(0.0, 1.0) * LEVELS).ceil() / LEVELS;
    let glow = ((q - 0.85) / 0.15).clamp(0.0, 1.0) * 0.35;
    let k = 0.1 + 0.9 * q;
    base.map(|c| {
        let v = f32::from(c) * k;
        (v + (255.0 - v) * glow).round() as u8
    })
}

fn head_rgb(base: [u8; 3]) -> [u8; 3] {
    base.map(|c| (f32::from(c) + (255.0 - f32::from(c)) * 0.8).round() as u8)
}

/// Hue sweep across lanes.
fn rainbow_rgb(lane: usize) -> [u8; 3] {
    let h = (lane as f32 * 0.04).fract() * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let rgb = match h as u8 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    };
    rgb.map(|c: f32| (c * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rain::movie_glyphs;
    use crate::rng::Rng;

    const GREEN: [u8; 3] = [0x00, 0xFF, 0x41];

    fn flush(screen: &mut Screen) -> String {
        let mut out = Vec::new();
        screen.flush(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn row(screen: &Screen, y: usize) -> String {
        screen.back[y * screen.width..(y + 1) * screen.width]
            .iter()
            .map(|g| g.ch)
            .collect()
    }

    #[test]
    fn unchanged_frame_writes_nothing() {
        let mut screen = Screen::new(10, 3);
        screen.put(4, 1, Glyph::new('ﾊ', GREEN, false));
        let first = flush(&mut screen);
        assert!(first.starts_with(BEGIN_FRAME) && first.ends_with(END_FRAME));
        assert!(first.contains("\x1b[2J") && first.contains("\x1b[2;5H\x1b[22;38;2;0;255;65mﾊ"));
        assert_eq!(flush(&mut screen), "");
    }

    #[test]
    fn only_changed_cells_are_sent() {
        let mut screen = Screen::new(10, 3);
        flush(&mut screen);
        screen.put(9, 2, Glyph::new('Z', GREEN, true));
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[3;10H\x1b[1;38;2;0;255;65mZ\x1b[?2026l"
        );
        screen.put(9, 2, BLANK);
        assert_eq!(flush(&mut screen), "\x1b[?2026h\x1b[3;10H \x1b[?2026l");
    }

    #[test]
    fn adjacent_cells_share_cursor_and_pen() {
        let mut screen = Screen::new(4, 1);
        flush(&mut screen);
        screen.put(1, 0, Glyph::new('1', GREEN, false));
        screen.put(2, 0, Glyph::new('2', GREEN, false));
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[22;38;2;0;255;65m12\x1b[?2026l"
        );
    }

    #[test]
    fn background_is_set_and_restored() {
        let mut screen = Screen::new(4, 1);
        flush(&mut screen);
        let cursor = Glyph { bg: GREEN, ..BLANK };
        screen.put(1, 0, cursor);
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[48;2;0;255;65m \x1b[?2026l"
        );
        screen.put(1, 0, BLANK);
        screen.put(2, 0, cursor);
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[48;2;0;0;0m \x1b[48;2;0;255;65m \x1b[?2026l"
        );
    }

    #[test]
    fn underline_is_set_and_cleared() {
        let mut screen = Screen::new(3, 1);
        flush(&mut screen);
        let typing = Glyph {
            ul: Some(GREEN),
            ..Glyph::new('Z', [64; 3], false)
        };
        screen.put(1, 0, typing);
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[22;38;2;64;64;64;4;58:2::0:255:65mZ\x1b[?2026l"
        );
        screen.put(1, 0, Glyph::new('a', [255; 3], true));
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[1;38;2;255;255;255;24ma\x1b[?2026l"
        );
    }

    #[test]
    fn wide_character_tail_is_not_sent() {
        let mut screen = Screen::new(5, 1);
        flush(&mut screen);
        screen.put(1, 0, Glyph::new('漢', GREEN, false));
        screen.put(2, 0, Glyph::new(WIDE_TAIL, GREEN, false));
        screen.put(3, 0, Glyph::new('x', GREEN, false));
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[22;38;2;0;255;65m漢x\x1b[?2026l"
        );
    }

    #[test]
    fn resize_forces_full_clear() {
        let mut screen = Screen::new(3, 3);
        flush(&mut screen);
        screen.resize(5, 2);
        assert!(flush(&mut screen).contains("\x1b[2J"));
    }

    #[test]
    fn out_of_bounds_put_is_ignored() {
        let mut screen = Screen::new(0, 0);
        screen.put(0, 0, BLANK);
        let mut screen = Screen::new(2, 2);
        screen.put(2, 0, BLANK);
        screen.put(0, 2, BLANK);
    }

    #[test]
    fn message_is_centred_and_clipped() {
        for (w, h) in [(0, 0), (1, 1), (3, 1), (40, 10)] {
            let rain = Rain::new(w, h, movie_glyphs(), Rng::new(3));
            let mut screen = Screen::new(w, h);
            compose(
                &mut screen,
                &rain,
                GREEN,
                false,
                Some("Wake up, Neo\t"),
                None,
            );
            flush(&mut screen);
        }
        let rain = Rain::new(20, 5, movie_glyphs(), Rng::new(3));
        let mut screen = Screen::new(20, 5);
        compose(&mut screen, &rain, GREEN, false, Some("Neo"), None);
        assert_eq!(row(&screen, 2), "        Neo         ");
    }

    #[test]
    fn text_box_layout() {
        let rain = Rain::new(20, 5, movie_glyphs(), Rng::new(3));
        let mut screen = Screen::new(20, 5);
        let text: Vec<char> = "a 漢b".chars().collect();
        let mut frame = TextFrame {
            text: &text,
            shown: 3,
            scramble: Some(('Z', 0.5)),
            cursor: true,
            level: 1.0,
        };
        compose(&mut screen, &rain, GREEN, false, None, Some(&frame));
        // 2 rain + 2 padding, "a", space, wide character, then the cursor.
        assert_eq!(row(&screen, 2), format!("    a 漢\0Z{}", " ".repeat(11)));
        let a = screen.back[2 * 20 + 4];
        assert_eq!((a.rgb, a.bold, a.bg, a.ul), ([255; 3], true, BLACK, None));
        // Typing: half-lightness rain colour, underlined in the full rain colour.
        let typing = screen.back[2 * 20 + 8];
        assert_eq!(
            (typing.rgb, typing.ul, typing.bg),
            ([0, 128, 33], Some(GREEN), BLACK)
        );

        frame.scramble = None;
        compose(&mut screen, &rain, GREEN, false, None, Some(&frame));
        let idle = screen.back[2 * 20 + 8];
        assert_eq!(
            (idle.ch, idle.bg, idle.ul),
            (' ', GREEN, None),
            "block cursor"
        );
    }

    #[test]
    fn text_box_dims_rain_and_fits_any_size() {
        let mut rain = Rain::new(30, 9, movie_glyphs(), Rng::new(4));
        for _ in 0..300 {
            rain.update(1.0 / 60.0);
        }
        let text: Vec<char> = "x".chars().collect();
        let frame = TextFrame {
            text: &text,
            shown: 0,
            scramble: None,
            cursor: false,
            level: 1.0,
        };
        let mut plain = Screen::new(30, 9);
        compose(&mut plain, &rain, GREEN, false, None, None);
        let mut boxed = Screen::new(30, 9);
        compose(&mut boxed, &rain, GREEN, false, None, Some(&frame));
        for (i, (p, b)) in plain.back.iter().zip(&boxed.back).enumerate() {
            let (x, y) = (i % 30, i / 30);
            let inside = (3..6).contains(&y) && (2..28).contains(&x);
            let dimmed = p.rgb.map(|c| (f32::from(c) * BOX_DIM).round() as u8);
            assert_eq!(b.rgb, if inside { dimmed } else { p.rgb }, "cell {x},{y}");
        }
        for (w, h) in [(0, 0), (9, 9), (10, 2), (10, 3), (1, 50)] {
            let rain = Rain::new(w, h, movie_glyphs(), Rng::new(1));
            let mut screen = Screen::new(w, h);
            compose(&mut screen, &rain, GREEN, false, None, Some(&frame));
            flush(&mut screen);
        }
    }

    #[test]
    fn trail_darkens_with_age() {
        let mut prev = trail_rgb(GREEN, 1.0);
        for i in (1..100).rev() {
            let next = trail_rgb(GREEN, i as f32 / 100.0);
            assert!(next[1] <= prev[1], "{next:?} brighter than {prev:?}");
            prev = next;
        }
        assert!(trail_rgb(GREEN, 0.01)[1] > 0, "faintest cell still visible");
    }

    #[test]
    fn composed_rain_stays_in_bounds() {
        let mut rain = Rain::new(33, 7, movie_glyphs(), Rng::new(9));
        let mut screen = Screen::new(33, 7);
        for i in 0..600 {
            rain.update(1.0 / 60.0);
            compose(&mut screen, &rain, GREEN, i % 2 == 0, None, None);
            flush(&mut screen);
        }
        assert!(screen.back.iter().any(|g| g.bold), "heads drawn");
    }
}
