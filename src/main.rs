//! Matrix digital rain for Windows Terminal, inspired by cmatrix.

#[cfg(not(windows))]
compile_error!("cmatrix-rs targets the Windows console API");

mod args;
mod console;
mod rain;
mod render;
mod rng;

use std::io;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use args::{Command, Options};
use console::Console;
use rain::Rain;
use render::Screen;
use rng::Rng;

const FRAME: Duration = Duration::from_micros(16_667);
/// Longest simulated step, so a stalled frame does not make drops jump.
const MAX_STEP: f32 = 0.1;

fn main() -> ExitCode {
    let opts = match args::parse(std::env::args().skip(1)) {
        Ok(Command::Run(opts)) => opts,
        Ok(Command::Help) => {
            println!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("cmatrix-rs {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("cmatrix: {e}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    match run(opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cmatrix: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(mut opts: Options) -> io::Result<()> {
    let console = Console::open()?;
    let (width, height) = console.size()?;
    let glyphs = if opts.ascii {
        rain::ascii_glyphs()
    } else {
        rain::movie_glyphs()
    };
    let mut rain = Rain::new(width, height, glyphs, Rng::from_time());
    let mut screen = Screen::new(width, height);
    let mut out = io::stdout().lock();
    let mut keys = Vec::new();
    let mut paused = false;
    let mut last = Instant::now();

    loop {
        let now = Instant::now();
        keys.clear();
        console.read_keys(&mut keys)?;
        if console.interrupted() || (opts.screensaver && !keys.is_empty()) {
            return Ok(());
        }
        for &key in &keys {
            match key {
                'q' | 'Q' | '\x1b' | '\x03' => return Ok(()),
                'p' | 'P' => paused = !paused,
                'r' | 'R' => opts.rainbow = !opts.rainbow,
                '0'..='9' => opts.delay = key as u8 - b'0',
                _ => {
                    if let Some(&(_, _, rgb)) = args::COLORS.iter().find(|c| c.1 == key) {
                        opts.rgb = rgb;
                        opts.rainbow = false;
                    }
                }
            }
        }

        let size = console.size()?;
        if size != rain.size() {
            rain.resize(size.0, size.1);
            screen.resize(size.0, size.1);
        }
        let dt = now.duration_since(last).as_secs_f32().min(MAX_STEP);
        last = now;
        if !paused {
            rain.update(dt * speed_factor(opts.delay));
        }
        render::compose(
            &mut screen,
            &rain,
            opts.rgb,
            opts.rainbow,
            opts.message.as_deref(),
        );
        screen.flush(&mut out)?;

        if let Some(rest) = FRAME.checked_sub(now.elapsed()) {
            thread::sleep(rest);
        }
    }
}

/// Delay 4 (the cmatrix default) is 1x; 0 is ~1.6x, 10 is ~0.14x.
fn speed_factor(delay: u8) -> f32 {
    (11.0 - f32::from(delay)) / 7.0
}
