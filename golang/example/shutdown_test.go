package example

import (
	"fmt"
	"testing"
	"time"

	proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

// TestSignalShutdown tests the signal-based shutdown functionality
func TestSignalShutdown(t *testing.T) {
	// Test with recording proxy (uses signal-based shutdown)
	t.Run("RecordingProxySignalShutdown", func(t *testing.T) {
		tmpDir := t.TempDir()

		p, err := proxy.StartRecording(proxy.RecordingOptions{
			Port:         0, // Auto-assign port
			InventoryDir: tmpDir,
			// No ControlPort - uses signal-based shutdown (SIGTERM)
		})
		if err != nil {
			t.Fatalf("Failed to start recording proxy: %v", err)
		}

		// Give it time to start
		time.Sleep(2 * time.Second)

		if !p.IsRunning() {
			t.Fatal("Proxy should be running")
		}

		fmt.Printf("Recording proxy started on port %d\n", p.Port)

		// Stop using signal-based shutdown (SIGTERM on Unix, CTRL_BREAK on Windows)
		if err := p.Stop(); err != nil {
			t.Fatalf("Failed to stop proxy via signal: %v", err)
		}

		// Verify it stopped
		time.Sleep(1 * time.Second)
		if p.IsRunning() {
			t.Fatal("Proxy should have stopped")
		}

		fmt.Println("Recording proxy stopped successfully via signal shutdown")
	})

	// Test with playback proxy (needs inventory)
	// This test is commented out as it requires a valid inventory
	/*
		t.Run("PlaybackProxySignalShutdown", func(t *testing.T) {
			tmpDir := t.TempDir()

			// Create a minimal inventory
			inventoryPath := filepath.Join(tmpDir, "index.json")
			inventory := proxy.Inventory{
				Resources: []proxy.Resource{},
			}
			if err := proxy.SaveInventory(inventoryPath, &inventory); err != nil {
				t.Fatalf("Failed to create inventory: %v", err)
			}

			p, err := proxy.StartPlayback(proxy.PlaybackOptions{
				Port:         0, // Auto-assign port
				InventoryDir: tmpDir,
				// No ControlPort - uses signal-based shutdown (SIGTERM)
			})
			if err != nil {
				t.Fatalf("Failed to start playback proxy: %v", err)
			}

			// Give it time to start
			time.Sleep(2 * time.Second)

			if !p.IsRunning() {
				t.Fatal("Proxy should be running")
			}

			fmt.Printf("Playback proxy started on port %d\n", p.Port)

			// Stop using signal-based shutdown (SIGTERM on Unix, CTRL_BREAK on Windows)
			if err := p.Stop(); err != nil {
				t.Fatalf("Failed to stop proxy via signal: %v", err)
			}

			// Verify it stopped
			time.Sleep(1 * time.Second)
			if p.IsRunning() {
				t.Fatal("Proxy should have stopped")
			}

			fmt.Println("Playback proxy stopped successfully via signal shutdown")
		})
	*/
}
