package example

import (
	"fmt"
	"testing"
	"time"

	proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

// TestHTTPShutdown tests the HTTP shutdown functionality
func TestHTTPShutdown(t *testing.T) {
	// Test with recording proxy
	t.Run("RecordingProxyHTTPShutdown", func(t *testing.T) {
		tmpDir := t.TempDir()
		controlPort := 19081

		p, err := proxy.StartRecording(proxy.RecordingOptions{
			Port:         0, // Auto-assign port
			InventoryDir: tmpDir,
			ControlPort:  &controlPort,
		})
		if err != nil {
			t.Fatalf("Failed to start recording proxy: %v", err)
		}

		// Give it time to start
		time.Sleep(2 * time.Second)

		if !p.IsRunning() {
			t.Fatal("Proxy should be running")
		}

		fmt.Printf("Recording proxy started on port %d, control port %d\n", p.Port, *p.ControlPort)

		// Stop using HTTP shutdown
		if err := p.Stop(); err != nil {
			t.Fatalf("Failed to stop proxy via HTTP: %v", err)
		}

		// Verify it stopped
		time.Sleep(1 * time.Second)
		if p.IsRunning() {
			t.Fatal("Proxy should have stopped")
		}

		fmt.Println("Recording proxy stopped successfully via HTTP shutdown")
	})

	// Test with playback proxy (needs inventory)
	// This test is commented out as it requires a valid inventory
	/*
		t.Run("PlaybackProxyHTTPShutdown", func(t *testing.T) {
			tmpDir := t.TempDir()
			controlPort := 19082

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
				ControlPort:  &controlPort,
			})
			if err != nil {
				t.Fatalf("Failed to start playback proxy: %v", err)
			}

			// Give it time to start
			time.Sleep(2 * time.Second)

			if !p.IsRunning() {
				t.Fatal("Proxy should be running")
			}

			fmt.Printf("Playback proxy started on port %d, control port %d\n", p.Port, *p.ControlPort)

			// Stop using HTTP shutdown
			if err := p.Stop(); err != nil {
				t.Fatalf("Failed to stop proxy via HTTP: %v", err)
			}

			// Verify it stopped
			time.Sleep(1 * time.Second)
			if p.IsRunning() {
				t.Fatal("Proxy should have stopped")
			}

			fmt.Println("Playback proxy stopped successfully via HTTP shutdown")
		})
	*/
}
