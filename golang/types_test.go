package httpplaybackproxy

import (
	"encoding/json"
	"path/filepath"
	"reflect"
	"testing"
)

const sampleInventory = `{
  "formatVersion": 2,
  "entryUrl": "https://example.com/",
  "resources": [
    {
      "method": "GET",
      "url": "https://example.com/",
      "ttfbMs": 120,
      "durationMs": 3,
      "mbps": 16.5,
      "rawHeaders": {
        "content-type": "text/html; charset=utf-8",
        "set-cookie": ["a=1", "b=2"]
      },
      "contentEncoding": "gzip",
      "contentFilePath": "contents/get/https/example.com/index.html",
      "minify": false,
      "original": {
        "filePath": "originals/get/https/example.com/index.html",
        "contentSha256": "abc",
        "contentEncoding": "gzip",
        "minify": false
      }
    }
  ]
}`

func TestInventoryRoundTripKeepsCurrentFormat(t *testing.T) {
	var inv Inventory
	if err := json.Unmarshal([]byte(sampleInventory), &inv); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if inv.FormatVersion == nil || *inv.FormatVersion != 2 {
		t.Fatalf("formatVersion lost: %v", inv.FormatVersion)
	}
	r := inv.Resources[0]
	if got := r.RawHeaders["set-cookie"]; !reflect.DeepEqual(got, HeaderValue{"a=1", "b=2"}) {
		t.Fatalf("multi-value header: %v", got)
	}
	if r.Original == nil || r.Original.FilePath != "originals/get/https/example.com/index.html" {
		t.Fatalf("original lost: %+v", r.Original)
	}

	path := filepath.Join(t.TempDir(), "index.json")
	if err := SaveInventory(path, &inv); err != nil {
		t.Fatalf("save: %v", err)
	}
	again, err := LoadInventory(path)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	if !reflect.DeepEqual(&inv, again) {
		t.Fatalf("round trip changed the inventory:\n%+v\n%+v", inv, *again)
	}
	var raw map[string]any
	data, _ := json.Marshal(again.Resources[0].RawHeaders)
	_ = json.Unmarshal(data, &raw)
	if _, single := raw["content-type"].(string); !single {
		t.Fatalf("single-value header should stay a string: %s", data)
	}
}
