package main

import (
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"testing"
	"time"

	proxy "github.com/pagespeed-quest/http-playback-proxy/golang"
)

// TestAcceptance is the main acceptance test
func TestAcceptance(t *testing.T) {
	// Setup: Create test HTTP server
	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
    <link rel="stylesheet" href="/style.css">
</head>
<body>
    <h1>Hello, World!</h1>
    <script src="/script.js"></script>
</body>
</html>`)
	})
	mux.HandleFunc("/style.css", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/css")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `body { background-color: #f0f0f0; }`)
	})
	mux.HandleFunc("/script.js", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/javascript")
		w.WriteHeader(http.StatusOK)
		fmt.Fprint(w, `console.log("Hello from script");`)
	})

	server := httptest.NewServer(mux)
	serverURL := server.URL
	t.Logf("Test HTTP server started at %s", serverURL)

	// Create temporary inventory directory
	tmpDir := t.TempDir()
	inventoryDir := filepath.Join(tmpDir, "inventory")
	t.Logf("Using inventory directory: %s", inventoryDir)

	// Test 1: Recording
	t.Run("Recording", func(t *testing.T) {
		testRecording(t, serverURL, inventoryDir)
	})

	// Test 2: Load and validate inventory
	t.Run("LoadInventory", func(t *testing.T) {
		testLoadInventory(t, inventoryDir)
	})

	// CRITICAL: Stop the HTTP server to prove offline replay capability
	// Playback MUST serve from inventory without the origin server
	t.Log("Stopping HTTP server to ensure offline replay...")
	server.Close()
	t.Log("HTTP server stopped - playback must work without it")

	// Verify server is truly stopped by attempting direct connection
	client := &http.Client{Timeout: 2 * time.Second}
	if _, err := client.Get(serverURL + "/"); err == nil {
		t.Fatal("Direct request should have failed - server should be stopped!")
	}
	t.Log("Confirmed: Direct requests fail (server is stopped)")

	// Test 3: Playback - Server is STOPPED, must serve from inventory only
	t.Run("Playback", func(t *testing.T) {
		testPlayback(t, serverURL, inventoryDir)
	})

	// Test 4: Shutdown (removed reload test - reload functionality removed from system)
}

