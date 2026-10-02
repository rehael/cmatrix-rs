//! Frame composition and diff-based VT output: only cells that changed since the
//! last frame are sent, so output stays small at any terminal size.

use std::fmt::Write as _;
use std::io::{self, Write};

use crate::rain::Rain;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Glyph {
    pub ch: char,
    pub rgb: [u8; 3],
    pub bold: bool,
}

pub const BLANK: Glyph = Glyph {
    ch: ' ',
    rgb: [0; 3],
    bold: false,
};

const MESSAGE_RGB: [u8; 3] = [0xF0; 3];
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
    buf: String,
}

impl Screen {
    pub fn new(width: usize, height: usize) -> Self {
        let mut screen = Self {
            width: 0,
            back: Vec::new(),
            front: Vec::new(),
            clear: true,
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
            self.clear = false;
        }
        let mut pen = None;
        let mut cursor = None;
        for (i, (&glyph, shown)) in self.back.iter().zip(self.front.iter_mut()).enumerate() {
            if *shown == Some(glyph) {
                continue;
            }
            let (x, y) = (i % self.width, i / self.width);
            if cursor != Some((x, y)) {
                _ = write!(buf, "\x1b[{};{}H", y + 1, x + 1);
            }
            if glyph.ch != ' ' && pen != Some((glyph.rgb, glyph.bold)) {
                let [r, g, b] = glyph.rgb;
                let weight = if glyph.bold { 1 } else { 22 };
                _ = write!(buf, "\x1b[{weight};38;2;{r};{g};{b}m");
                pen = Some((glyph.rgb, glyph.bold));
            }
            buf.push(glyph.ch);
            *shown = Some(glyph);
            cursor = Some((x + 1, y));
        }
        if buf.len() == BEGIN_FRAME.len() {
            return Ok(());
        }
        buf.push_str(END_FRAME);
        out.write_all(buf.as_bytes())?;
        out.flush()
    }
}

/// Draws the rain (and optional message) into the back buffer.
pub fn compose(
    screen: &mut Screen,
    rain: &Rain,
    rgb: [u8; 3],
    rainbow: bool,
    message: Option<&str>,
) {
    screen.back.fill(BLANK);
    let (width, _) = rain.size();
    let base = |x: usize| if rainbow { rainbow_rgb(x / 2) } else { rgb };
    let cells = rain.cells();
    for (i, cell) in cells.iter().enumerate().filter(|(_, c)| c.bright > 0.0) {
        let (x, y) = (i % width, i / width);
        screen.put(
            x,
            y,
            Glyph {
                ch: cell.glyph,
                rgb: trail_rgb(base(x), cell.bright),
                bold: false,
            },
        );
    }
    for (x, y) in rain.heads() {
        let ch = cells[y * width + x].glyph;
        screen.put(
            x,
            y,
            Glyph {
                ch,
                rgb: head_rgb(base(x)),
                bold: true,
            },
        );
    }
    if let Some(message) = message {
        overlay_message(screen, message);
    }
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
                Glyph {
                    ch,
                    rgb: MESSAGE_RGB,
                    bold: true,
                }
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

    #[test]
    fn unchanged_frame_writes_nothing() {
        let mut screen = Screen::new(10, 3);
        screen.put(
            4,
            1,
            Glyph {
                ch: 'ﾊ',
                rgb: GREEN,
                bold: false,
            },
        );
        let first = flush(&mut screen);
        assert!(first.starts_with(BEGIN_FRAME) && first.ends_with(END_FRAME));
        assert!(first.contains("\x1b[2J") && first.contains("\x1b[2;5H\x1b[22;38;2;0;255;65mﾊ"));
        assert_eq!(flush(&mut screen), "");
    }

    #[test]
    fn only_changed_cells_are_sent() {
        let mut screen = Screen::new(10, 3);
        flush(&mut screen);
        screen.put(
            9,
            2,
            Glyph {
                ch: 'Z',
                rgb: GREEN,
                bold: true,
            },
        );
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
        let g = Glyph {
            ch: '1',
            rgb: GREEN,
            bold: false,
        };
        screen.put(1, 0, g);
        screen.put(2, 0, Glyph { ch: '2', ..g });
        assert_eq!(
            flush(&mut screen),
            "\x1b[?2026h\x1b[1;2H\x1b[22;38;2;0;255;65m12\x1b[?2026l"
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
            compose(&mut screen, &rain, GREEN, false, Some("Wake up, Neo\t"));
            flush(&mut screen);
        }
        let rain = Rain::new(20, 5, movie_glyphs(), Rng::new(3));
        let mut screen = Screen::new(20, 5);
        compose(&mut screen, &rain, GREEN, false, Some("Neo"));
        let row: String = screen.back[2 * 20..3 * 20].iter().map(|g| g.ch).collect();
        assert_eq!(row, "        Neo         ");
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
            compose(&mut screen, &rain, GREEN, i % 2 == 0, None);
            flush(&mut screen);
        }
        assert!(screen.back.iter().any(|g| g.bold), "heads drawn");
    }
}
