# Signal Handling Design

This document defines the signal-based control design for HTTP Playback Proxy, following Unix daemon best practices while maintaining Windows compatibility.

## Design Philosophy

**Follow standard daemon signal handling:**
- Use Unix signals as the primary control mechanism (SIGTERM/SIGINT for shutdown)
- Provide clean cross-platform abstraction
- No HTTP Control API - signals only

**Goals:**
1. Standard Unix daemon behavior (compatible with systemd, Docker, Kubernetes)
2. Programmatic control from scripts and wrappers (kill, systemctl, docker stop)
3. Windows compatibility with semantic equivalents
4. Simple and reliable shutdown mechanism

## Signal Mapping

### Recording Mode

| Purpose | Unix | Windows | Behavior |
|---------|------|---------|----------|
| **Interactive stop** | SIGINT (Ctrl+C) | CTRL_C_EVENT | Graceful shutdown, save inventory |
| **Programmatic stop** | SIGTERM | CTRL_BREAK_EVENT | Graceful shutdown, save inventory |
| Force kill | SIGKILL | Task kill | Immediate termination, **no inventory save** |

**Node.js Limitation:**
Node.js's `process.kill()` does not support CTRL_BREAK_EVENT on Windows. The TypeScript wrapper uses SIGINT (CTRL_C_EVENT) instead for Windows programmatic shutdown. The Rust binary accepts both CTRL_C and CTRL_BREAK, so this limitation only affects the Node.js wrapper behavior.

**CLI:**
```bash
# Start recording
http-playback-proxy recording https://example.com --port 8080

# Stop (from terminal)
^C  # SIGINT

# Stop (from script/wrapper)
kill $PID              # SIGTERM (Unix default)
kill -TERM $PID        # Explicit SIGTERM
docker stop container  # Sends SIGTERM
systemctl stop service # Sends SIGTERM
```

### Playback Mode

| Purpose | Unix | Windows | Behavior |
|---------|------|---------|----------|
| **Interactive stop** | SIGINT (Ctrl+C) | CTRL_C_EVENT | Graceful shutdown |
| **Programmatic stop** | SIGTERM | CTRL_BREAK_EVENT | Graceful shutdown |
| Force kill | SIGKILL | Task kill | Immediate termination |

**Node.js Limitation:**
Node.js's `process.kill()` does not support CTRL_BREAK_EVENT on Windows. The TypeScript wrapper uses SIGINT (CTRL_C_EVENT) instead for Windows programmatic shutdown. The Rust binary accepts both CTRL_C and CTRL_BREAK, so this limitation only affects the Node.js wrapper behavior.

**CLI:**
```bash
# Start playback
http-playback-proxy playback --port 8080

# Stop
^C              # SIGINT (interactive)
kill $PID       # SIGTERM (programmatic)
```

## Signal Subcommand (Internal Helper)

The `signal` subcommand is an internal helper for sending signals across platforms.

**Usage:**
```bash
# Send CTRL_BREAK (Windows) / SIGTERM (Unix)
http-playback-proxy signal --pid <PID> --kind ctrl-break

# Send CTRL_C (Windows) / SIGINT (Unix)
http-playback-proxy signal --pid <PID> --kind ctrl-c

# Send SIGTERM (Unix) / CTRL_BREAK (Windows)
http-playback-proxy signal --pid <PID> --kind term

# Send SIGINT (Unix) / CTRL_C (Windows)
http-playback-proxy signal --pid <PID> --kind int
```

**Why This Exists:**

On Unix, this subcommand provides a simple way to send signals using `kill(2)`.

On Windows, this was originally intended to send console control events using native APIs (`AttachConsole`, `GenerateConsoleCtrlEvent`), but this approach has limitations: `AttachConsole` can only attach to processes in the same console session, which fails when called from Node.js child processes or different console sessions.

**Current Limitation:**

Due to Windows console session restrictions, the signal subcommand cannot reliably send signals to child processes on Windows. Language wrappers should use `process.kill('SIGINT')` on Windows instead, which Node.js converts to CTRL_C_EVENT.

**Internal Use Only:**

This subcommand is hidden from the main help (`--help`) and is documented here for maintainers and wrapper developers.

## Implementation Details

### Rust Signal Handler

**Location:** `src/recording/signal_handler.rs`, `src/playback/signal_handler.rs`

#### Recording Mode

```rust
#[cfg(unix)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT, shutting down gracefully...");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down gracefully...");
            Ok(())
        }
    }
}

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
            info!("Received CTRL_BREAK (SIGTERM equivalent), shutting down gracefully...");
            Ok(())
        }
    }
}
```

#### Playback Mode

Identical to recording mode - shutdown signals only:

```rust
#[cfg(unix)]
pub async fn wait_for_shutdown_signal() -> Result<(), std::io::Error> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT, shutting down gracefully...");
            Ok(())
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down gracefully...");
            Ok(())
        }
    }
}

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
            info!("Received CTRL_BREAK (SIGTERM equivalent), shutting down gracefully...");
            Ok(())
        }
    }
}
```

