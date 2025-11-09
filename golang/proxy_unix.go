//go:build unix

package httpplaybackproxy

import (
	"os/exec"
	"syscall"
)

// setProcAttributes sets Unix-specific process attributes
func setProcAttributes(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Setpgid: true, // Create new process group on Unix
	}
}
