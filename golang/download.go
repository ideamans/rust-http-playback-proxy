package httpplaybackproxy

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"time"
)

const (
	githubUser = "pagespeed-quest"
	githubRepo = "http-playback-proxy"
	baseURL    = "https://github.com/" + githubUser + "/" + githubRepo + "/releases/download"

	// Default download timeout
	defaultDownloadTimeout = 5 * time.Minute
)

// getPlatform returns the current platform string (e.g., "darwin-arm64")
func getPlatform() string {
	platform := runtime.GOOS + "-" + runtime.GOARCH
	// Normalize platform strings
	switch platform {
	case "darwin-amd64":
		return "darwin-amd64"
	case "darwin-arm64":
		return "darwin-arm64"
	case "linux-amd64":
		return "linux-amd64"
	case "linux-arm64":
		return "linux-arm64"
	case "windows-amd64":
		return "windows-amd64"
	default:
		return platform
	}
}

// getBinaryName returns the expected binary name for the current platform
func getBinaryName() string {
	if runtime.GOOS == "windows" {
		return "http-playback-proxy.exe"
	}
	return "http-playback-proxy"
}

// getBinaryPath returns the expected path to the binary
func getBinaryPath() string {
	platform := getPlatform()
	return filepath.Join("bin", platform, getBinaryName())
}

// getCacheDir returns a writable cache directory for downloaded binaries
// Priority: HTTP_PLAYBACK_PROXY_CACHE_DIR > XDG_CACHE_HOME/http-playback-proxy > ~/.cache/http-playback-proxy
func getCacheDir() (string, error) {
	// Check environment variable first
	if cacheDir := os.Getenv("HTTP_PLAYBACK_PROXY_CACHE_DIR"); cacheDir != "" {
		return cacheDir, nil
	}

	// Try XDG_CACHE_HOME
	if xdgCache := os.Getenv("XDG_CACHE_HOME"); xdgCache != "" {
		return filepath.Join(xdgCache, "http-playback-proxy"), nil
	}

	// Fall back to user's home directory
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("failed to get home directory: %w", err)
	}

	return filepath.Join(homeDir, ".cache", "http-playback-proxy"), nil
}

// getPackageRoot returns the package root directory
// Uses runtime.Caller to find the actual package location
// In development: /path/to/project/golang/download.go -> /path/to/project/golang
// In production: /path/to/go/pkg/mod/github.com/pagespeed-quest/http-playback-proxy/golang@version/download.go
func getPackageRoot() (string, error) {
	// Get the location of this source file
	_, filename, _, ok := runtime.Caller(0)
	if !ok {
		return "", fmt.Errorf("failed to get caller information")
	}

	// This file is in the package root directory
	// e.g., /path/to/project/golang/download.go -> /path/to/project/golang
	packageDir := filepath.Dir(filename)

	// Verify this is the correct package by checking for go.mod
	// This prevents accidentally using a different module's directory
	goModPath := filepath.Join(packageDir, "go.mod")
	if _, err := os.Stat(goModPath); err == nil {
		// Found go.mod in the package directory - this is correct
		return packageDir, nil
	}

	// No go.mod found - this is expected in installed packages
	// Return the directory anyway (it will contain bin/ subdirectory if binaries are bundled)
	return packageDir, nil
}

// checkBinaryExists checks if the binary exists in the package or cache
// Note: Go modules are installed read-only in $GOPATH/pkg/mod, so binaries
// cannot be bundled with the module. This function checks the package directory
// only for development purposes (when working in the source tree).
// In production, binaries are always downloaded to the cache directory.
func checkBinaryExists() bool {
	// Check package directory first (development only)
	packageRoot, err := getPackageRoot()
	if err == nil {
		binPath := filepath.Join(packageRoot, getBinaryPath())
		if _, err := os.Stat(binPath); err == nil {
			return true
		}
	}

	// Check cache directory (production)
	cacheDir, err := getCacheDir()
	if err == nil {
		binPath := filepath.Join(cacheDir, getBinaryPath())
		if _, err := os.Stat(binPath); err == nil {
			return true
		}
	}

	return false
}

// CheckBinaryExists is a public wrapper for checking if the binary exists
func CheckBinaryExists() bool {
	return checkBinaryExists()
}

// downloadBinary downloads the pre-built binary from GitHub Releases
func downloadBinary() error {
	return downloadBinaryVersion(Version)
}

