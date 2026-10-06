//! Command line, a cmatrix-compatible subset. Short flags may be combined (`-rs`)
//! and values attached (`-u2`) or separate (`-u 2`).

pub const USAGE: &str = "\
Usage: cmatrix [-hrsV] [--ascii] [-C color] [-F file | -M message] [-u delay]
  -C color    green (default), red, blue, yellow, cyan, magenta, white
  -F file     type the file's lines one by one in a box over the rain, repeating;
              '-' reads stdin
  -M message  show a message in the centre of the screen
  -r          rainbow mode
  -s          screensaver: exit on the first key press
  -u delay    0 (fast) to 10 (slow), default 4
  --ascii     ASCII glyphs, for fonts without half-width katakana
  -h          help
  -V          version

Keys: q, Esc, Ctrl+C quit; p pause; r rainbow; 0-9 speed;
      ! red, @ green, # yellow, $ blue, % magenta, ^ cyan, & white";

/// Name, runtime key (as in cmatrix), RGB.
pub const COLORS: [(&str, char, [u8; 3]); 7] = [
    ("green", '@', [0x00, 0xFF, 0x41]),
    ("red", '!', [0xFF, 0x30, 0x30]),
    ("blue", '$', [0x30, 0x80, 0xFF]),
    ("yellow", '#', [0xFF, 0xE0, 0x20]),
    ("cyan", '^', [0x00, 0xE0, 0xFF]),
    ("magenta", '%', [0xFF, 0x30, 0xE0]),
    ("white", '&', [0xE0, 0xE0, 0xE0]),
];

#[derive(Debug, PartialEq)]
pub struct Options {
    pub rgb: [u8; 3],
    pub rainbow: bool,
    pub message: Option<String>,
    /// `-F` source; "-" is stdin.
    pub text_file: Option<String>,
    pub screensaver: bool,
    pub delay: u8,
    pub ascii: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            rgb: COLORS[0].2,
            rainbow: false,
            message: None,
            text_file: None,
            screensaver: false,
            delay: 4,
            ascii: false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Run(Options),
    Help,
    Version,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut opts = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ascii" => {
                opts.ascii = true;
                continue;
            }
            "--help" => return Ok(Command::Help),
            "--version" => return Ok(Command::Version),
            long if long.starts_with("--") => return Err(format!("unknown option '{arg}'")),
            _ => {}
        }
        let Some(flags) = arg.strip_prefix('-').filter(|f| !f.is_empty()) else {
            return Err(format!("unexpected argument '{arg}'"));
        };
        for (i, flag) in flags.char_indices() {
            match flag {
                'h' => return Ok(Command::Help),
                'V' => return Ok(Command::Version),
                'r' => opts.rainbow = true,
                's' => opts.screensaver = true,
                'C' | 'F' | 'M' | 'u' => {
                    let attached = &flags[i + flag.len_utf8()..];
                    let value = match attached {
                        "" => args
                            .next()
                            .ok_or_else(|| format!("option -{flag} needs a value"))?,
                        _ => attached.to_string(),
                    };
                    match flag {
                        'C' => opts.rgb = color(&value)?,
                        'F' => opts.text_file = Some(value),
                        'M' => opts.message = Some(value),
                        _ => opts.delay = delay(&value)?,
                    }
                    break;
                }
                _ => return Err(format!("unknown option -{flag}")),
            }
        }
    }
    if opts.message.is_some() && opts.text_file.is_some() {
        return Err("-F and -M cannot be combined".into());
    }
    Ok(Command::Run(opts))
}

fn color(name: &str) -> Result<[u8; 3], String> {
    COLORS
        .iter()
        .find(|c| c.0 == name)
        .map(|c| c.2)
        .ok_or_else(|| format!("invalid color '{name}'"))
}

fn delay(value: &str) -> Result<u8, String> {
    value
        .parse()
        .ok()
        .filter(|d| *d <= 10)
        .ok_or_else(|| format!("delay must be 0-10, got '{value}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults() {
        assert_eq!(run(&[]), Ok(Command::Run(Options::default())));
    }

    #[test]
    fn combined_and_attached() {
        let Ok(Command::Run(o)) = run(&["-rsu2", "-C", "red", "-MWake up", "--ascii"]) else {
            panic!("parse failed");
        };
        assert!(o.rainbow && o.screensaver && o.ascii);
        assert_eq!(
            (o.delay, o.rgb, o.message.as_deref()),
            (2, [0xFF, 0x30, 0x30], Some("Wake up"))
        );
    }

    #[test]
    fn text_file() {
        for (args, file) in [
            (&["-F", "neo.txt"][..], "neo.txt"),
            (&["-F", "-"], "-"),
            (&["-F-"], "-"),
            (&["-rFneo.txt"], "neo.txt"),
        ] {
            let Ok(Command::Run(o)) = run(args) else {
                panic!("{args:?} rejected");
            };
            assert_eq!(o.text_file.as_deref(), Some(file));
        }
    }

    #[test]
    fn help_and_version() {
        assert_eq!(run(&["-h"]), Ok(Command::Help));
        assert_eq!(run(&["--help"]), Ok(Command::Help));
        assert_eq!(run(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn errors() {
        for bad in [
            &["-u", "11"][..],
            &["-u"],
            &["-C", "pink"],
            &["-x"],
            &["foo"],
            &["-"],
            &["--bogus"],
            &["-F"],
            &["-F", "a.txt", "-M", "hi"],
        ] {
            assert!(run(bad).is_err(), "{bad:?} accepted");
        }
    }
}
