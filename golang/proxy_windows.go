//go:build windows

package httpplaybackproxy

import (
	"os"
	"os/exec"
	"syscall"
)

// setProcAttributes sets Windows-specific process attributes
func setProcAttributes(cmd *exec.Cmd) {
	// Create a new process group on Windows
	// This is required to send console control events (Ctrl+C/Ctrl+Break)
	cmd.SysProcAttr = &syscall.SysProcAttr{
		CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP,
	}
}

// stopProcess sends Ctrl+Break event on Windows for graceful shutdown
func stopProcess(proc *os.Process) error {
	// Try to send os.Interrupt (maps to CTRL_BREAK_EVENT on Windows)
	// This allows the Rust process to run its Ctrl+C handler
	err := proc.Signal(os.Interrupt)

	// If the process is already done, that's success
	if err == os.ErrProcessDone {
		return nil
	}

	// If Signal fails (headless console), fall back to Kill as safety net
	if err != nil {
		return proc.Kill()
	}

	return nil
}
