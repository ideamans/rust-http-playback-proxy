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
	EncodingIdentity ContentEncodingType = "identity"
)

// Resource represents a single HTTP resource in the inventory
type Resource struct {
	Method              string               `json:"method"`
	URL                 string               `json:"url"`
	TtfbMs              uint64               `json:"ttfbMs"`
	Mbps                *float64             `json:"mbps,omitempty"`
	StatusCode          *uint16              `json:"statusCode,omitempty"`
	ErrorMessage        *string              `json:"errorMessage,omitempty"`
	RawHeaders          map[string]string    `json:"rawHeaders,omitempty"`
	ContentEncoding     *ContentEncodingType `json:"contentEncoding,omitempty"`
	ContentTypeMime     *string              `json:"contentTypeMime,omitempty"`
	ContentTypeCharset  *string              `json:"contentTypeCharset,omitempty"`
	ContentFilePath     *string              `json:"contentFilePath,omitempty"`
	ContentUtf8         *string              `json:"contentUtf8,omitempty"`
	ContentBase64       *string              `json:"contentBase64,omitempty"`
	Minify              *bool                `json:"minify,omitempty"`
}

// Inventory represents the complete inventory of recorded resources
type Inventory struct {
	EntryURL   *string      `json:"entryUrl,omitempty"`
	DeviceType *DeviceType  `json:"deviceType,omitempty"`
	Resources  []Resource   `json:"resources"`
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

// GetInventoryPath returns the path to the inventory.json file
func GetInventoryPath(inventoryDir string) string {
	return filepath.Join(inventoryDir, "inventory.json")
}
