//go:build windows

package httpplaybackproxy

import (
	"os"
	"os/exec"
	"syscall"
)

var (
	kernel32                     = syscall.NewLazyDLL("kernel32.dll")
	procGenerateConsoleCtrlEvent = kernel32.NewProc("GenerateConsoleCtrlEvent")
	procOpenProcess              = kernel32.NewProc("OpenProcess")
	procCloseHandle              = kernel32.NewProc("CloseHandle")
)

const (
	CTRL_C_EVENT        = 0
	CTRL_BREAK_EVENT    = 1
	PROCESS_QUERY_INFORMATION = 0x0400
	SYNCHRONIZE         = 0x00100000
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
	r1, _, _ := procGenerateConsoleCtrlEvent.Call(
		uintptr(CTRL_BREAK_EVENT),
		uintptr(proc.Pid),
	)

	if r1 == 0 {
		// GenerateConsoleCtrlEvent failed, fall back to Kill
		return proc.Kill()
	}

	return nil
}

// isProcessRunning checks if a process is still running on Windows
// Signal(0) doesn't work reliably on Windows, so we use OpenProcess instead
func isProcessRunning(proc *os.Process) bool {
	if proc == nil || proc.Pid == -1 {
		return false
	}

	// Try to open the process with PROCESS_QUERY_INFORMATION access
	// If successful, the process exists
	handle, _, _ := procOpenProcess.Call(
		uintptr(PROCESS_QUERY_INFORMATION),
		uintptr(0), // Don't inherit handle
		uintptr(proc.Pid),
	)

	if handle == 0 {
		// Failed to open process - it doesn't exist
		return false
	}

	// Close the handle
	procCloseHandle.Call(handle)
	return true
}
