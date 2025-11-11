# Windows Signal Handling Challenges

This document describes the ongoing challenges with implementing graceful shutdown on Windows, specifically for TypeScript wrapper integration.

## Problem Overview

On Windows, sending console control events (CTRL_C, CTRL_BREAK) to child processes for graceful shutdown is significantly more complex than Unix signal handling. The TypeScript wrapper needs to trigger graceful shutdown of the Rust proxy process, which should save inventory before exiting.

## Signal Handling Design

### Unix vs Windows Equivalents

| Purpose | Unix | Windows | Behavior |
|---------|------|---------|----------|
| **Interactive stop** | SIGINT (Ctrl+C) | CTRL_C_EVENT | Graceful shutdown, save inventory |
| **Programmatic stop** | SIGTERM | CTRL_BREAK_EVENT | Graceful shutdown, save inventory |
| Force kill | SIGKILL | Task kill | Immediate termination, **no inventory save** |

### Node.js Limitations

Node.js's `process.kill()` has critical limitations on Windows:

- `process.kill('SIGTERM')` → Performs **force kill (taskkill)**, not graceful shutdown
- `process.kill('SIGINT')` → Also does **not reliably** trigger console control events
- No direct way to send CTRL_BREAK_EVENT from Node.js

## Attempted Solutions

### Approach 1: Node.js `process.kill('SIGINT')` (Failed)

**Attempt:** Use Node.js built-in `process.kill('SIGINT')` on Windows.

**Result:** Inventory file was not saved. Node.js does not properly deliver CTRL_C_EVENT to child processes.

```typescript
// Does NOT work reliably on Windows
if (process.platform === 'win32') {
  this.process.kill('SIGINT');
}
```

### Approach 2: Signal Subcommand with AttachConsole (Partially Failed)

**Attempt:** Create a `signal` subcommand that uses Windows native APIs to send console control events.

**Implementation:**
```rust
// src/signal_sender.rs
#[cfg(windows)]
fn send_signal_windows(pid: u32, kind: SignalKind) -> Result<()> {
    unsafe {
        // 1. FreeConsole() - Detach from current console
        FreeConsole();

        // 2. AttachConsole(pid) - Attach to target process console
        AttachConsole(pid);

        // 3. SetConsoleCtrlHandler - Set up handler to ignore CTRL events
        SetConsoleCtrlHandler(handler, TRUE);

        // 4. GenerateConsoleCtrlEvent - Send CTRL_BREAK or CTRL_C
        GenerateConsoleCtrlEvent(event, 0);

        // 5. Cleanup
        FreeConsole();
    }
}
```

**Issues Encountered:**

#### Issue 1: Access Denied (SOLVED)
- **Error:** `AttachConsole` failed with ERROR_ACCESS_DENIED
- **Cause:** Calling process was already attached to a console
- **Solution:** Call `FreeConsole()` first to detach from current console

#### Issue 2: Process Hangs (CURRENT ISSUE)
- **Error:** After sending signal, the test process hangs with "Terminate batch job (Y/N)?" prompt
- **Cause:** `GenerateConsoleCtrlEvent(event, 0)` sends the event to **all processes in the console session**, including the signal sender itself
- **Current Status:** Even with `SetConsoleCtrlHandler` to ignore events, the signal subcommand appears to receive the CTRL event and hangs

**CI Test Output:**
```
# Stopping recording proxy...
Terminate batch job (Y/N)?
Entering debug mode. Use h or ? for help.
```

### Windows API Constraints

The fundamental issue with `GenerateConsoleCtrlEvent` on Windows:

1. **Console Session Scope:** Events are sent to all processes sharing the same console session
2. **Process Group Limitation:** The second parameter (process group ID) has limited utility:
   - `0` = current process group (includes sender)
   - Cannot specify arbitrary process group of another process
3. **Handler Complexity:** Even with `SetConsoleCtrlHandler`, the signal sender may still receive the event

## Current Implementation

### Rust Signal Handler (WORKING)

The Rust binary correctly handles both CTRL_C and CTRL_BREAK events:

```rust
// src/recording/signal_handler.rs
#[cfg(windows)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::windows;

    let mut ctrl_c = windows::ctrl_c()?;
    let mut ctrl_break = windows::ctrl_break()?;

    tokio::select! {
        _ = ctrl_c.recv() => {
            info!("Received CTRL_C, shutting down gracefully...");
            Ok(())
        }
        _ = ctrl_break.recv() => {
            info!("Received CTRL_BREAK, shutting down gracefully...");
            Ok(())
        }
    }
}
```

When the proxy receives CTRL_C or CTRL_BREAK, it:
1. Stops accepting new requests
2. Waits for in-flight requests to complete
3. Saves inventory to disk
4. Exits gracefully

### TypeScript Wrapper (PROBLEMATIC)

```typescript
// typescript/src/proxy.ts
async stop(): Promise<void> {
  if (process.platform === 'win32') {
    // Use signal subcommand to send CTRL_BREAK
    const binaryPath = getFullBinaryPath();
    const { spawnSync } = require('child_process');
    const result = spawnSync(
      binaryPath,
      ['signal', '--pid', this.process.pid.toString(), '--kind', 'ctrl-break'],
      { stdio: 'pipe' }
    );
    // ... error handling ...
  } else {
    this.process.kill('SIGTERM');
  }
}
```

