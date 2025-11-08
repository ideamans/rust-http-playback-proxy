#[cfg(test)]
mod types_tests {
    use crate::types::{
        BodyChunk, ContentEncodingType, DeviceType, Inventory, Resource, Transaction,
    };
    use serde::Serialize;

    use std::str::FromStr;

    #[test]
    fn test_content_encoding_serialization() {
        let gzip = ContentEncodingType::Gzip;
        let json = serde_json::to_string(&gzip).unwrap();
        assert_eq!(json, "\"gzip\"");

        let br = ContentEncodingType::Br;
        let json = serde_json::to_string(&br).unwrap();
        assert_eq!(json, "\"br\"");
    }

    #[test]
    fn test_content_encoding_deserialization() {
        let gzip: ContentEncodingType = serde_json::from_str("\"gzip\"").unwrap();
        assert_eq!(gzip, ContentEncodingType::Gzip);

        let deflate: ContentEncodingType = serde_json::from_str("\"deflate\"").unwrap();
        assert_eq!(deflate, ContentEncodingType::Deflate);
    }

    #[test]
    fn test_content_encoding_from_str() {
        assert_eq!(
            ContentEncodingType::from_str("gzip").unwrap(),
            ContentEncodingType::Gzip
        );
        assert_eq!(
            ContentEncodingType::from_str("br").unwrap(),
            ContentEncodingType::Br
        );
        assert_eq!(
            ContentEncodingType::from_str("identity").unwrap(),
            ContentEncodingType::Identity
        );

        assert!(ContentEncodingType::from_str("invalid").is_err());
    }

    #[test]
    fn test_device_type_serialization() {
        let mobile = DeviceType::Mobile;
        let json = serde_json::to_string(&mobile).unwrap();
        assert_eq!(json, "\"mobile\"");

        let desktop = DeviceType::Desktop;
        let json = serde_json::to_string(&desktop).unwrap();
        assert_eq!(json, "\"desktop\"");
    }

    #[test]
    fn test_resource_creation() {
        let resource = Resource::new("GET".to_string(), "https://example.com".to_string());

        assert_eq!(resource.method, "GET");
        assert_eq!(resource.url, "https://example.com");
        assert_eq!(resource.ttfb_ms, 0);
        assert!(resource.status_code.is_none());
        assert!(resource.mbps.is_none());
    }

    #[test]
    fn test_resource_serialization() {
        let mut resource = Resource::new("GET".to_string(), "https://example.com".to_string());
        resource.status_code = Some(200);
        resource.mbps = Some(1.5);

        let json = serde_json::to_string(&resource).unwrap();
        assert!(json.contains("\"method\":\"GET\""));
        assert!(json.contains("\"url\":\"https://example.com\""));
        assert!(json.contains("\"statusCode\":200"));
        assert!(json.contains("\"mbps\":1.5"));
    }

    #[test]
    fn test_inventory_creation() {
        let inventory = Inventory::new();

        assert!(inventory.entry_url.is_none());
        assert!(inventory.device_type.is_none());
        assert!(inventory.resources.is_empty());
    }

    #[test]
    fn test_inventory_serialization() {
        let mut inventory = Inventory::new();
        inventory.entry_url = Some("https://example.com".to_string());
        inventory.device_type = Some(DeviceType::Mobile);

        let resource = Resource::new("GET".to_string(), "https://example.com".to_string());
        inventory.resources.push(resource);

        // 2スペースインデントで整形
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        inventory.serialize(&mut ser).unwrap();
        let json = String::from_utf8(buf).unwrap();
        assert!(json.contains("\"entryUrl\""));
        assert!(json.contains("\"deviceType\""));
        assert!(json.contains("\"resources\""));
    }

    #[test]
    fn test_inventory_deserialization() {
        let json = r#"{
            "entryUrl": "https://example.com",
            "deviceType": "mobile",
            "resources": [
                {
                    "method": "GET",
                    "url": "https://example.com",
                    "ttfbMs": 100,
                    "statusCode": 200
                }
            ]
        }"#;

        let inventory: Inventory = serde_json::from_str(json).unwrap();

        assert_eq!(inventory.entry_url, Some("https://example.com".to_string()));
        assert_eq!(inventory.device_type, Some(DeviceType::Mobile));
        assert_eq!(inventory.resources.len(), 1);

        let resource = &inventory.resources[0];
        assert_eq!(resource.method, "GET");
        assert_eq!(resource.url, "https://example.com");
        assert_eq!(resource.ttfb_ms, 100);
        assert_eq!(resource.status_code, Some(200));
    }

    #[test]
    fn test_body_chunk_creation() {
        let chunk = BodyChunk {
            chunk: b"test data".to_vec(),
            target_time: 1000,
        };

        assert_eq!(chunk.chunk, b"test data");
        assert_eq!(chunk.target_time, 1000);
    }

    #[test]
    fn test_transaction_creation() {
        let chunks = vec![
            BodyChunk {
                chunk: b"chunk1".to_vec(),
                target_time: 100,
            },
            BodyChunk {
                chunk: b"chunk2".to_vec(),
                target_time: 200,
            },
        ];

        let transaction = Transaction {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            ttfb: 50,
            status_code: Some(200),
            error_message: None,
            raw_headers: None,
            chunks,
            target_close_time: 300, // Example close time
        };

        assert_eq!(transaction.method, "GET");
        assert_eq!(transaction.url, "https://example.com");
        assert_eq!(transaction.ttfb, 50);
        assert_eq!(transaction.status_code, Some(200));
        assert_eq!(transaction.chunks.len(), 2);
        assert_eq!(transaction.target_close_time, 300);
    }
}
