import { test, describe } from 'node:test';
import assert from 'node:assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { startRecording, startPlayback, saveInventory } from '../proxy';
import type { Inventory } from '../types';

/**
 * Helper to create a temporary directory
 */
function createTempDir(prefix: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

/**
 * Helper to sleep for a given number of milliseconds
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe('Signal Shutdown', () => {
  test('RecordingProxySignalShutdown', async () => {
    const tmpDir = createTempDir('test-recording-shutdown-');

    try {
      // Start recording proxy with auto-assigned port
      const proxy = await startRecording({
        port: 0,
        inventoryDir: tmpDir,
      });

      // Give it time to fully start
      await sleep(2000);

      // Verify it's running
      assert.strictEqual(proxy.isRunning(), true, 'Proxy should be running');

      console.log(`Recording proxy started on port ${proxy.port}`);

      // Stop using signal-based shutdown (SIGTERM on Unix, CTRL_BREAK on Windows)
      await proxy.stop();

      // Verify it stopped
      await sleep(1000);
      assert.strictEqual(proxy.isRunning(), false, 'Proxy should have stopped');

      console.log('Recording proxy stopped successfully via signal shutdown');
    } finally {
      // Clean up temp directory
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('PlaybackProxySignalShutdown', async () => {
    const tmpDir = createTempDir('test-playback-shutdown-');

    try {
      // Create a minimal inventory
      const inventoryPath = path.join(tmpDir, 'index.json');
      const inventory: Inventory = {
        resources: [],
      };
      await saveInventory(inventoryPath, inventory);

      // Start playback proxy with auto-assigned port
      const proxy = await startPlayback({
        port: 0,
        inventoryDir: tmpDir,
      });

      // Give it time to fully start
      await sleep(2000);

      // Verify it's running
      assert.strictEqual(proxy.isRunning(), true, 'Proxy should be running');

      console.log(`Playback proxy started on port ${proxy.port}`);

      // Stop using signal-based shutdown (SIGTERM on Unix, CTRL_BREAK on Windows)
      await proxy.stop();

      // Verify it stopped
      await sleep(1000);
      assert.strictEqual(proxy.isRunning(), false, 'Proxy should have stopped');

      console.log('Playback proxy stopped successfully via signal shutdown');
    } finally {
      // Clean up temp directory
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });
});
