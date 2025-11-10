package httpplaybackproxy

import (
	"bufio"
	"context"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"sync"
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
	ControlPort  *int // Optional control/management API port
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
	Port         int        // Optional: Port to use (default: 18080, will auto-search)
	DeviceType   DeviceType // Optional: Device type (default: mobile)
	InventoryDir string     // Optional: Inventory directory (default: ./inventory)
	ControlPort  *int       // Optional: Control/management API port (enables HTTP shutdown)
}

// PlaybackOptions holds options for starting a playback proxy
type PlaybackOptions struct {
	Port         int
	InventoryDir string
	ControlPort  *int // Optional: Control/management API port (enables HTTP shutdown)
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

	// Add control port if specified
	if opts.ControlPort != nil {
		args = append(args, "--control-port", strconv.Itoa(*opts.ControlPort))
	}

	cmd := exec.CommandContext(ctx, binaryPath, args...)

	// Capture stdout to extract actual port number
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create stdout pipe: %w", err)
	}
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
		actualPort = 18080 // Default fallback
	}
	actualInventoryDir := inventoryDir
	actualDeviceType := deviceType

	proxy := &Proxy{
		Mode:         ModeRecording,
		Port:         actualPort,
		ControlPort:  opts.ControlPort,
		InventoryDir: actualInventoryDir,
		EntryURL:     opts.EntryURL,
		DeviceType:   actualDeviceType,
		cmd:          cmd,
		ctx:          ctx,
		cancel:       cancel,
	}

	// Read stdout to find actual port number and forward output
	portChan := make(chan int, 1)
	go func() {
		scanner := bufio.NewScanner(stdout)
		// Regex that matches both "HTTPS MITM Proxy" and "Playback proxy"
		portRegex := regexp.MustCompile(`(?:HTTPS MITM |Playback |Recording )?[Pp]roxy listening on (?:127\.0\.0\.1|0\.0\.0\.0):(\d+)`)
		portFound := false
		for scanner.Scan() {
			line := scanner.Text()
			fmt.Println(line) // Forward to stdout

			// Extract port number from output
			if !portFound {
				if matches := portRegex.FindStringSubmatch(line); len(matches) > 1 {
					if port, err := strconv.Atoi(matches[1]); err == nil {
						portChan <- port
						portFound = true
					}
				}
			}
		}
		if !portFound {
			close(portChan) // Signal that no port was found
		}
	}()

	// Wait for actual port number (with timeout)
	select {
	case port := <-portChan:
		proxy.Port = port
	case <-time.After(5 * time.Second):
		// Timeout - use default port
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

	// Set defaults to match CLI behavior
	port := opts.Port
	if port == 0 {
		port = 18080 // Binary will auto-search from this
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

	if port != 18080 {
		args = append(args, "--port", strconv.Itoa(port))
	}

	args = append(args, "--inventory", inventoryDir)

	// Add control port if specified
	if opts.ControlPort != nil {
		args = append(args, "--control-port", strconv.Itoa(*opts.ControlPort))
	}

	cmd := exec.CommandContext(ctx, binaryPath, args...)

	// Capture stdout to extract actual port number
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create stdout pipe: %w", err)
	}
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
		ControlPort:  opts.ControlPort,
		InventoryDir: inventoryDir,
		cmd:          cmd,
		ctx:          ctx,
		cancel:       cancel,
	}

	// Read stdout to find actual port number and forward output
	portChan := make(chan int, 1)
	go func() {
		scanner := bufio.NewScanner(stdout)
		// Regex that matches both "HTTPS MITM Proxy" and "Playback proxy"
		portRegex := regexp.MustCompile(`(?:HTTPS MITM |Playback |Recording )?[Pp]roxy listening on (?:127\.0\.0\.1|0\.0\.0\.0):(\d+)`)
		portFound := false
		for scanner.Scan() {
			line := scanner.Text()
			fmt.Println(line) // Forward to stdout

			// Extract port number from output
			if !portFound {
				if matches := portRegex.FindStringSubmatch(line); len(matches) > 1 {
					if port, err := strconv.Atoi(matches[1]); err == nil {
						portChan <- port
						portFound = true
					}
				}
			}
		}
		if !portFound {
			close(portChan) // Signal that no port was found
		}
	}()

	// Wait for actual port number (with timeout)
	select {
	case port := <-portChan:
		proxy.Port = port
	case <-time.After(5 * time.Second):
		// Timeout - use default port
	}

	return proxy, nil
}

// Stop stops the proxy gracefully
// If ControlPort is set, sends HTTP shutdown request
// Otherwise sends SIGINT/SIGTERM (platform-specific)
func (p *Proxy) Stop() error {
	if p.cmd == nil || p.cmd.Process == nil {
		return fmt.Errorf("proxy is not running")
	}

	// Try HTTP shutdown if control port is available
	if p.ControlPort != nil {
		if err := p.httpShutdown(); err != nil {
			// If HTTP shutdown fails, fall back to signal-based shutdown
			fmt.Printf("HTTP shutdown failed: %v, falling back to signal\n", err)
		} else {
			// HTTP shutdown successful, wait for process to exit
			return p.waitForExit()
		}
	}

	// Platform-specific process termination (SIGINT/SIGTERM)
	if err := stopProcess(p.cmd.Process); err != nil {
		// If stop fails, cancel the context
		p.cancel()
		return fmt.Errorf("failed to stop process: %w", err)
	}

	return p.waitForExit()
}

// httpShutdown sends an HTTP shutdown request to the control API
func (p *Proxy) httpShutdown() error {
	if p.ControlPort == nil {
		return fmt.Errorf("no control port configured")
	}

	url := fmt.Sprintf("http://127.0.0.1:%d/_shutdown", *p.ControlPort)
	req, err := http.NewRequest("POST", url, nil)
	if err != nil {
		return fmt.Errorf("failed to create shutdown request: %w", err)
	}

	client := &http.Client{Timeout: 5 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("failed to send shutdown request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("shutdown request failed with status: %d", resp.StatusCode)
	}

	return nil
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