## Alternative Approaches to Consider

### Option 1: CREATE_NEW_CONSOLE Flag

Spawn the proxy process with `CREATE_NEW_CONSOLE` flag to isolate it in its own console session.

**Pros:**
- Signal subcommand won't share the same console
- `GenerateConsoleCtrlEvent` won't affect the sender

**Cons:**
- Creates a visible console window on Windows
- May not be suitable for background/CI environments
- TypeScript `child_process.spawn()` has limited support for this flag

### Option 2: Control Socket/Named Pipe

Instead of console control events, use a named pipe or local socket for control commands.

**Pros:**
- Reliable cross-process communication
- No console session limitations
- Works in all environments (CI, background, GUI)

**Cons:**
- Deviates from signal-based design philosophy
- Requires additional control server in Rust
- More complex implementation

### Option 3: WM_CLOSE Message

Send `WM_CLOSE` message to the process's main window (if it has one).

**Pros:**
- Standard Windows shutdown mechanism

**Cons:**
- Requires the process to have a message loop
- Console applications may not have a window
- May not trigger custom shutdown logic

### Option 4: Custom Handler with Thread Safety

Implement a more sophisticated handler that uses thread-local storage or atomic flags to prevent the signal sender from being affected.

**Pros:**
- Keeps signal-based approach
- Works within console session constraints

**Cons:**
- Complex implementation
- May still have edge cases

## Test Environment

### CI Configuration (GitHub Actions Windows)

```yaml
- name: Test TypeScript acceptance
  run: |
    cd acceptance/typescript
    npm test
```

The test spawns:
1. HTTP test server (Node.js)
2. Recording proxy (Rust binary)
3. Makes HTTP requests through proxy
4. Calls `proxy.stop()` to trigger graceful shutdown
5. Verifies inventory file was saved

### Expected Behavior

After `proxy.stop()`:
1. Signal subcommand executes
2. CTRL_BREAK event is delivered to proxy process
3. Proxy saves inventory and exits with code 0
4. Signal subcommand exits with code 0
5. Test continues to validate inventory

### Actual Behavior (Current)

After `proxy.stop()`:
1. Signal subcommand executes
2. CTRL_BREAK event is sent
3. **Process hangs with "Terminate batch job (Y/N)?" prompt**
4. Test times out and fails

## Rust Implementation Details

### Signal Subcommand Code

```rust
// src/signal_sender.rs
#[cfg(windows)]
fn send_signal_windows(pid: u32, kind: SignalKind) -> Result<()> {
    use windows_sys::Win32::Foundation::{BOOL, FALSE, TRUE};
    use windows_sys::Win32::System::Console::{
        AttachConsole, CTRL_BREAK_EVENT, CTRL_C_EVENT, FreeConsole,
        GenerateConsoleCtrlEvent, PHANDLER_ROUTINE, SetConsoleCtrlHandler,
    };

    let event = match kind {
        SignalKind::CtrlBreak | SignalKind::Term => CTRL_BREAK_EVENT,
        SignalKind::CtrlC | SignalKind::Int => CTRL_C_EVENT,
    };

    // Handler function to ignore CTRL events
    unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> BOOL {
        TRUE // Return TRUE to indicate handled (ignored)
    }

    unsafe {
        // 1. Detach from current console
        FreeConsole();

        // 2. Attach to target process console
        if AttachConsole(pid) == 0 {
            return Err(anyhow::anyhow!("Failed to attach to console"));
        }

        // 3. Install handler to ignore CTRL events
        let handler: PHANDLER_ROUTINE = Some(ctrl_handler);
        SetConsoleCtrlHandler(handler, TRUE);

        // 4. Send CTRL event
        let result = GenerateConsoleCtrlEvent(event, 0);

        // 5. Wait for delivery
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 6. Cleanup
        SetConsoleCtrlHandler(handler, FALSE);
        FreeConsole();

        if result == 0 {
            return Err(anyhow::anyhow!("Failed to generate console ctrl event"));
        }
    }

    Ok(())
}
```

## Dependencies

```toml
# Cargo.toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = ["Win32_Foundation", "Win32_System_Console"] }
```

## References

- [Windows Console Control Events](https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent)
- [SetConsoleCtrlHandler](https://learn.microsoft.com/en-us/windows/console/setconsolectrlhandler)
- [AttachConsole](https://learn.microsoft.com/en-us/windows/console/attachconsole)
- [FreeConsole](https://learn.microsoft.com/en-us/windows/console/freeconsole)
- [Node.js process.kill() Windows behavior](https://nodejs.org/api/process.html#process_process_kill_pid_signal)

## Status

**Current Status:** BLOCKED - Signal subcommand causes process hang on Windows

**Next Steps to Investigate:**

1. Test if removing `Sleep(100)` prevents hang
2. Try `CREATE_PROCESS_GROUP` flag when spawning proxy
3. Consider implementing control socket approach as fallback
4. Research if other projects have solved this problem

## Related Files

- `src/signal_sender.rs` - Signal subcommand implementation
- `src/cli.rs` - CLI definition for signal subcommand
- `typescript/src/proxy.ts` - TypeScript wrapper using signal subcommand
- `SIGNAL.md` - Overall signal handling design document
- `acceptance/typescript/test.js` - Integration test that currently fails on Windows
