/**
 * Example: Testing HTTP Shutdown functionality
 *
 * This demonstrates how to use the control port for graceful HTTP-based shutdown
 */

import { startRecording, startPlayback } from '../dist/index.js';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function testRecordingShutdown() {
  console.log('Testing recording proxy HTTP shutdown...');

  const tmpDir = path.join(__dirname, '..', 'test-inventory-' + Date.now());
  fs.mkdirSync(tmpDir, { recursive: true });

  try {
    const proxy = await startRecording({
      port: 0, // Auto-assign port
      deviceType: 'desktop',
      inventoryDir: tmpDir,
      controlPort: 19091, // Enable control API
    });

    console.log(`Recording proxy started on port ${proxy.port}, control port ${proxy.controlPort}`);

    // Wait a bit
    await new Promise((resolve) => setTimeout(resolve, 2000));

    if (!proxy.isRunning()) {
      throw new Error('Proxy should be running');
    }

    // Stop via HTTP shutdown
    console.log('Stopping via HTTP shutdown...');
    await proxy.stop();

    // Verify it stopped
    await new Promise((resolve) => setTimeout(resolve, 1000));
    if (proxy.isRunning()) {
      throw new Error('Proxy should have stopped');
    }

    console.log('✓ Recording proxy stopped successfully via HTTP shutdown');
  } finally {
    // Cleanup
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
}

async function main() {
  try {
    await testRecordingShutdown();
    console.log('\nAll tests passed!');
  } catch (error) {
    console.error('Test failed:', error);
    process.exit(1);
  }
}

main();