// downloadBinaryVersion downloads a specific version of the pre-built binary
func downloadBinaryVersion(version string) error {
	// Try to download to cache directory first
	cacheDir, err := getCacheDir()
	if err != nil {
		return fmt.Errorf("failed to get cache directory: %w", err)
	}

	// Try cache directory first
	targetDir := cacheDir
	if err := os.MkdirAll(targetDir, 0755); err != nil {
		// If cache directory creation fails, try package directory
		packageRoot, projErr := getPackageRoot()
		if projErr != nil {
			return fmt.Errorf("failed to get target directory: cache=%w, package=%w", err, projErr)
		}
		targetDir = packageRoot
		fmt.Fprintf(os.Stderr, "Warning: Could not create cache directory, using package directory: %v\n", err)
	}

	platform := getPlatform()
	archiveName := fmt.Sprintf("http-playback-proxy-v%s-%s.tar.gz", version, platform)
	url := fmt.Sprintf("%s/v%s/%s", baseURL, version, archiveName)

	fmt.Printf("Downloading http-playback-proxy binary for %s...\n", platform)
	fmt.Printf("URL: %s\n", url)
	fmt.Printf("Target: %s\n", targetDir)

	// Create HTTP client with timeout and context
	ctx, cancel := context.WithTimeout(context.Background(), defaultDownloadTimeout)
	defer cancel()

	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	client := &http.Client{
		Timeout: defaultDownloadTimeout,
	}

	// Download the tar.gz archive
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("failed to download binary: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("failed to download binary: HTTP %d", resp.StatusCode)
	}

	// Extract the tar.gz archive
	gzr, err := gzip.NewReader(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to create gzip reader: %w", err)
	}
	defer gzr.Close()

	tr := tar.NewReader(gzr)

	for {
		header, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return fmt.Errorf("failed to read tar: %w", err)
		}

		target := filepath.Join(targetDir, "bin", platform, header.Name)

		switch header.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(target, 0755); err != nil {
				return fmt.Errorf("failed to create directory: %w", err)
			}
		case tar.TypeReg:
			dir := filepath.Dir(target)
			if err := os.MkdirAll(dir, 0755); err != nil {
				return fmt.Errorf("failed to create directory: %w", err)
			}

			f, err := os.OpenFile(target, os.O_CREATE|os.O_RDWR, os.FileMode(header.Mode))
			if err != nil {
				return fmt.Errorf("failed to create file: %w", err)
			}

			if _, err := io.Copy(f, tr); err != nil {
				f.Close()
				return fmt.Errorf("failed to write file: %w", err)
			}
			f.Close()

			// Make binary executable on Unix-like systems
			if runtime.GOOS != "windows" {
				if err := os.Chmod(target, 0755); err != nil {
					return fmt.Errorf("failed to make binary executable: %w", err)
				}
			}
		}
	}

	fmt.Printf("Successfully downloaded and extracted binary to %s\n", targetDir)
	return nil
}

// DownloadBinary is a public wrapper for downloading the binary with the default version
func DownloadBinary(version string) error {
	if version == "" {
		return downloadBinary()
	}
	return downloadBinaryVersion(version)
}

// EnsureBinary ensures the binary is available, downloading if necessary.
// This is the recommended way to initialize the library.
// Returns an error if the binary cannot be found or downloaded.
func EnsureBinary() error {
	if checkBinaryExists() {
		return nil
	}

	fmt.Println("Pre-built binary not found. Attempting to download from GitHub Releases...")

	if err := downloadBinary(); err != nil {
		return fmt.Errorf("failed to download binary v%s: %w", Version, err)
	}

	return nil
}

// GetBinaryPath returns the full path to the binary
// Returns an error if the binary doesn't exist
// Priority: 1) Package directory (development), 2) Cache directory (production)
func GetBinaryPath() (string, error) {
	// Check package directory first (development only)
	packageRoot, err := getPackageRoot()
	if err == nil {
		binPath := filepath.Join(packageRoot, getBinaryPath())
		if _, err := os.Stat(binPath); err == nil {
			return binPath, nil
		}
	}

	// Check cache directory (production)
	cacheDir, err := getCacheDir()
	if err == nil {
		binPath := filepath.Join(cacheDir, getBinaryPath())
		if _, err := os.Stat(binPath); err == nil {
			return binPath, nil
		}
	}

	return "", fmt.Errorf("binary not found, please call EnsureBinary() first")
}
