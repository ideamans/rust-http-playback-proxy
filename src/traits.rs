use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// HTTP client abstraction for making requests
#[async_trait]
#[allow(dead_code)]
pub trait HttpClient: Send + Sync {
    async fn request(
        &self,
        method: &str,
        url: &str,
        headers: Option<&std::collections::HashMap<String, String>>,
        body: Option<&[u8]>,
    ) -> Result<HttpResponse>;
}

/// HTTP response abstraction
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HttpResponse {
    pub status_code: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

/// File system abstraction for I/O operations
#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn read(&self, path: &Path) -> Result<Vec<u8>>;
    async fn write(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn create_dir_all(&self, path: &Path) -> Result<()>;
    async fn exists(&self, path: &Path) -> bool;
    async fn read_to_string(&self, path: &Path) -> Result<String>;
    async fn write_string(&self, path: &Path, content: &str) -> Result<()>;
}

/// Time abstraction for testing timing behavior
#[allow(dead_code)]
pub trait TimeProvider: Send + Sync {
    fn now_ms(&self) -> u64;
    fn elapsed_since(&self, start: u64) -> u64;
}

/// Port finder abstraction
#[allow(dead_code)]
pub trait PortFinder: Send + Sync {
    fn find_available_port(&self, start_port: u16) -> Result<u16>;
}

/// Real implementations
pub struct RealFileSystem;
pub struct RealTimeProvider {
    #[allow(dead_code)]
    start_time: std::time::Instant,
}

impl RealTimeProvider {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
        }
    }
}

impl Default for RealTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSystem for RealFileSystem {
    async fn read(&self, path: &Path) -> Result<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        // Write and explicitly sync to disk to ensure visibility across processes
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(content).await?;
        file.sync_all().await?; // Ensure data is flushed to disk
        Ok(())
    }

    async fn create_dir_all(&self, path: &Path) -> Result<()> {
        tokio::fs::create_dir_all(path).await?;
        Ok(())
    }

    async fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    async fn read_to_string(&self, path: &Path) -> Result<String> {
        Ok(tokio::fs::read_to_string(path).await?)
    }

    async fn write_string(&self, path: &Path, content: &str) -> Result<()> {
        self.write(path, content.as_bytes()).await
    }
}

impl TimeProvider for RealTimeProvider {
    fn now_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    fn elapsed_since(&self, start: u64) -> u64 {
        let now = self.now_ms();
        now.saturating_sub(start)
    }
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Mock HTTP client for testing
    pub struct MockHttpClient {
        responses: Arc<Mutex<HashMap<String, HttpResponse>>>,
        requests: Arc<Mutex<Vec<(String, String)>>>, // (method, url)
    }

    #[allow(dead_code)]
    impl MockHttpClient {
        pub fn new() -> Self {
            Self {
                responses: Arc::new(Mutex::new(HashMap::new())),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn set_response(&self, key: &str, response: HttpResponse) {
            self.responses
                .lock()
                .unwrap()
                .insert(key.to_string(), response);
        }

        pub fn get_requests(&self) -> Vec<(String, String)> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn request(
            &self,
            method: &str,
            url: &str,
            _headers: Option<&std::collections::HashMap<String, String>>,
            _body: Option<&[u8]>,
        ) -> Result<HttpResponse> {
            self.requests
                .lock()
                .unwrap()
                .push((method.to_string(), url.to_string()));

            let key = format!("{}:{}", method, url);
            if let Some(response) = self.responses.lock().unwrap().get(&key) {
                Ok(response.clone())
            } else {
                Ok(HttpResponse {
                    status_code: 200,
                    headers: HashMap::new(),
                    body: b"default mock response".to_vec(),
                    elapsed_ms: 50,
                })
            }
        }
    }

    /// Mock file system for testing
    pub struct MockFileSystem {
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        directories: Arc<Mutex<std::collections::HashSet<String>>>,
    }

    #[allow(dead_code)]
    impl MockFileSystem {
        pub fn new() -> Self {
            Self {
                files: Arc::new(Mutex::new(HashMap::new())),
                directories: Arc::new(Mutex::new(std::collections::HashSet::new())),
            }
        }

        pub fn set_file(&self, path: &str, content: Vec<u8>) {
            self.files.lock().unwrap().insert(path.to_string(), content);
        }

        pub fn get_file(&self, path: &str) -> Option<Vec<u8>> {
            self.files.lock().unwrap().get(path).cloned()
        }

        pub fn file_exists(&self, path: &str) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }

        #[cfg(test)]
        pub fn list_files(&self) -> Vec<String> {
            self.files.lock().unwrap().keys().cloned().collect()
        }
    }

    #[async_trait]
    impl FileSystem for MockFileSystem {
        async fn read(&self, path: &Path) -> Result<Vec<u8>> {
            let path_str = path.to_string_lossy().to_string();
            self.files
                .lock()
                .unwrap()
                .get(&path_str)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("File not found: {}", path_str))
        }

        async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
            let path_str = path.to_string_lossy().to_string();
            self.files
                .lock()
                .unwrap()
                .insert(path_str, content.to_vec());
            Ok(())
        }

        async fn create_dir_all(&self, path: &Path) -> Result<()> {
            let path_str = path.to_string_lossy().to_string();
            self.directories.lock().unwrap().insert(path_str);
            Ok(())
        }

        async fn exists(&self, path: &Path) -> bool {
            let path_str = path.to_string_lossy().to_string();
            self.files.lock().unwrap().contains_key(&path_str)
        }

        async fn read_to_string(&self, path: &Path) -> Result<String> {
            let bytes = self.read(path).await?;
            Ok(String::from_utf8(bytes)?)
        }

        async fn write_string(&self, path: &Path, content: &str) -> Result<()> {
            self.write(path, content.as_bytes()).await
        }
    }

    /// Mock time provider for testing
    pub struct MockTimeProvider {
        current_time: Arc<Mutex<u64>>,
    }

    #[allow(dead_code)]
    impl MockTimeProvider {
        pub fn new(initial_time: u64) -> Self {
            Self {
                current_time: Arc::new(Mutex::new(initial_time)),
            }
        }

        pub fn advance(&self, ms: u64) {
            *self.current_time.lock().unwrap() += ms;
        }

        pub fn set_time(&self, ms: u64) {
            *self.current_time.lock().unwrap() = ms;
        }
    }

    impl TimeProvider for MockTimeProvider {
        fn now_ms(&self) -> u64 {
            *self.current_time.lock().unwrap()
        }

        fn elapsed_since(&self, start: u64) -> u64 {
            let now = self.now_ms();
            now.saturating_sub(start)
        }
    }
}
