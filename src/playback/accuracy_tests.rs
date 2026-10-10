//! Round trips through the recorder's batch processor and the playback
//! transaction builder, checking the two properties the tool exists for:
//! an unedited resource is replayed byte for byte in exactly the recorded time,
//! and an edited one is replayed correctly in time proportional to its size.

#[cfg(test)]
mod tests {
    use crate::playback::load_inventory;
    use crate::playback::transaction::{
        PlaybackContext, convert_resources_to_transactions, minify_content, transfer_duration_ms,
    };
    use crate::recording::batch_processor::BatchProcessor;
    use crate::recording::proxy::save_inventory_with_fs;
    use crate::recording::throughput_mbps;
    use crate::traits::{RealFileSystem, RealTimeProvider};
    use crate::types::{
        CURRENT_FORMAT_VERSION, ContentEncodingType, HeaderValue, Inventory, Resource, Transaction,
    };
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::TempDir;

    const HTML: &str = "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01 Transitional//EN\"><html><head><title>a &amp; b</title></head><body><p><span>x</span><span>y</span> &lt;script&gt;</p><pre>  keep\n   this</pre><script>// comment\nvar a = 1\nvar b = `line1\n  line2`</script><svg><use xlink:href=\"#i\"></use></svg></body></html>";
    const JS: &str = "var a=1;function f(){return a+1}var t=`x\n  y`;";
    const CSS: &str =
        "body{margin:0;padding:0}h1{color:red}@media (max-width:600px){h1{color:blue}}";

