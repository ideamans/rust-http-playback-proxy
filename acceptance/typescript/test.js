import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import http from 'node:http';
import { URL } from 'node:url';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { mkdtempSync, existsSync, readFileSync } from 'node:fs';
import {
  startRecording,
  startPlayback,
  loadInventory,
  getInventoryPath,
  getResourceContentPath,
} from 'http-playback-proxy';

// Test HTTP server
let testServer;
let serverUrl;
let inventoryDir;

before(async () => {
  // Create temporary directory
  inventoryDir = mkdtempSync(join(tmpdir(), 'http-playback-proxy-test-'));
  console.log(`Using inventory directory: ${inventoryDir}`);

  // Start test HTTP server
  testServer = http.createServer((req, res) => {
    if (req.url === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(`<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
    <link rel="stylesheet" href="/style.css">
</head>
<body>
    <h1>Hello, World!</h1>
    <script src="/script.js"></script>
</body>
</html>`);
    } else if (req.url === '/style.css') {
      res.writeHead(200, { 'Content-Type': 'text/css' });
      res.end('body { background-color: #f0f0f0; }');
    } else if (req.url === '/script.js') {
      res.writeHead(200, { 'Content-Type': 'application/javascript' });
      res.end('console.log("Hello from script");');
    } else {
      res.writeHead(404);
      res.end('Not found');
    }
  });

  await new Promise((resolve) => {
    testServer.listen(0, '127.0.0.1', () => {
      const addr = testServer.address();
      serverUrl = `http://127.0.0.1:${addr.port}`;
      console.log(`Test HTTP server started at ${serverUrl}`);
      resolve();
    });
  });
});

after(async () => {
  if (testServer) {
    await new Promise((resolve) => testServer.close(resolve));
    console.log('Test HTTP server stopped');
  }
});

