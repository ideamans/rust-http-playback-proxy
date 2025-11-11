//go:build unix

package httpplaybackproxy

import (
	"os"
	"os/exec"
	"syscall"
)

// setProcAttributes sets Unix-specific process attributes
func setProcAttributes(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Setpgid: true, // Create new process group on Unix
	}
}

// stopProcess sends SIGTERM to gracefully stop the process on Unix
func stopProcess(proc *os.Process) error {
	return proc.Signal(syscall.SIGTERM)
}

// isProcessRunning checks if a process is still running on Unix
func isProcessRunning(proc *os.Process) bool {
	if proc == nil || proc.Pid == -1 {
		return false
	}

	// Signal(0) works reliably on Unix to check process existence
	err := proc.Signal(syscall.Signal(0))
	return err == nil
}
