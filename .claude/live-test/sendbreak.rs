// sendbreak <pid>: raises CTRL_BREAK_EVENT on the console that process <pid> is attached to.
// Every process on that console receives it; sendbreak itself ignores it.
// Build: rustc --edition 2024 -O sendbreak.rs
#[link(name = "kernel32")]
unsafe extern "system" {
    fn FreeConsole() -> i32;
    fn AttachConsole(pid: u32) -> i32;
    fn SetConsoleCtrlHandler(h: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
    fn GetLastError() -> u32;
}

unsafe extern "system" fn ignore(_: u32) -> i32 {
    1
}

fn main() {
    let Some(pid) = std::env::args().nth(1).and_then(|a| a.parse::<u32>().ok()) else {
        eprintln!("usage: sendbreak <pid>");
        std::process::exit(2);
    };
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            eprintln!("AttachConsole failed: {}", GetLastError());
            std::process::exit(1);
        }
        SetConsoleCtrlHandler(Some(ignore), 1);
        let ok = GenerateConsoleCtrlEvent(1, 0); // CTRL_BREAK_EVENT, all processes on the console
        let err = GetLastError();
        std::thread::sleep(std::time::Duration::from_millis(500));
        FreeConsole();
        std::process::exit(if ok != 0 { 0 } else { err as i32 });
    }
}