### Proxy Integration

#### Recording Mode

```rust
pub async fn start_recording_proxy(
    port: u16,
    inventory: Inventory,
    inventory_dir: PathBuf,
) -> Result<()> {
    // ... setup proxy ...

    // Signal handler task
    let shutdown_handler = tokio::spawn(async move {
        // Wait for shutdown signal (SIGINT/SIGTERM)
        if let Err(e) = wait_for_shutdown_signal().await {
            error!("Signal handler error: {}", e);
        }

        // Graceful shutdown sequence
        info!("Shutting down, waiting for in-flight requests...");
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Save inventory
        let mut inventory = handler_inventory.lock().await;
        batch_processor.process_all(&mut inventory).await?;
        save_inventory(&inventory, &inventory_dir).await?;

        info!("Inventory saved, shutdown complete");
    });

    // Run proxy with shutdown
    tokio::select! {
        result = proxy.start() => { /* ... */ }
        _ = shutdown_handler => { /* ... */ }
    }

    Ok(())
}
```

#### Playback Mode

```rust
pub async fn start_playback_proxy<F: FileSystem + 'static>(
    port: u16,
    transactions: Vec<Transaction>,
) -> Result<()> {
    // ... setup proxy ...

    let shared_transactions = Arc::new(RwLock::new(Arc::new(transactions)));

    // Signal handler task
    let signal_handler = tokio::spawn(async move {
        if let Err(e) = wait_for_shutdown_signal().await {
            error!("Signal handler error: {}", e);
        }
        info!("Received shutdown signal (SIGTERM/SIGINT)");
        let _ = signal_shutdown_tx.send(());
    });

    // Run proxy with shutdown
    tokio::select! {
        result = proxy.start() => { /* ... */ }
        _ = signal_handler => { /* ... */ }
    }

    Ok(())
}
```

### TypeScript Wrapper

**Location:** `typescript/src/proxy.ts`

```typescript
export class Proxy {
  async stop(): Promise<void> {
    if (!this.process) {
      throw new Error('Proxy is not running');
    }

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.process?.kill('SIGKILL');
        reject(new Error('Proxy did not stop gracefully, killed forcefully'));
      }, 10000);

      this.process.once('exit', (code, signal) => {
        clearTimeout(timeout);
        if (code === 0 || code === null || signal === 'SIGTERM' || signal === 'SIGINT') {
          resolve();
        } else {
          reject(new Error(`Proxy exited with code ${code} signal ${signal}`));
        }
      });

      // Send platform-appropriate signal:
      // Unix: SIGTERM (standard kill signal)
      // Windows: SIGINT (CTRL_C_EVENT) - Node.js limitation, cannot send CTRL_BREAK
      try {
        if (process.platform === 'win32') {
          // On Windows, use SIGINT which Node.js converts to CTRL_C_EVENT
          // Node.js cannot send CTRL_BREAK_EVENT, and the signal subcommand
          // cannot attach to processes in different console sessions
          this.process.kill('SIGINT');
        } else {
          // On Unix, use standard SIGTERM
          this.process.kill('SIGTERM');
        }
      } catch (err) {
        reject(err);
      }
    });
  }
}

export async function startRecording(options: RecordingOptions): Promise<Proxy> {
  // ... setup ...

  const args: string[] = ['recording'];
  if (options.entryUrl) args.push(options.entryUrl);
  if (options.port) args.push('--port', options.port.toString());
  args.push('--device', options.deviceType || 'mobile');
  args.push('--inventory', options.inventoryDir || './inventory');

  const proc = spawn(binaryPath, args, spawnOptions);
  const proxy = new Proxy('recording', port, inventoryDir, options.entryUrl, deviceType);
  proxy.setProcess(proc);

  return proxy;
}

export async function startPlayback(options: PlaybackOptions): Promise<Proxy> {
  // ... setup ...

  const args: string[] = ['playback'];
  if (options.port) args.push('--port', options.port.toString());
  args.push('--inventory', options.inventoryDir || './inventory');

  const proc = spawn(binaryPath, args, spawnOptions);
  const proxy = new Proxy('playback', port, inventoryDir);
  proxy.setProcess(proc);

  return proxy;
}
```

### Go Wrapper

**Location:** `golang/proxy.go`

