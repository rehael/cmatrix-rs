// conkey <pid> <char>: writes one key press into the console input of process <pid>.
// Independent of window focus. Build: rustc --edition 2024 -O conkey.rs
type Handle = *mut core::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyEventRecord {
    key_down: i32,
    repeat_count: u16,
    virtual_key_code: u16,
    virtual_scan_code: u16,
    unicode_char: u16,
    control_key_state: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InputRecord {
    event_type: u16,
    key: KeyEventRecord,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn FreeConsole() -> i32;
    fn AttachConsole(pid: u32) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut core::ffi::c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn WriteConsoleInputW(h: Handle, recs: *const InputRecord, n: u32, written: *mut u32) -> i32;
    fn GetLastError() -> u32;
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(pid), Some(ch)) = (
        args.next().and_then(|a| a.parse::<u32>().ok()),
        args.next().and_then(|a| a.chars().next()),
    ) else {
        eprintln!("usage: conkey <pid> <char>");
        std::process::exit(2);
    };
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            eprintln!("AttachConsole failed: {}", GetLastError());
            std::process::exit(1);
        }
        let name: Vec<u16> = "CONIN$\0".encode_utf16().collect();
        // GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING
        let h = CreateFileW(name.as_ptr(), 0xC000_0000, 3, core::ptr::null_mut(), 3, 0, core::ptr::null_mut());
        let key = |down| InputRecord {
            event_type: 1, // KEY_EVENT
            key: KeyEventRecord {
                key_down: down,
                repeat_count: 1,
                virtual_key_code: ch.to_ascii_uppercase() as u16,
                virtual_scan_code: 0,
                unicode_char: ch as u16,
                control_key_state: 0,
            },
        };
        let recs = [key(1), key(0)];
        let mut written = 0;
        if WriteConsoleInputW(h, recs.as_ptr(), 2, &mut written) == 0 {
            eprintln!("WriteConsoleInputW failed: {}", GetLastError());
            std::process::exit(1);
        }
    }
}
