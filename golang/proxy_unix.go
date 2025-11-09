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

// stopProcess sends SIGINT to gracefully stop the process on Unix
func stopProcess(proc *os.Process) error {
	return proc.Signal(syscall.SIGINT)
}
