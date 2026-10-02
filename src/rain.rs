//! Rain simulation, time-based and independent of terminal size.
//!
//! One lane per even column (the spacing cmatrix uses). A drop's head moves down its
//! lane and writes glyphs that stay in place and fade, so the trail length in cells
//! is set by fade rate over fall speed. Glyphs mutate occasionally while visible.

use crate::rng::Rng;

/// ASCII symbols mixed in with the katakana and digits.
const SYMBOLS: &str = ":.\"=*+-<>|";

/// Fall speed range in cells per second at speed factor 1.
const SPEED: (f32, f32) = (6.0, 20.0);
/// Chance per second that a visible glyph changes.
const MUTATE_PER_SEC: f32 = 0.6;

/// Trail length in cells for a new drop.
fn trail_len(rng: &mut Rng, height: usize) -> f32 {
    4.0 + rng.below(height * 3 / 4) as f32
}

pub fn ascii_glyphs() -> Vec<char> {
    ('!'..='z').collect()
}

/// Half-width katakana ｦ to ﾝ (U+FF66 to U+FF9D), digits and a few ASCII symbols.
/// All are single-cell wide, so columns stay aligned.
pub fn movie_glyphs() -> Vec<char> {
    ('\u{FF66}'..='\u{FF9D}')
        .chain('0'..='9')
        .chain(SYMBOLS.chars())
        .collect()
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Cell {
    pub glyph: char,
    /// 1.0 when written, 0.0 when empty.
    pub bright: f32,
    /// Brightness lost per second.
    fade: f32,
}

struct Stream {
    y: f32,
    next_row: usize,
    fade: f32,
}

struct Lane {
    speed: f32,
    /// Seconds until the next drop spawns.
    wait: f32,
    drops: Vec<Stream>,
}

pub struct Rain {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    lanes: Vec<Lane>,
    glyphs: Vec<char>,
    rng: Rng,
}

impl Rain {
    /// `glyphs` must not be empty.
    pub fn new(width: usize, height: usize, glyphs: Vec<char>, rng: Rng) -> Self {
        assert!(!glyphs.is_empty());
        let mut rain = Self {
            width: 0,
            height: 0,
            cells: Vec::new(),
            lanes: Vec::new(),
            glyphs,
            rng,
        };
        rain.resize(width, height);
        rain
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Row-major, `width * height` cells.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// Positions `(x, y)` of the leading glyph of every drop on screen.
    pub fn heads(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.lanes.iter().enumerate().flat_map(|(lane, l)| {
            l.drops
                .iter()
                .filter_map(move |d| d.next_row.checked_sub(1).map(|y| (lane * 2, y)))
        })
    }

    /// Keeps whatever overlaps the new size, so resizing does not restart the rain.
    /// At startup lanes begin empty and fill from the top; lanes added by a resize
    /// start mid-fall, so a grown window has no empty region.
    pub fn resize(&mut self, width: usize, height: usize) {
        let seed = !self.cells.is_empty();
        let mut cells = vec![Cell::default(); width * height];
        let keep = width.min(self.width);
        for y in 0..height.min(self.height) {
            cells[y * width..][..keep].copy_from_slice(&self.cells[y * self.width..][..keep]);
        }
        self.cells = cells;
        self.width = width;
        self.height = height;

        let lanes = width.div_ceil(2);
        self.lanes.truncate(lanes);
        for drop in self.lanes.iter_mut().flat_map(|l| &mut l.drops) {
            drop.next_row = drop.next_row.min(height);
        }
        while self.lanes.len() < lanes {
            let speed = self.rng.range(SPEED.0, SPEED.1);
            let wait = self.rng.unit() * height as f32 / speed;
            let mut lane = Lane {
                speed,
                wait,
                drops: Vec::new(),
            };
            if seed {
                self.seed(&mut lane, self.lanes.len() * 2);
            }
            self.lanes.push(lane);
        }
    }

    /// Places a drop at a random point of its fall, trail already drawn.
    fn seed(&mut self, lane: &mut Lane, x: usize) {
        if self.height == 0 {
            return;
        }
        let trail = trail_len(&mut self.rng, self.height);
        let fade = lane.speed / trail;
        let head = self.rng.below(self.height + trail as usize);
        for row in head.saturating_sub(trail as usize)..=head.min(self.height - 1) {
            let bright = 1.0 - (head - row) as f32 / trail;
            let glyph = self.rng.pick(&self.glyphs);
            self.cells[row * self.width + x] = Cell {
                glyph,
                bright,
                fade,
            };
        }
        if head < self.height {
            lane.drops.push(Stream {
                y: head as f32,
                next_row: head + 1,
                fade,
            });
        }
    }

    /// Advances the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let Self {
            width,
            height,
            cells,
            lanes,
            glyphs,
            rng,
        } = self;
        let (width, height) = (*width, *height);

        for cell in cells.iter_mut().filter(|c| c.bright > 0.0) {
            cell.bright -= cell.fade * dt;
            if cell.bright <= 0.0 {
                cell.bright = 0.0;
            } else if rng.unit() < MUTATE_PER_SEC * dt {
                cell.glyph = rng.pick(glyphs);
            }
        }

        for (lane_idx, lane) in lanes.iter_mut().enumerate() {
            let x = lane_idx * 2;
            lane.wait -= dt;
            if lane.wait <= 0.0 {
                if lane.drops.is_empty() {
                    lane.speed = rng.range(SPEED.0, SPEED.1);
                }
                let trail = trail_len(rng, height);
                let gap = 1.0 + rng.below(height) as f32;
                lane.drops.push(Stream {
                    y: 0.0,
                    next_row: 0,
                    fade: lane.speed / trail,
                });
                lane.wait = (trail + gap) / lane.speed;
            }
            for drop in &mut lane.drops {
                drop.y += lane.speed * dt;
                while drop.next_row < height && drop.next_row as f32 <= drop.y {
                    let glyph = rng.pick(glyphs);
                    cells[drop.next_row * width + x] = Cell {
                        glyph,
                        bright: 1.0,
                        fade: drop.fade,
                    };
                    drop.next_row += 1;
                }
                // The head stays at full brightness until it moves on.
                if let Some(head) = drop.next_row.checked_sub(1) {
                    cells[head * width + x].bright = 1.0;
                }
            }
            lane.drops.retain(|d| d.y < height as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rain(w: usize, h: usize) -> Rain {
        Rain::new(w, h, movie_glyphs(), Rng::new(7))
    }

    fn run(r: &mut Rain, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            r.update(1.0 / 60.0);
            let (w, h) = r.size();
            assert_eq!(r.cells().len(), w * h);
            assert!(
                r.heads().all(|(x, y)| x < w && y < h),
                "head outside {w}x{h}"
            );
        }
    }

    #[test]
    fn glyphs_are_single_width() {
        // Half-width katakana block and ASCII; anything else may render two cells wide.
        let narrow = |c: char| c.is_ascii_graphic() || ('\u{FF61}'..='\u{FF9F}').contains(&c);
        assert!(movie_glyphs().into_iter().all(narrow));
        assert!(ascii_glyphs().into_iter().all(narrow));
    }

    #[test]
    fn any_size_including_degenerate() {
        for (w, h) in [
            (0, 0),
            (0, 5),
            (5, 0),
            (1, 1),
            (1, 80),
            (2, 1),
            (3, 2),
            (80, 24),
            (400, 120),
        ] {
            let mut r = rain(w, h);
            run(&mut r, 5.0);
        }
    }

    #[test]
    fn rain_fills_screen() {
        let mut r = rain(80, 24);
        run(&mut r, 10.0);
        let lit = r.cells().iter().filter(|c| c.bright > 0.0).count();
        assert!(lit > 80 * 24 / 20, "only {lit} lit cells");
        assert!(r.heads().count() > 0);
        // Only even columns carry drops.
        for (i, c) in r.cells().iter().enumerate() {
            assert!(c.bright == 0.0 || i % 80 % 2 == 0);
        }
    }

    #[test]
    fn trail_fades_out() {
        let mut r = rain(2, 30);
        run(&mut r, 3.0);
        let before: Vec<Cell> = r.cells().to_vec();
        // Stop spawning: no lane, no new drops; existing cells must fade to empty.
        r.lanes.clear();
        run(&mut r, 30.0);
        assert!(before.iter().any(|c| c.bright > 0.0));
        assert!(r.cells().iter().all(|c| c.bright == 0.0));
    }

    #[test]
    fn resize_keeps_overlap_and_survives_storms() {
        let mut r = rain(60, 20);
        run(&mut r, 5.0);
        let old = r.cells().to_vec();
        r.resize(30, 10);
        for y in 0..10 {
            assert_eq!(r.cells()[y * 30..][..30], old[y * 60..][..30]);
        }
        let mut rng = Rng::new(1);
        for _ in 0..200 {
            r.resize(rng.below(300), rng.below(100));
            run(&mut r, 0.05);
        }
    }

    #[test]
    fn startup_is_empty_but_growth_is_seeded() {
        let mut r = rain(10, 40);
        assert!(r.cells().iter().all(|c| c.bright == 0.0));
        r.resize(200, 40);
        let new_area_lit = (0..40)
            .flat_map(|y| (10..200).map(move |x| (x, y)))
            .filter(|&(x, y)| r.cells()[y * 200 + x].bright > 0.0)
            .count();
        assert!(
            new_area_lit > 200,
            "only {new_area_lit} lit cells in new area"
        );
        assert!(r.heads().any(|(x, _)| x >= 10));
        run(&mut r, 1.0);
    }

    #[test]
    fn large_step_is_safe() {
        let mut r = rain(10, 10);
        r.update(100.0);
        r.update(0.0);
        assert!(r.heads().all(|(x, y)| x < 10 && y < 10));
    }
}