```go
package proxy

import (
    "context"
    "errors"
    "fmt"
    "os"
    "os/exec"
    "runtime"
    "syscall"
    "time"
)

func (p *Proxy) Stop() error {
    if p.cmd == nil || p.cmd.Process == nil {
        return fmt.Errorf("proxy is not running")
    }

    // Platform-specific process termination (SIGTERM preferred, SIGINT fallback)
    if err := stopProcess(p.cmd.Process); err != nil {
        // If stop fails, cancel the context
        p.cancel()
        return fmt.Errorf("failed to stop process: %w", err)
    }

    return p.waitForExit()
}

// waitForExit waits for the process to exit with proper error handling
func (p *Proxy) waitForExit() error {
    done := make(chan error, 1)
    go func() {
        done <- p.cmd.Wait()
    }()

    select {
    case err := <-done:
        if err != nil {
            // Exit code 130 is expected for SIGINT, -1 for signals, 0 for success
            if exitErr, ok := err.(*exec.ExitError); ok {
                exitCode := exitErr.ExitCode()
                // Windows: 0xc000013a (STATUS_CONTROL_C_EXIT) = 3221225786 or -1073741510
                // Unix: 130 (128 + SIGINT=2) or -1 for signals
                if exitCode == 0 || exitCode == 130 || exitCode == -1 ||
                    exitCode == 3221225786 || exitCode == -1073741510 {
                    // Normal exit codes for graceful shutdown
                    return nil
                }
            }
            // For other signal-related errors, also treat as success
            if err.Error() == "signal: interrupt" {
                return nil
            }
            return fmt.Errorf("proxy exited with error: %w", err)
        }
        // Exit code 0 - success
        return nil
    case <-time.After(10 * time.Second):
        // Force kill if graceful shutdown takes too long
        p.cancel()
        _ = p.cmd.Process.Kill()
        return fmt.Errorf("proxy did not stop gracefully, killed forcefully")
    }
}

func StartRecording(opts RecordingOptions) (*Proxy, error) {
    // ... binary setup ...

    args := []string{"recording"}
    if opts.EntryURL != "" {
        args = append(args, opts.EntryURL)
    }
    if opts.Port > 0 {
        args = append(args, "--port", fmt.Sprintf("%d", opts.Port))
    }
    args = append(args, "--device", string(opts.DeviceType))
    args = append(args, "--inventory", opts.InventoryDir)

    cmd := exec.Command(binaryPath, args...)
    // ... start process ...

    return &Proxy{
        mode:         "recording",
        port:         actualPort,
        inventoryDir: opts.InventoryDir,
        cmd:          cmd,
    }, nil
}

func StartPlayback(opts PlaybackOptions) (*Proxy, error) {
    // ... binary setup ...

    args := []string{"playback"}
    if opts.Port > 0 {
        args = append(args, "--port", fmt.Sprintf("%d", opts.Port))
    }
    args = append(args, "--inventory", opts.InventoryDir)

    cmd := exec.Command(binaryPath, args...)
    // ... start process ...

    return &Proxy{
        mode:         "playback",
        port:         actualPort,
        inventoryDir: opts.InventoryDir,
        cmd:          cmd,
    }, nil
}
```

## Benefits

1. **Standard Unix behavior** - Works with systemd, Docker, Kubernetes out-of-box
2. **Simplified API** - Signal-based only, no HTTP Control API
3. **Better ergonomics** - `kill $PID` just works
4. **Testable** - Easy to send signals in integration tests
5. **Cross-platform** - Consistent shutdown behavior across platforms

## Testing

### Unix Testing

```bash
# Start recording
./http-playback-proxy recording https://example.com --port 8080 &
PID=$!

# Make some requests...

# Stop with SIGTERM (programmatic)
kill $PID
# or
kill -TERM $PID

# Start playback
./http-playback-proxy playback --port 8080 &
PID=$!

# Stop
kill $PID
```

### Windows Testing

```powershell
# Start recording
Start-Process http-playback-proxy.exe -ArgumentList "recording","https://example.com","--port","8080"
$proc = Get-Process http-playback-proxy

# Make some requests...

# Stop (CTRL_BREAK equivalent)
$proc | Stop-Process

# Start playback
Start-Process http-playback-proxy.exe -ArgumentList "playback","--port","8080"
$proc = Get-Process http-playback-proxy

# Stop
$proc | Stop-Process
```

### Integration Test Pattern

```rust
#[tokio::test]
async fn test_recording_sigterm_shutdown() {
    let mut cmd = Command::new(get_binary_path())
        .args(&["recording", "https://example.com", "--port", "0"])
        .spawn()
        .unwrap();

    // Wait for startup
    tokio::time::sleep(Duration::from_secs(1)).await;

    #[cfg(unix)]
    {
        // Send SIGTERM
        unsafe {
            libc::kill(cmd.id() as i32, libc::SIGTERM);
        }
    }

    #[cfg(windows)]
    {
        // Send CTRL_BREAK (SIGTERM equivalent)
        cmd.kill().unwrap();
    }

    // Wait for graceful shutdown
    let result = tokio::time::timeout(Duration::from_secs(5), cmd.wait()).await;
    assert!(result.is_ok());

    // Verify inventory was saved
    assert!(Path::new("./inventory/index.json").exists());
}
```

## References

- [systemd.service(5)](https://www.freedesktop.org/software/systemd/man/systemd.service.html)
- [tokio::signal](https://docs.rs/tokio/latest/tokio/signal/index.html)
- [Unix Signals (signal(7))](https://man7.org/linux/man-pages/man7/signal.7.html)
- [Windows Console Events](https://docs.microsoft.com/en-us/windows/console/handlerroutine)
