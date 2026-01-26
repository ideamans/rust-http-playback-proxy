package httpplaybackproxy

import (
	"context"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"sync"
	"time"
)

// ProxyMode represents the mode of the proxy
type ProxyMode string

const (
	ModeRecording ProxyMode = "recording"
	ModePlayback  ProxyMode = "playback"
)

// Proxy represents a running proxy instance
type Proxy struct {
	Mode         ProxyMode
	Port         int
	InventoryDir string
	EntryURL     string     // Only for recording mode
	DeviceType   DeviceType // Only for recording mode
	cmd          *exec.Cmd
	ctx          context.Context
	cancel       context.CancelFunc
	portMutex    sync.RWMutex
}

// RecordingOptions holds options for starting a recording proxy
type RecordingOptions struct {
	EntryURL     string     // Optional: Entry URL to start recording from
	Port         int        // Optional: Port to use (0 = auto-detect)
	DeviceType   DeviceType // Optional: Device type (default: mobile)
	InventoryDir string     // Optional: Inventory directory (default: ./inventory)
}

// PlaybackOptions holds options for starting a playback proxy
type PlaybackOptions struct {
	Port         int
	InventoryDir string
}

// getAvailablePort finds an available port by binding to port 0 and releasing it
func getAvailablePort() (int, error) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, fmt.Errorf("failed to find available port: %w", err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	listener.Close()
	return port, nil
}

// waitForPort waits for a port to become available (accepting connections)
// Uses TCP connection attempts with exponential backoff
func waitForPort(port int, timeout time.Duration) error {
	startTime := time.Now()
	delay := 50 * time.Millisecond

	for time.Since(startTime) < timeout {
		conn, err := net.DialTimeout("tcp", fmt.Sprintf("127.0.0.1:%d", port), time.Second)
		if err == nil {
			conn.Close()
			return nil // Port is open
		}
		time.Sleep(delay)
		delay = time.Duration(float64(delay) * 1.5)
		if delay > 500*time.Millisecond {
			delay = 500 * time.Millisecond
		}
	}

	return fmt.Errorf("timeout waiting for port %d to become available", port)
}

// StartRecording starts a recording proxy
func StartRecording(opts RecordingOptions) (*Proxy, error) {
	if err := EnsureBinary(); err != nil {
		return nil, fmt.Errorf("failed to ensure binary: %w", err)
	}

	binaryPath, err := GetBinaryPath()
	if err != nil {
		return nil, err
	}

	// Get an available port if not specified
	port := opts.Port
	if port == 0 {
		port, err = getAvailablePort()
		if err != nil {
			return nil, err
		}
	}

	deviceType := opts.DeviceType
	if deviceType == "" {
		deviceType = DeviceTypeMobile
	}

	inventoryDir := opts.InventoryDir
	if inventoryDir == "" {
		inventoryDir = "./inventory"
	}

	// Build command
	ctx, cancel := context.WithCancel(context.Background())
	args := []string{"recording"}

	// Add entry URL if provided
	if opts.EntryURL != "" {
		args = append(args, opts.EntryURL)
	}

	// Always specify the port explicitly
	args = append(args, "--port", strconv.Itoa(port))

	// Add device type
	args = append(args, "--device", string(deviceType))

	// Add inventory directory
	args = append(args, "--inventory", inventoryDir)

	cmd := exec.CommandContext(ctx, binaryPath, args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	setProcAttributes(cmd)

	// Start the process
	if err := cmd.Start(); err != nil {
		cancel()
		return nil, fmt.Errorf("failed to start recording proxy: %w", err)
	}

	proxy := &Proxy{
		Mode:         ModeRecording,
		Port:         port,
		InventoryDir: inventoryDir,
		EntryURL:     opts.EntryURL,
		DeviceType:   deviceType,
		cmd:          cmd,
		ctx:          ctx,
		cancel:       cancel,
	}

	// Wait for the port to become available
	if err := waitForPort(port, 15*time.Second); err != nil {
		// Check if process exited early
		if !proxy.IsRunning() {
			cancel()
			return nil, fmt.Errorf("proxy process exited before port became available")
		}
		cancel()
		return nil, err
	}

	return proxy, nil
}

// StartPlayback starts a playback proxy
func StartPlayback(opts PlaybackOptions) (*Proxy, error) {
	if err := EnsureBinary(); err != nil {
		return nil, fmt.Errorf("failed to ensure binary: %w", err)
	}

	binaryPath, err := GetBinaryPath()
	if err != nil {
		return nil, err
	}

	// Get an available port if not specified
	port := opts.Port
	if port == 0 {
		port, err = getAvailablePort()
		if err != nil {
			return nil, err
		}
	}

	inventoryDir := opts.InventoryDir
	if inventoryDir == "" {
		inventoryDir = "./inventory"
	}

	// Verify inventory exists
	inventoryPath := GetInventoryPath(inventoryDir)
	if _, err := os.Stat(inventoryPath); err != nil {
		return nil, fmt.Errorf("inventory file not found at %s: %w", inventoryPath, err)
	}

	// Build command
	ctx, cancel := context.WithCancel(context.Background())
	args := []string{"playback"}

	// Always specify the port explicitly
	args = append(args, "--port", strconv.Itoa(port))

	args = append(args, "--inventory", inventoryDir)

	cmd := exec.CommandContext(ctx, binaryPath, args...)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	setProcAttributes(cmd)

	// Start the process
	if err := cmd.Start(); err != nil {
		cancel()
		return nil, fmt.Errorf("failed to start playback proxy: %w", err)
	}

	proxy := &Proxy{
		Mode:         ModePlayback,
		Port:         port,
		InventoryDir: inventoryDir,
		cmd:          cmd,
		ctx:          ctx,
		cancel:       cancel,
	}

	// Wait for the port to become available
	if err := waitForPort(port, 15*time.Second); err != nil {
		// Check if process exited early
		if !proxy.IsRunning() {
			cancel()
			return nil, fmt.Errorf("proxy process exited before port became available")
		}
		cancel()
		return nil, err
	}

	return proxy, nil
}

// Stop stops the proxy gracefully
// Sends SIGTERM (cross-platform)
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

// IsRunning checks if the proxy is still running
func (p *Proxy) IsRunning() bool {
	if p.cmd == nil || p.cmd.Process == nil {
		return false
	}

	// Use platform-specific process check (defined in proxy_unix.go and proxy_windows.go)
	return isProcessRunning(p.cmd.Process)
}

// Wait waits for the proxy to exit
func (p *Proxy) Wait() error {
	if p.cmd == nil {
		return fmt.Errorf("proxy is not running")
	}
	return p.cmd.Wait()
}

// GetInventory loads the inventory for this proxy
// This is useful after recording is complete
func (p *Proxy) GetInventory() (*Inventory, error) {
	inventoryPath := filepath.Join(p.InventoryDir, "index.json")
	return LoadInventory(inventoryPath)
}
