//! Signal sender module for cross-platform signal delivery
//!
//! This module provides a unified interface for sending signals to processes
//! across different platforms. On Windows, it uses native console control events
//! (CTRL_C_EVENT, CTRL_BREAK_EVENT). On Unix, it uses standard signals (SIGTERM, SIGINT).

use anyhow::Result;

/// Signal kinds that can be sent to a process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// Windows: CTRL_BREAK_EVENT, Unix: SIGTERM
    CtrlBreak,
    /// Windows: CTRL_C_EVENT, Unix: SIGINT
    CtrlC,
    /// Unix: SIGTERM, Windows: CTRL_BREAK_EVENT
    Term,
    /// Unix: SIGINT, Windows: CTRL_C_EVENT
    Int,
}

impl SignalKind {
    /// Parse signal kind from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ctrl-break" => Ok(Self::CtrlBreak),
            "ctrl-c" => Ok(Self::CtrlC),
            "term" => Ok(Self::Term),
            "int" => Ok(Self::Int),
            _ => anyhow::bail!("Unknown signal kind: {}", s),
        }
    }
}

/// Send a signal to a process
pub fn send_signal(pid: u32, kind: SignalKind) -> Result<()> {
    #[cfg(windows)]
    {
        send_signal_windows(pid, kind)
    }

    #[cfg(unix)]
    {
        send_signal_unix(pid, kind)
    }
}

#[cfg(windows)]
fn send_signal_windows(pid: u32, kind: SignalKind) -> Result<()> {
    use windows_sys::Win32::System::Console::{
        AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, FreeConsole, GenerateConsoleCtrlEvent,
        SetConsoleCtrlHandler,
    };

    // Map signal kind to Windows console control event
    let event = match kind {
        SignalKind::CtrlBreak | SignalKind::Term => CTRL_BREAK_EVENT,
        SignalKind::CtrlC | SignalKind::Int => CTRL_C_EVENT,
    };

    unsafe {
        // Step 1: Detach from current console (if any)
        // This is required because AttachConsole fails with ERROR_ACCESS_DENIED
        // if the calling process is already attached to a console
        FreeConsole();

        // Step 2: Attach to the target process's console
        if AttachConsole(pid) == 0 {
            let err = std::io::Error::last_os_error();
            anyhow::bail!(
                "Failed to attach to console of process {}: {}\n\
                 Hint: Make sure the target process has a console window.\n\
                 If running in CI or background, consider using CREATE_NEW_CONSOLE flag when spawning.",
                pid,
                err
            );
        }

        // Step 3: Ignore CTRL events for this process
        // Without this, the signal sender itself would also receive the CTRL event
        SetConsoleCtrlHandler(None, 1); // TRUE = ignore

        // Step 4: Send the console control event to the process group
        let result = GenerateConsoleCtrlEvent(event, 0);

        // Step 5: Wait briefly for the event to be delivered
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Step 6: Detach from the target console
        FreeConsole();

        // Step 7: Re-enable CTRL event handling for this process
        SetConsoleCtrlHandler(None, 0); // FALSE = handle normally

        if result == 0 {
            anyhow::bail!(
                "Failed to send signal to process {}: {}",
                pid,
                std::io::Error::last_os_error()
            );
        }
    }

    Ok(())
}

#[cfg(unix)]
fn send_signal_unix(pid: u32, kind: SignalKind) -> Result<()> {
    use anyhow::Context;
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;

    // Map signal kind to Unix signal
    let signal = match kind {
        SignalKind::CtrlBreak | SignalKind::Term => Signal::SIGTERM,
        SignalKind::CtrlC | SignalKind::Int => Signal::SIGINT,
    };

    let pid = Pid::from_raw(pid as i32);
    signal::kill(pid, signal).context(format!("Failed to send signal to process {}", pid))?;

    Ok(())
}
