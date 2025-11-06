import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { ensureBinary, getFullBinaryPath } from './binary';
import type { ProxyMode, RecordingOptions, PlaybackOptions, Inventory } from './types';

/**
 * Represents a running proxy instance
 */
export class Proxy {
  public readonly mode: ProxyMode;
  public readonly port: number;
  public readonly inventoryDir: string;
  public readonly entryUrl?: string;
  public readonly deviceType?: string;

  private process?: ChildProcess;

  constructor(
    mode: ProxyMode,
    port: number,
    inventoryDir: string,
    entryUrl?: string,
    deviceType?: string
  ) {
    this.mode = mode;
    this.port = port;
    this.inventoryDir = inventoryDir;
    this.entryUrl = entryUrl;
    this.deviceType = deviceType;
  }

  /**
   * Set the child process
   */
  setProcess(proc: ChildProcess): void {
    this.process = proc;
  }

  /**
   * Stop the proxy gracefully
   * For recording mode, this sends SIGINT to allow the proxy to save the inventory
   */
  async stop(): Promise<void> {
    if (!this.process) {
      throw new Error('Proxy is not running');
    }

    return new Promise((resolve, reject) => {
      if (!this.process) {
        reject(new Error('Proxy is not running'));
        return;
      }

      // Set up timeout
      const timeout = setTimeout(() => {
        if (this.process) {
          this.process.kill('SIGKILL');
          reject(new Error('Proxy did not stop gracefully, killed forcefully'));
        }
      }, 10000);

      // Listen for exit
      this.process.once('exit', (code, signal) => {
        clearTimeout(timeout);
        // Exit code 130 is expected for SIGINT, null can also occur on some platforms
        // Also accept signal === 'SIGINT' as success
        if (code === 0 || code === 130 || code === null || signal === 'SIGINT') {
          resolve();
        } else {
          reject(new Error(`Proxy exited with code ${code} signal ${signal}`));
        }
      });

      // Send SIGINT for graceful shutdown
      this.process.kill('SIGINT');
    });
  }

  /**
   * Check if the proxy is still running
   */
  isRunning(): boolean {
    if (!this.process) {
      return false;
    }

    // Check if process is still alive
    try {
      process.kill(this.process.pid!, 0);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Wait for the proxy to exit
   */
  async wait(): Promise<void> {
    if (!this.process) {
      throw new Error('Proxy is not running');
    }

    return new Promise((resolve, reject) => {
      this.process!.once('exit', (code) => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`Proxy exited with code ${code}`));
        }
      });
    });
  }

  /**
   * Load the inventory for this proxy
   * This is useful after recording is complete
   */
  async getInventory(): Promise<Inventory> {
    const inventoryPath = path.join(this.inventoryDir, 'inventory.json');
    return loadInventory(inventoryPath);
  }
}

/**
 * Start a recording proxy
 */
export async function startRecording(options: RecordingOptions): Promise<Proxy> {
  await ensureBinary();

  const binaryPath = getFullBinaryPath();

  // Set defaults to match CLI behavior
  const port = options.port !== undefined ? options.port : 8080;
  const deviceType = options.deviceType || 'mobile';
  const inventoryDir = options.inventoryDir || './inventory';

  // Build command
  const args: string[] = ['recording'];

  // Add entry URL if provided
  if (options.entryUrl) {
    args.push(options.entryUrl);
  }

  // Add port option (only if not default)
  if (options.port !== undefined) {
    args.push('--port', port.toString());
  }

  // Add device type
  args.push('--device', deviceType);

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Start the process
  const proc = spawn(binaryPath, args, {
    stdio: 'inherit',
    detached: false,
  });

  const proxy = new Proxy('recording', port, inventoryDir, options.entryUrl, deviceType);
  proxy.setProcess(proc);

  // Give the proxy a moment to start
  await new Promise(resolve => setTimeout(resolve, 500));

  return proxy;
}

/**
 * Start a playback proxy
 */
export async function startPlayback(options: PlaybackOptions): Promise<Proxy> {
  await ensureBinary();

  const binaryPath = getFullBinaryPath();

  // Set defaults
  const port = options.port !== undefined ? options.port : 8080;
  const inventoryDir = options.inventoryDir || './inventory';

  // Verify inventory exists
  const inventoryPath = getInventoryPath(inventoryDir);
  if (!fs.existsSync(inventoryPath)) {
    throw new Error(`Inventory file not found at ${inventoryPath}`);
  }

  // Build command
  const args: string[] = ['playback'];

  // Add port option (only if not default)
  if (options.port !== undefined && options.port !== 8080) {
    args.push('--port', port.toString());
  }

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Start the process
  const proc = spawn(binaryPath, args, {
    stdio: 'inherit',
    detached: false,
  });

  const proxy = new Proxy('playback', port, inventoryDir);
  proxy.setProcess(proc);

  // Give the proxy a moment to start
  await new Promise(resolve => setTimeout(resolve, 500));

  return proxy;
}

/**
 * Load an inventory from a JSON file
 */
export async function loadInventory(inventoryPath: string): Promise<Inventory> {
  const data = await fs.promises.readFile(inventoryPath, 'utf8');
  return JSON.parse(data) as Inventory;
}

/**
 * Save an inventory to a JSON file
 */
export async function saveInventory(inventoryPath: string, inventory: Inventory): Promise<void> {
  const data = JSON.stringify(inventory, null, 2);
  await fs.promises.writeFile(inventoryPath, data, 'utf8');
}

/**
 * Get the full path to a resource's content file
 */
export function getResourceContentPath(inventoryDir: string, resource: { contentFilePath?: string }): string {
  if (!resource.contentFilePath) {
    return '';
  }
  return path.join(inventoryDir, resource.contentFilePath);
}

/**
 * Get the path to the inventory.json file
 */
export function getInventoryPath(inventoryDir: string): string {
  return path.join(inventoryDir, 'inventory.json');
}
