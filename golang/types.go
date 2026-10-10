package httpplaybackproxy

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// DeviceType represents the device type for recording
type DeviceType string

const (
	DeviceTypeDesktop DeviceType = "desktop"
	DeviceTypeMobile  DeviceType = "mobile"
)

// ContentEncodingType represents the content encoding type
type ContentEncodingType string

const (
	EncodingGzip     ContentEncodingType = "gzip"
	EncodingCompress ContentEncodingType = "compress"
	EncodingDeflate  ContentEncodingType = "deflate"
	EncodingBr       ContentEncodingType = "br"
	EncodingZstd     ContentEncodingType = "zstd"
	EncodingIdentity ContentEncodingType = "identity"
)

// Resource represents a single HTTP resource in the inventory
type Resource struct {
	Method          string                 `json:"method"`
	URL             string                 `json:"url"`
	TtfbMs          uint64                 `json:"ttfbMs"`
	DurationMs      *uint64                `json:"durationMs,omitempty"`
	Mbps            *float64               `json:"mbps,omitempty"`
	StatusCode      *uint16                `json:"statusCode,omitempty"`
	ErrorMessage    *string                `json:"errorMessage,omitempty"`
	RawHeaders      map[string]HeaderValue `json:"rawHeaders,omitempty"`
	ContentEncoding *ContentEncodingType   `json:"contentEncoding,omitempty"`
	ContentTypeMime *string                `json:"contentTypeMime,omitempty"`
	ContentCharset  *string                `json:"contentCharset,omitempty"`
	ContentFilePath *string                `json:"contentFilePath,omitempty"`
	ContentUtf8     *string                `json:"contentUtf8,omitempty"`
	ContentBase64   *string                `json:"contentBase64,omitempty"`
	Minify          *bool                  `json:"minify,omitempty"`
	Original        *OriginalBody          `json:"original,omitempty"`
}

// OriginalBody is the recorded wire body of a resource. Playback serves it
// untouched while the editable file and the fields that shape the body still
// match what was recorded.
type OriginalBody struct {
	FilePath        string               `json:"filePath"`
	ContentSha256   string               `json:"contentSha256"`
	ContentEncoding *ContentEncodingType `json:"contentEncoding,omitempty"`
	ContentCharset  *string              `json:"contentCharset,omitempty"`
	Minify          *bool                `json:"minify,omitempty"`
}

// HeaderValue is a recorded header: a single string, or an array of strings
// for headers that appeared more than once (Set-Cookie).
type HeaderValue []string

func (h *HeaderValue) UnmarshalJSON(data []byte) error {
	var single string
	if err := json.Unmarshal(data, &single); err == nil {
		*h = HeaderValue{single}
		return nil
	}
	var multiple []string
	if err := json.Unmarshal(data, &multiple); err != nil {
		return err
	}
	*h = HeaderValue(multiple)
	return nil
}

func (h HeaderValue) MarshalJSON() ([]byte, error) {
	if len(h) == 1 {
		return json.Marshal(h[0])
	}
	return json.Marshal([]string(h))
}

// Inventory represents the complete inventory of recorded resources
type Inventory struct {
	// FormatVersion 2: transfer time is wire bytes × 8 / mbps and unedited
	// bodies replay from Original. Absent: transfer time is DurationMs.
	FormatVersion *uint32     `json:"formatVersion,omitempty"`
	EntryURL      *string     `json:"entryUrl,omitempty"`
	ExtraURLs     []string    `json:"extraUrls,omitempty"`
	DeviceType    *DeviceType `json:"deviceType,omitempty"`
	Resources     []Resource  `json:"resources"`
}

// LoadInventory loads an inventory from a JSON file
func LoadInventory(inventoryPath string) (*Inventory, error) {
	data, err := os.ReadFile(inventoryPath)
	if err != nil {
		return nil, err
	}

	var inventory Inventory
	if err := json.Unmarshal(data, &inventory); err != nil {
		return nil, err
	}

	return &inventory, nil
}

// SaveInventory saves an inventory to a JSON file
func SaveInventory(inventoryPath string, inventory *Inventory) error {
	data, err := json.MarshalIndent(inventory, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(inventoryPath, data, 0644)
}

// GetResourceContentPath returns the full path to a resource's content file
// given the inventory directory and the resource
func GetResourceContentPath(inventoryDir string, resource *Resource) string {
	if resource.ContentFilePath == nil {
		return ""
	}
	return filepath.Join(inventoryDir, *resource.ContentFilePath)
}

// GetInventoryPath returns the path to the index.json file
func GetInventoryPath(inventoryDir string) string {
	return filepath.Join(inventoryDir, "index.json")
}
