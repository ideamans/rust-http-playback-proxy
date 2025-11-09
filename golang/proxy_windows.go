//go:build windows

package httpplaybackproxy

import (
	"os/exec"
)

// setProcAttributes sets Windows-specific process attributes
func setProcAttributes(cmd *exec.Cmd) {
	// Windows doesn't support Setpgid
	// Process groups work differently on Windows
}
