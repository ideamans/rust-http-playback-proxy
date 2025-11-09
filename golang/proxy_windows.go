//go:build windows

package httpplaybackproxy

import (
	"os"
	"os/exec"
)

// setProcAttributes sets Windows-specific process attributes
func setProcAttributes(cmd *exec.Cmd) {
	// Windows doesn't support Setpgid
	// Process groups work differently on Windows
}

// stopProcess terminates the process on Windows
// Windows doesn't support SIGINT, so we use Kill() which triggers
// the process's cleanup handlers (including Ctrl+C handler if registered)
func stopProcess(proc *os.Process) error {
	// On Windows, Kill sends a termination signal that allows
	// graceful shutdown if the process has registered handlers
	return proc.Kill()
}