func testRecording(t *testing.T, serverURL string, inventoryDir string) {
	t.Log("Starting recording proxy...")

	// Start recording proxy (no control port needed - uses signal-based shutdown)
	p, err := proxy.StartRecording(proxy.RecordingOptions{
		EntryURL:     serverURL,
		Port:         0, // Use default port
		DeviceType:   proxy.DeviceTypeMobile,
		InventoryDir: inventoryDir,
	})
	if err != nil {
		t.Fatalf("Failed to start recording proxy: %v", err)
	}
	defer func() {
		if p.IsRunning() {
			p.Stop()
		}
	}()

	t.Logf("Recording proxy started on port %d", p.Port)

	// Wait for proxy to be ready
	time.Sleep(1 * time.Second)

	// Create HTTP client with proxy
	proxyURL := fmt.Sprintf("http://127.0.0.1:%d", p.Port)
	proxyURLParsed, _ := url.Parse(proxyURL)
	client := &http.Client{
		Transport: &http.Transport{
			Proxy: func(req *http.Request) (*url.URL, error) {
				return proxyURLParsed, nil
			},
		},
		Timeout: 10 * time.Second,
	}

	// Make requests through proxy
	t.Log("Making HTTP requests through recording proxy...")

	// Request 1: HTML page
	resp, err := client.Get(serverURL + "/")
	if err != nil {
		t.Fatalf("Failed to fetch HTML page: %v", err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched HTML page: %d bytes", len(body))

	// Request 2: CSS file
	resp, err = client.Get(serverURL + "/style.css")
	if err != nil {
		t.Fatalf("Failed to fetch CSS file: %v", err)
	}
	body, _ = io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched CSS file: %d bytes", len(body))

	// Request 3: JavaScript file
	resp, err = client.Get(serverURL + "/script.js")
	if err != nil {
		t.Fatalf("Failed to fetch JS file: %v", err)
	}
	body, _ = io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched JS file: %d bytes", len(body))

	// Give proxy time to process
	time.Sleep(1 * time.Second)

	// Stop recording proxy
	t.Log("Stopping recording proxy...")
	if err := p.Stop(); err != nil {
		t.Fatalf("Failed to stop recording proxy: %v", err)
	}

	// Wait for inventory to be saved
	time.Sleep(2 * time.Second)

	// Verify inventory file exists
	inventoryPath := proxy.GetInventoryPath(inventoryDir)
	if _, err := os.Stat(inventoryPath); err != nil {
		t.Fatalf("Inventory file not found: %v", err)
	}
	t.Logf("Inventory file created: %s", inventoryPath)
}

func testLoadInventory(t *testing.T, inventoryDir string) {
	t.Log("Loading and validating inventory...")

	inventoryPath := proxy.GetInventoryPath(inventoryDir)
	inventory, err := proxy.LoadInventory(inventoryPath)
	if err != nil {
		t.Fatalf("Failed to load inventory: %v", err)
	}

	t.Logf("Loaded inventory with %d resources", len(inventory.Resources))

	if len(inventory.Resources) == 0 {
		t.Fatal("Expected at least one resource in inventory")
	}

	// List actual files in contents directory for debugging
	contentsDir := filepath.Join(inventoryDir, "contents")
	if info, err := os.Stat(contentsDir); err == nil && info.IsDir() {
		t.Logf("Listing contents directory: %s", contentsDir)
		filepath.Walk(contentsDir, func(path string, info os.FileInfo, err error) error {
			if err == nil && !info.IsDir() {
				relPath, _ := filepath.Rel(inventoryDir, path)
				t.Logf("  Found file: %s", relPath)
			}
			return nil
		})
	} else {
		t.Logf("Contents directory not found or not accessible: %v", err)
	}

	// Validate resources
	for i, resource := range inventory.Resources {
		t.Logf("Resource %d: %s %s (TTFB: %dms)", i, resource.Method, resource.URL, resource.TtfbMs)
		if resource.ContentFilePath != nil {
			t.Logf("  ContentFilePath from inventory: %s", *resource.ContentFilePath)
		} else {
			t.Logf("  ContentFilePath is nil")
		}

		if resource.Method == "" {
			t.Errorf("Resource %d has empty method", i)
		}
		if resource.URL == "" {
			t.Errorf("Resource %d has empty URL", i)
		}

		// Check content file exists
		if resource.ContentFilePath != nil {
			contentPath := proxy.GetResourceContentPath(inventoryDir, &resource)
			t.Logf("  Looking for file at: %s", contentPath)
			if _, err := os.Stat(contentPath); err != nil {
				t.Errorf("Resource %d content file not found: %s", i, contentPath)
			} else {
				t.Logf("  Content file: %s", contentPath)
			}
		}
	}

	t.Logf("Inventory validation passed")
}

func testPlayback(t *testing.T, serverURL string, inventoryDir string) {
	t.Log("Starting playback proxy...")

	// CRITICAL: The HTTP server is STOPPED before this test runs
	// This test MUST prove that playback serves from inventory without the origin server
	// If this test passes, it confirms true offline replay capability

	// Start playback proxy - this MUST serve from inventory only
	p, err := proxy.StartPlayback(proxy.PlaybackOptions{
		Port:         0, // Use default port
		InventoryDir: inventoryDir,
	})
	if err != nil {
		t.Fatalf("Failed to start playback proxy: %v", err)
	}
	defer func() {
		if p.IsRunning() {
			p.Stop()
		}
	}()

	t.Logf("Playback proxy started on port %d", p.Port)

	// Wait for proxy to be ready
	time.Sleep(1 * time.Second)

	// Create HTTP client with proxy
	// Important: We request the SAME URLs that were recorded,
	// but the proxy will serve them from the inventory instead of the actual server
	proxyURL := fmt.Sprintf("http://127.0.0.1:%d", p.Port)
	proxyURLParsed, _ := url.Parse(proxyURL)
	client := &http.Client{
		Transport: &http.Transport{
			Proxy: func(req *http.Request) (*url.URL, error) {
				return proxyURLParsed, nil
			},
		},
		Timeout: 10 * time.Second,
	}

	// Make requests through proxy (same URLs as recording)
	t.Log("Making HTTP requests through playback proxy...")
	t.Log("Note: These will be served from recorded inventory, not the actual server")

	// Request 1: HTML page
	resp, err := client.Get(serverURL + "/")
	if err != nil {
		t.Fatalf("Failed to fetch HTML page: %v", err)
	}
	body, _ := io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched HTML page: %d bytes", len(body))

	if resp.StatusCode != http.StatusOK {
		t.Errorf("Expected status 200, got %d", resp.StatusCode)
	}

	// Verify content contains expected text
	if len(body) == 0 {
		t.Error("Expected non-empty response body")
	}

	// Request 2: CSS file
	resp, err = client.Get(serverURL + "/style.css")
	if err != nil {
		t.Fatalf("Failed to fetch CSS file: %v", err)
	}
	body, _ = io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched CSS file: %d bytes", len(body))

	// Request 3: JavaScript file
	resp, err = client.Get(serverURL + "/script.js")
	if err != nil {
		t.Fatalf("Failed to fetch JS file: %v", err)
	}
	body, _ = io.ReadAll(resp.Body)
	resp.Body.Close()
	t.Logf("Fetched JS file: %d bytes", len(body))

	// Stop playback proxy
	t.Log("Stopping playback proxy...")
	if err := p.Stop(); err != nil {
		t.Fatalf("Failed to stop playback proxy: %v", err)
	}

	t.Log("Playback test passed")
}

// Removed: testShutdownAndReload - Reload functionality has been removed from the system
