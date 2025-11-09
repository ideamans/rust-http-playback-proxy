package httpplaybackproxy

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"syscall"
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
	EntryURL     string // Only for recording mode
	DeviceType   DeviceType // Only for recording mode
	cmd          *exec.Cmd
	ctx          context.Context
	cancel       context.CancelFunc
}

// RecordingOptions holds options for starting a recording proxy
type RecordingOptions struct {
	EntryURL     string // Optional: Entry URL to start recording from
	Port         int    // Optional: Port to use (default: 8080, will auto-search)
	DeviceType   DeviceType // Optional: Device type (default: mobile)
	InventoryDir string // Optional: Inventory directory (default: ./inventory)
}

// PlaybackOptions holds options for starting a playback proxy
type PlaybackOptions struct {
	Port         int
	InventoryDir string
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

	// Note: Defaults are now handled in the args building section above
	// to match CLI behavior exactly

	// Build command
	ctx, cancel := context.WithCancel(context.Background())
	args := []string{"recording"}

	// Add entry URL if provided
	if opts.EntryURL != "" {
		args = append(args, opts.EntryURL)
	}

	// Add port option
	if opts.Port != 0 {
		args = append(args, "--port", strconv.Itoa(opts.Port))
	}

	// Add device type
	deviceType := opts.DeviceType
	if deviceType == "" {
		deviceType = DeviceTypeMobile
	}
	args = append(args, "--device", string(deviceType))

	// Add inventory directory
	inventoryDir := opts.InventoryDir
	if inventoryDir == "" {
		inventoryDir = "./inventory"
	}
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

	// Store the actual values used (after defaults)
	actualPort := opts.Port
	if actualPort == 0 {
		actualPort = 8080
	}
	actualInventoryDir := inventoryDir
	actualDeviceType := deviceType

	proxy := &Proxy{
		Mode:         ModeRecording,
		Port:         actualPort,
		InventoryDir: actualInventoryDir,
		EntryURL:     opts.EntryURL,
		DeviceType:   actualDeviceType,
		cmd:          cmd,
		ctx:          ctx,
		cancel:       cancel,
	}

	// Give the proxy a moment to start
	time.Sleep(500 * time.Millisecond)

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

	// Set defaults to match CLI behavior
	port := opts.Port
	if port == 0 {
		port = 8080 // Binary will auto-search from this
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

	if port != 8080 {
		args = append(args, "--port", strconv.Itoa(port))
	}

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

	// Give the proxy a moment to start
	time.Sleep(500 * time.Millisecond)

	return proxy, nil
}

// Stop stops the proxy gracefully
// For recording mode, this sends SIGINT to allow the proxy to save the inventory
func (p *Proxy) Stop() error {
	if p.cmd == nil || p.cmd.Process == nil {
		return fmt.Errorf("proxy is not running")
	}

	// Platform-specific process termination
	if err := stopProcess(p.cmd.Process); err != nil {
		// If stop fails, cancel the context
		p.cancel()
		return fmt.Errorf("failed to stop process: %w", err)
	}

	// Wait for the process to exit with a timeout
	done := make(chan error, 1)
	go func() {
		done <- p.cmd.Wait()
	}()

	select {
	case err := <-done:
		if err != nil {
			// Exit code 130 is expected for SIGINT, and some systems also return -1
			if exitErr, ok := err.(*exec.ExitError); ok {
				exitCode := exitErr.ExitCode()
				if exitCode == 130 || exitCode == -1 {
					// SIGINT is a normal way to stop the proxy
					return nil
				}
			}
			// For other signal-related errors, also treat as success
			if err.Error() == "signal: interrupt" {
				return nil
			}
			return fmt.Errorf("proxy exited with error: %w", err)
		}
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

	// Check if process is still alive
	err := p.cmd.Process.Signal(syscall.Signal(0))
	return err == nil
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
	inventoryPath := filepath.Join(p.InventoryDir, "inventory.json")
	return LoadInventory(inventoryPath)
}
