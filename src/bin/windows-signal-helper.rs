//! Windows Signal Helper
//!
//! A small helper binary for sending console control events (Ctrl+C/Ctrl+Break) to processes on Windows.
//! This is needed because Node.js child_process.kill() doesn't trigger Rust's ctrlc handler on Windows.
//!
//! Usage: windows-signal-helper <pid>
//! Returns exit code 0 on success, 1 on error.

#[cfg(windows)]
use std::io::{self, Write};
#[cfg(windows)]
use windows::Win32::System::Console::{
    GenerateConsoleCtrlEvent, CTRL_BREAK_EVENT, CTRL_C_EVENT,
};

#[cfg(not(windows))]
fn main() {
    eprintln!("Error: This binary is only for Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <pid>", args[0]);
        std::process::exit(1);
    }

    let pid_str = &args[1];
    let pid: u32 = match pid_str.parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Error: Invalid PID '{}'", pid_str);
            std::process::exit(1);
        }
    };

    // Try Ctrl+Break first (more reliable for graceful shutdown)
    match send_ctrl_break(pid) {
        Ok(()) => {
            println!("Successfully sent Ctrl+Break to process {}", pid);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Warning: Failed to send Ctrl+Break: {}", e);
            // Fall back to Ctrl+C
        }
    }

    // Fall back to Ctrl+C
    match send_ctrl_c(pid) {
        Ok(()) => {
            println!("Successfully sent Ctrl+C to process {}", pid);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error: Failed to send Ctrl+C: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn send_ctrl_break(pid: u32) -> io::Result<()> {
    unsafe {
        GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("GenerateConsoleCtrlEvent failed: {:?}", e)))?;
    }
    Ok(())
}

#[cfg(windows)]
fn send_ctrl_c(pid: u32) -> io::Result<()> {
    unsafe {
        GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("GenerateConsoleCtrlEvent failed: {:?}", e)))?;
    }
    Ok(())
}
