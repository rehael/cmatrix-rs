//! Minimal Win32 console layer: raw key input, VT output, viewport size, Ctrl+Break.
//! Restores the console on drop, including during a panic unwind.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

type Handle = *mut core::ffi::c_void;

const STD_INPUT_HANDLE: u32 = -10i32 as u32;
const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;

const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
const ENABLE_PROCESSED_OUTPUT: u32 = 0x0001;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

const KEY_EVENT: u16 = 0x0001;
const VK_SHIFT: u16 = 0x10;
const VK_MENU: u16 = 0x12;

/// Alternate screen, hidden cursor, no autowrap (the bottom-right cell must not scroll).
const ENTER: &str = "\x1b[?1049h\x1b[?25l\x1b[?7l";
const LEAVE: &str = "\x1b[?2026l\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l";

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
#[derive(Default)]
struct ScreenBufferInfo {
    size: Coord,
    cursor: Coord,
    attributes: u16,
    window: SmallRect,
    max_window: Coord,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyEventRecord {
    key_down: i32,
    repeat_count: u16,
    virtual_key_code: u16,
    virtual_scan_code: u16,
    unicode_char: u16,
    control_key_state: u32,
}

/// INPUT_RECORD; KEY_EVENT_RECORD is the largest member of its event union.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct InputRecord {
    event_type: u16,
    key: KeyEventRecord,
}

const _: () = assert!(size_of::<InputRecord>() == 20);
const _: () = assert!(size_of::<ScreenBufferInfo>() == 22);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(std_handle: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    fn GetConsoleScreenBufferInfo(handle: Handle, info: *mut ScreenBufferInfo) -> i32;
    fn GetNumberOfConsoleInputEvents(handle: Handle, count: *mut u32) -> i32;
    fn ReadConsoleInputW(handle: Handle, buf: *mut InputRecord, len: u32, read: *mut u32) -> i32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

static CTRL_EVENT: AtomicBool = AtomicBool::new(false);

/// Ctrl+C arrives as a key in raw mode; this catches Ctrl+Break and close so the loop exits cleanly.
unsafe extern "system" fn on_ctrl(_event: u32) -> i32 {
    CTRL_EVENT.store(true, Ordering::Relaxed);
    1
}

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub struct Console {
    input: Handle,
    output: Handle,
    input_mode: u32,
    output_mode: u32,
    /// VT output is on and the alternate screen may be active.
    entered: bool,
}

impl Console {
    pub fn open() -> io::Result<Self> {
        let (input, output) = unsafe {
            (
                GetStdHandle(STD_INPUT_HANDLE),
                GetStdHandle(STD_OUTPUT_HANDLE),
            )
        };
        let (mut input_mode, mut output_mode) = (0, 0);
        let attached = unsafe {
            GetConsoleMode(input, &mut input_mode) != 0
                && GetConsoleMode(output, &mut output_mode) != 0
        };
        if !attached {
            return Err(io::Error::other("stdin and stdout must be a console"));
        }
        let mut console = Self {
            input,
            output,
            input_mode,
            output_mode,
            entered: false,
        };
        let raw_input = input_mode
            & !(ENABLE_PROCESSED_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_VIRTUAL_TERMINAL_INPUT);
        let vt_output = output_mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        // `console` is live from here, so an early return restores the modes via Drop.
        unsafe {
            check(SetConsoleMode(output, vt_output))?;
            check(SetConsoleMode(input, raw_input))?;
            check(SetConsoleCtrlHandler(Some(on_ctrl), 1))?;
        }
        console.entered = true;
        let mut out = io::stdout().lock();
        out.write_all(ENTER.as_bytes())?;
        out.flush()?;
        Ok(console)
    }

    /// Visible viewport in cells. `srWindow`, not `dwSize`: in conhost `dwSize`
    /// includes scrollback, which is what makes cmatrix draw off-screen on Windows.
    pub fn size(&self) -> io::Result<(usize, usize)> {
        let mut info = ScreenBufferInfo::default();
        check(unsafe { GetConsoleScreenBufferInfo(self.output, &mut info) })?;
        let w = (info.window.right - info.window.left + 1).max(0) as usize;
        let h = (info.window.bottom - info.window.top + 1).max(0) as usize;
        Ok((w, h))
    }

    /// Appends pending key presses without blocking. Non-character keys map to '\0';
    /// bare modifier presses are dropped.
    pub fn read_keys(&self, keys: &mut Vec<char>) -> io::Result<()> {
        let mut buf = [InputRecord::default(); 32];
        loop {
            let mut pending = 0;
            check(unsafe { GetNumberOfConsoleInputEvents(self.input, &mut pending) })?;
            if pending == 0 {
                return Ok(());
            }
            let mut read = 0;
            check(unsafe {
                ReadConsoleInputW(self.input, buf.as_mut_ptr(), buf.len() as u32, &mut read)
            })?;
            for rec in &buf[..read as usize] {
                let key = rec.key;
                let modifier = (VK_SHIFT..=VK_MENU).contains(&key.virtual_key_code);
                if rec.event_type == KEY_EVENT && key.key_down != 0 && !modifier {
                    keys.push(char::from_u32(u32::from(key.unicode_char)).unwrap_or('\0'));
                }
            }
        }
    }

    /// True after Ctrl+Break or a console close request.
    pub fn interrupted(&self) -> bool {
        CTRL_EVENT.load(Ordering::Relaxed)
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        if self.entered {
            let mut out = io::stdout().lock();
            let _ = out.write_all(LEAVE.as_bytes());
            let _ = out.flush();
        }
        unsafe {
            SetConsoleCtrlHandler(Some(on_ctrl), 0);
            SetConsoleMode(self.input, self.input_mode);
            SetConsoleMode(self.output, self.output_mode);
        }
    }
}