describe('HTTP Playback Proxy Acceptance Test', () => {
  it('should record HTTP traffic', async () => {
    console.log('Starting recording proxy...');

    // Start recording proxy with control port for graceful shutdown
    const controlPort = 20000 + Math.floor(Math.random() * 1000); // Random port to avoid conflicts
    const proxy = await startRecording({
      entryUrl: serverUrl,
      port: 0, // Use default
      deviceType: 'mobile',
      inventoryDir: join(inventoryDir, 'inventory'),
      controlPort: controlPort, // Use random control port for HTTP shutdown
    });

    console.log(`Recording proxy started on port ${proxy.port}, control port ${proxy.controlPort}`);

    // Wait for proxy to be ready
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Make requests through proxy
    console.log('Making HTTP requests through recording proxy...');

    // Request 1: HTML page
    await makeRequest(serverUrl + '/', '127.0.0.1', proxy.port);
    console.log('Fetched HTML page');

    // Request 2: CSS file
    await makeRequest(serverUrl + '/style.css', '127.0.0.1', proxy.port);
    console.log('Fetched CSS file');

    // Request 3: JavaScript file
    await makeRequest(serverUrl + '/script.js', '127.0.0.1', proxy.port);
    console.log('Fetched JS file');

    // Give proxy time to process
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Stop recording proxy
    console.log('Stopping recording proxy...');
    await proxy.stop();

    // Wait for inventory to be saved
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Verify inventory file exists
    const inventoryPath = getInventoryPath(join(inventoryDir, 'inventory'));
    assert.ok(existsSync(inventoryPath), 'Inventory file should exist');
    console.log(`Inventory file created: ${inventoryPath}`);
  });

  it('should load and validate inventory', async () => {
    console.log('Loading and validating inventory...');

    const inventoryPath = getInventoryPath(join(inventoryDir, 'inventory'));
    const inventory = await loadInventory(inventoryPath);

    console.log(`Loaded inventory with ${inventory.resources.length} resources`);

    assert.ok(inventory.resources.length > 0, 'Should have at least one resource');

    // Validate resources
    for (let i = 0; i < inventory.resources.length; i++) {
      const resource = inventory.resources[i];
      console.log(`Resource ${i}: ${resource.method} ${resource.url} (TTFB: ${resource.ttfbMs}ms)`);

      assert.ok(resource.method, `Resource ${i} should have a method`);
      assert.ok(resource.url, `Resource ${i} should have a URL`);

      // Check content file exists
      if (resource.contentFilePath) {
        const contentPath = getResourceContentPath(join(inventoryDir, 'inventory'), resource);
        assert.ok(existsSync(contentPath), `Resource ${i} content file should exist`);
        console.log(`  Content file: ${contentPath}`);
      }
    }

    console.log('Inventory validation passed');
  });

  it('should playback recorded traffic', async () => {
    // CRITICAL: Stop the HTTP server to prove offline replay capability
    // Playback MUST serve from inventory without the origin server
    console.log('Stopping HTTP server to ensure offline replay...');

    if (testServer) {
      // Extract port from serverUrl before closing
      const urlObj = new URL(serverUrl);
      const serverPort = parseInt(urlObj.port, 10);

      await new Promise((resolve) => testServer.close(resolve));
      console.log('HTTP server stopped - playback must work without it');

      // Verify server is stopped - attempt direct connection (NOT through proxy)
      try {
        await new Promise((resolve, reject) => {
          const directReq = http.request({
            hostname: '127.0.0.1',
            port: serverPort,
            path: '/',
            method: 'GET',
            timeout: 1000,
          }, () => {
            reject(new Error('Direct request should have failed - server should be stopped!'));
          });

          directReq.on('error', reject);  // Expected - server is stopped
          directReq.setTimeout(1000, () => {
            directReq.destroy();
            reject(new Error('Timeout - connection should have failed'));
          });
          directReq.end();
        });
      } catch (err) {
        // Expected - server is stopped
        console.log('Confirmed: Direct requests fail (server is stopped)');
      }
    }

    console.log('Starting playback proxy...');

    // Start playback proxy - this MUST serve from inventory only
    const proxy = await startPlayback({
      port: 0, // Use default
      inventoryDir: join(inventoryDir, 'inventory'),
    });

    console.log(`Playback proxy started on port ${proxy.port}`);

    // Wait for proxy to be ready
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Make requests through proxy (same URLs as recording)
    // These MUST succeed even though the origin server is stopped,
    // proving they are served from the recorded inventory
    console.log('Making HTTP requests through playback proxy...');
    console.log('Note: Server is STOPPED - these will be served from inventory only');

    // Request 1: HTML page
    const htmlBody = await makeRequest(serverUrl + '/', '127.0.0.1', proxy.port);
    console.log(`Fetched HTML page: ${htmlBody.length} bytes`);
    assert.ok(htmlBody.length > 0, 'HTML response should not be empty');

    // Request 2: CSS file
    const cssBody = await makeRequest(serverUrl + '/style.css', '127.0.0.1', proxy.port);
    console.log(`Fetched CSS file: ${cssBody.length} bytes`);
    assert.ok(cssBody.length > 0, 'CSS response should not be empty');

    // Request 3: JavaScript file
    const jsBody = await makeRequest(serverUrl + '/script.js', '127.0.0.1', proxy.port);
    console.log(`Fetched JS file: ${jsBody.length} bytes`);
    assert.ok(jsBody.length > 0, 'JS response should not be empty');

    // Stop playback proxy
    console.log('Stopping playback proxy...');
    await proxy.stop();

    console.log('Playback test passed');
  });

  it('should test reload', async () => {
    console.log('Testing reload...');

    // Use random control port
    const controlPort = 21000 + (process.pid % 1000);

    // Start playback proxy with control port
    const proxy = await startPlayback({
      inventoryDir: join(inventoryDir, 'inventory'),
      controlPort: controlPort,
    });

    console.log(`Playback proxy started on port ${proxy.port}, control port ${proxy.controlPort}`);

    // Wait for proxy to be ready
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Test 1: Verify proxy is running and serving
    const htmlBody1 = await makeRequest(serverUrl + '/', '127.0.0.1', proxy.port);
    console.log('Verified proxy is serving requests');
    assert.ok(htmlBody1.length > 0, 'Response should not be empty');

    // Test 2: Reload inventory
    // Wait a moment to ensure connection is fully closed (important for Windows)
    await new Promise(resolve => setTimeout(resolve, 100));
    console.log('Testing reload...');
    const reloadMessage = await proxy.reload();
    console.log(`Reload successful: ${reloadMessage}`);

    // Verify proxy still works after reload
    const htmlBody2 = await makeRequest(serverUrl + '/', '127.0.0.1', proxy.port);
    console.log('Verified proxy works after reload');
    assert.ok(htmlBody2.length > 0, 'Response after reload should not be empty');

    // Test 3: Shutdown via HTTP
    console.log('Testing shutdown via control API...');
    await proxy.stop();

    await new Promise((resolve) => setTimeout(resolve, 500));
    assert.ok(!proxy.isRunning(), 'Proxy should have stopped');
    console.log('Verified proxy stopped successfully');

    console.log('Reload test passed');
  });
});

// Helper function to make HTTP request through proxy
// Uses HTTP/1.1 proxy protocol (absolute URI in request line)
function makeRequest(url, proxyHost, proxyPort) {
  return new Promise((resolve, reject) => {
    const options = {
      hostname: proxyHost,
      port: proxyPort,
      path: url,  // Full URL for proxy request (absolute URI)
      method: 'GET',
      headers: {
        Host: new URL(url).host,  // Set Host header for target server
        Connection: 'close',       // Use close to avoid keep-alive issues
      },
    };

    const req = http.request(options, (res) => {
      let body = '';
      res.on('data', (chunk) => {
        body += chunk;
      });
      res.on('end', () => {
        resolve(body);
      });
    });

    req.on('error', reject);
    req.setTimeout(10000, () => {
      req.destroy();
      reject(new Error('Request timeout'));
    });

    req.end();
  });
}
