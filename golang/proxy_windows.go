//go:build windows

package httpplaybackproxy

import (
	"os"
	"os/exec"
	"syscall"
)

var (
	kernel32                  = syscall.NewLazyDLL("kernel32.dll")
	procGenerateConsoleCtrlEvent = kernel32.NewProc("GenerateConsoleCtrlEvent")
)

const (
	CTRL_C_EVENT        = 0
	CTRL_BREAK_EVENT    = 1
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
	if proc == nil {
		return nil
	}

	// If the process is already done, that's success
	if proc.Pid == -1 {
		return nil
	}

	// Send CTRL_BREAK_EVENT to the process group
	// This is the Windows equivalent of SIGINT for graceful shutdown
	r1, _, err := procGenerateConsoleCtrlEvent.Call(
		uintptr(CTRL_BREAK_EVENT),
		uintptr(proc.Pid),
	)

	if r1 == 0 {
		// GenerateConsoleCtrlEvent failed, fall back to Kill
		return proc.Kill()
	}

	return nil
}