    fn gzip(data: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(6));
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn gunzip(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(data)
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    /// A resource as the recording handler leaves it before batch processing.
    fn recorded(
        url: &str,
        mime: &str,
        wire: Vec<u8>,
        encoding: Option<ContentEncodingType>,
        duration_us: u64,
    ) -> Resource {
        let mut resource = Resource::new("GET".to_string(), url.to_string());
        resource.status_code = Some(200);
        resource.ttfb_ms = 80;
        resource.duration_ms = Some((duration_us + 500) / 1000);
        resource.mbps = throughput_mbps(wire.len(), duration_us);
        resource.content_type_mime = Some(mime.to_string());
        let mut headers = HashMap::new();
        headers.insert(
            "content-type".to_string(),
            HeaderValue::Single(format!("{}; charset=utf-8", mime)),
        );
        if let Some(enc) = &encoding {
            let name = match enc {
                ContentEncodingType::Gzip => "gzip",
                ContentEncodingType::Br => "br",
                _ => "identity",
            };
            headers.insert(
                "content-encoding".to_string(),
                HeaderValue::Single(name.to_string()),
            );
        }
        resource.raw_headers = Some(headers);
        resource.content_encoding = encoding;
        resource.raw_body = Some(wire);
        resource
    }

    /// Record `resources` into a fresh inventory directory and load it back.
    async fn record(dir: &Path, resources: Vec<Resource>) -> Inventory {
        let mut inventory = Inventory::new();
        inventory.resources = resources;
        let processor = BatchProcessor::new(
            dir.to_path_buf(),
            Arc::new(RealFileSystem),
            Arc::new(RealTimeProvider::new()),
        );
        processor.process_all(&mut inventory).await.unwrap();
        save_inventory_with_fs(&inventory, dir, Arc::new(RealFileSystem))
            .await
            .unwrap();
        load_inventory(dir, Arc::new(RealFileSystem)).await.unwrap()
    }

    async fn replay(dir: &Path, inventory: &Inventory) -> Vec<Transaction> {
        convert_resources_to_transactions(inventory, dir, Arc::new(RealFileSystem))
            .await
            .unwrap()
    }

    fn body(transaction: &Transaction) -> Vec<u8> {
        transaction
            .chunks
            .iter()
            .flat_map(|c| c.chunk.clone())
            .collect()
    }

    fn header<'a>(transaction: &'a Transaction, name: &str) -> Option<&'a str> {
        transaction
            .raw_headers
            .as_ref()
            .unwrap()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.first())
    }

    fn sample() -> Vec<Resource> {
        let image: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        vec![
            recorded(
                "https://example.com/",
                "text/html",
                gzip(HTML.as_bytes()),
                Some(ContentEncodingType::Gzip),
                2_400,
            ),
            recorded(
                "https://example.com/app.js",
                "application/javascript",
                gzip(JS.as_bytes()),
                Some(ContentEncodingType::Gzip),
                700,
            ),
            recorded(
                "https://example.com/app.css",
                "text/css",
                gzip(CSS.as_bytes()),
                Some(ContentEncodingType::Gzip),
                450,
            ),
            recorded(
                "https://example.com/hero.jpg",
                "image/jpeg",
                image,
                None,
                37_600,
            ),
        ]
    }

    #[tokio::test]
    async fn unedited_resources_replay_byte_for_byte_in_the_recorded_time() {
        let temp = TempDir::new().unwrap();
        let wires: Vec<Vec<u8>> = sample()
            .iter()
            .map(|r| r.raw_body.clone().unwrap())
            .collect();
        let inventory = record(temp.path(), sample()).await;
        assert_eq!(inventory.format_version, Some(CURRENT_FORMAT_VERSION));

        let transactions = replay(temp.path(), &inventory).await;
        assert_eq!(transactions.len(), 4);
        for ((transaction, resource), wire) in
            transactions.iter().zip(&inventory.resources).zip(&wires)
        {
            assert_eq!(&body(transaction), wire, "{} body differs", transaction.url);
            assert_eq!(
                transaction.target_close_time,
                resource.duration_ms.unwrap(),
                "{} timing differs",
                transaction.url
            );
            assert_eq!(
                header(transaction, "content-length"),
                Some(wire.len().to_string().as_str())
            );
        }
        assert_eq!(header(&transactions[0], "content-encoding"), Some("gzip"));
    }

    #[tokio::test]
    async fn html_is_stored_as_recorded() {
        let temp = TempDir::new().unwrap();
        let inventory = record(temp.path(), sample()).await;
        let html = &inventory.resources[0];
        let stored =
            std::fs::read(temp.path().join(html.content_file_path.as_ref().unwrap())).unwrap();
        assert_eq!(stored, HTML.as_bytes());
        assert_eq!(html.minify, Some(false));
    }

    #[tokio::test]
    async fn binary_bodies_are_not_duplicated_inline() {
        let temp = TempDir::new().unwrap();
        let inventory = record(temp.path(), sample()).await;
        let image = &inventory.resources[3];
        assert!(image.content_base64.is_none());
        // The file is the wire body, so there is nothing else to keep.
        assert!(image.original.is_none());
    }

    #[tokio::test]
    async fn an_edited_body_is_regenerated_and_timed_by_its_size() {
        let temp = TempDir::new().unwrap();
        let inventory = record(temp.path(), sample()).await;

        // Halve the image.
        let image = &inventory.resources[3];
        let image_path = temp.path().join(image.content_file_path.as_ref().unwrap());
        let bytes = std::fs::read(&image_path).unwrap();
        std::fs::write(&image_path, &bytes[..bytes.len() / 2]).unwrap();

        // Append a statement to the minified script.
        let js = &inventory.resources[1];
        let js_path = temp.path().join(js.content_file_path.as_ref().unwrap());
        let mut source = std::fs::read_to_string(&js_path).unwrap();
        source.push_str("\nvar c = a // trailing comment\nvar d = 2\n");
        std::fs::write(&js_path, source).unwrap();

        let transactions = replay(temp.path(), &inventory).await;

        let recorded_image_ms = image.duration_ms.unwrap();
        let halved = transactions[3].target_close_time;
        assert!(
            (halved as i64 - (recorded_image_ms / 2) as i64).abs() <= 1,
            "half the bytes should take half the time: {} vs {}",
            halved,
            recorded_image_ms
        );

        let js_served = gunzip(&body(&transactions[1]));
        let js_text = String::from_utf8(js_served).unwrap();
        assert!(
            js_text.contains("var d=2"),
            "statement after a trailing comment survived: {}",
            js_text
        );
        assert!(
            js_text.contains("`x\n  y`"),
            "template literal kept: {}",
            js_text
        );
        assert_eq!(header(&transactions[1], "content-encoding"), Some("gzip"));
    }

    #[tokio::test]
    async fn changing_the_encoding_updates_body_and_header_together() {
        let temp = TempDir::new().unwrap();
        let mut inventory = record(temp.path(), sample()).await;
        inventory.resources[2].content_encoding = Some(ContentEncodingType::Br);
        inventory.resources[0].content_encoding = None;

        let transactions = replay(temp.path(), &inventory).await;

        assert_eq!(header(&transactions[2], "content-encoding"), Some("br"));
        let mut css = Vec::new();
        brotli::BrotliDecompress(&mut std::io::Cursor::new(body(&transactions[2])), &mut css)
            .unwrap();
        assert!(String::from_utf8(css).unwrap().contains("color:"));

        assert_eq!(header(&transactions[0], "content-encoding"), None);
        assert_eq!(body(&transactions[0]), HTML.as_bytes());
    }

    #[tokio::test]
    async fn raising_mbps_shortens_an_unedited_transfer() {
        let temp = TempDir::new().unwrap();
        let mut inventory = record(temp.path(), sample()).await;
        let recorded_ms = inventory.resources[3].duration_ms.unwrap();
        let mbps = inventory.resources[3].mbps.unwrap();
        inventory.resources[3].mbps = Some(mbps * 4.0);

        let transactions = replay(temp.path(), &inventory).await;
        let expected = (recorded_ms as f64 / 4.0).round() as i64;
        assert!((transactions[3].target_close_time as i64 - expected).abs() <= 1);
    }

    #[tokio::test]
    async fn legacy_inventories_keep_the_recorded_duration() {
        let temp = TempDir::new().unwrap();
        let mut inventory = record(temp.path(), sample()).await;
        inventory.format_version = None;
        let image = &inventory.resources[3];
        let image_path = temp.path().join(image.content_file_path.as_ref().unwrap());
        let bytes = std::fs::read(&image_path).unwrap();
        std::fs::write(&image_path, &bytes[..bytes.len() / 2]).unwrap();

        let transactions = replay(temp.path(), &inventory).await;
        assert_eq!(
            transactions[3].target_close_time,
            inventory.resources[3].duration_ms.unwrap()
        );
    }

    #[test]
    fn a_resource_without_throughput_borrows_its_host_median() {
        let mut inventory = Inventory::new();
        inventory.format_version = Some(CURRENT_FORMAT_VERSION);
        for mbps in [10.0, 20.0, 40.0] {
            let mut r = Resource::new("GET".into(), "https://cdn.example.com/a".into());
            r.mbps = Some(mbps);
            inventory.resources.push(r);
        }
        let context = PlaybackContext::from_inventory(&inventory);
        let orphan = Resource::new("GET".into(), "https://cdn.example.com/new.js".into());
        // 25,000 bytes at the median 20 Mbps = 10 ms
        assert_eq!(transfer_duration_ms(25_000, &orphan, &context), 10);
    }

    #[test]
    fn javascript_minification_keeps_asi_comments_and_templates() {
        let source = b"var a = 1\nvar b = a // note\n;(function () { return `x\n  y` })()\n";
        let out = minify_content(source, &Some("application/javascript".to_string())).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("var a=1"), "{}", out);
        assert!(out.contains("var b=a"), "{}", out);
        assert!(out.contains("`x\n  y`"), "{}", out);
        assert!(!out.contains("note"), "{}", out);
        // Re-parses as valid JavaScript
        crate::beautify::format_javascript(&out).unwrap();
    }
}
