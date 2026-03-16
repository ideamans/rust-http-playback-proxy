import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import * as net from 'net';
import { ensureBinary, getFullBinaryPath } from './binary';
import type { ProxyMode, RecordingOptions, PlaybackOptions, Inventory } from './types';

/**
 * Find an available port by binding to port 0 and releasing it
 */
async function getAvailablePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (address && typeof address === 'object') {
        const port = address.port;
        server.close(() => resolve(port));
      } else {
        server.close(() => reject(new Error('Failed to get port from server address')));
      }
    });
    server.on('error', reject);
  });
}

/**
 * Wait for a port to become available (accepting connections)
 * Uses TCP connection attempts with exponential backoff
 */
async function waitForPort(port: number, timeoutMs: number = 60000): Promise<void> {
  const startTime = Date.now();
  let delay = 50; // Start with 50ms delay

  while (Date.now() - startTime < timeoutMs) {
    try {
      await new Promise<void>((resolve, reject) => {
        const socket = net.createConnection({ port, host: '127.0.0.1' });
        socket.on('connect', () => {
          socket.destroy();
          resolve();
        });
        socket.on('error', reject);
        socket.setTimeout(1000, () => {
          socket.destroy();
          reject(new Error('Connection timeout'));
        });
      });
      return; // Port is open
    } catch {
      // Port not ready yet, wait and retry
      await new Promise((resolve) => setTimeout(resolve, delay));
      delay = Math.min(delay * 1.5, 500); // Exponential backoff, max 500ms
    }
  }

  throw new Error(`Timeout waiting for port ${port} to become available`);
}

/**
 * Represents a running proxy instance
 */
export class Proxy {
  public readonly mode: ProxyMode;
  public readonly inventoryDir: string;
  public readonly entryUrl?: string;
  public readonly deviceType?: string;

  private _port: number;
  private process?: ChildProcess;

  constructor(
    mode: ProxyMode,
    port: number,
    inventoryDir: string,
    entryUrl?: string,
    deviceType?: string
  ) {
    this.mode = mode;
    this._port = port;
    this.inventoryDir = inventoryDir;
    this.entryUrl = entryUrl;
    this.deviceType = deviceType;
  }

  /**
   * Get the actual port number
   */
  get port(): number {
    return this._port;
  }

  /**
   * Set the child process
   */
  setProcess(proc: ChildProcess): void {
    this.process = proc;
  }

  /**
   * Stop the proxy gracefully
   * Sends SIGTERM signal (cross-platform)
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

      // Set up timeout for forceful termination
      const timeout = setTimeout(() => {
        if (this.process) {
          this.process.kill('SIGKILL');
          reject(new Error('Proxy did not stop gracefully, killed forcefully'));
        }
      }, 10000);

      // Listen for exit
      this.process.once('exit', (code, signal) => {
        clearTimeout(timeout);
        // Accept clean exits: code 0, SIGTERM, SIGINT
        if (code === 0 || code === null || signal === 'SIGTERM' || signal === 'SIGINT') {
          resolve();
        } else {
          reject(new Error(`Proxy exited with code ${code} signal ${signal}`));
        }
      });

      // Send platform-appropriate signal
      try {
        if (process.platform === 'win32') {
          // On Windows, use the signal subcommand to send CTRL_BREAK
          const binaryPath = getFullBinaryPath();
          const { spawnSync } = require('child_process');
          const result = spawnSync(
            binaryPath,
            ['signal', '--pid', this.process.pid!.toString(), '--kind', 'ctrl-break'],
            { stdio: 'pipe' }
          );

          if (result.error) {
            clearTimeout(timeout);
            reject(new Error(`Failed to send signal: ${result.error.message}`));
            return;
          }

          if (result.status !== 0) {
            clearTimeout(timeout);
            const stderr = result.stderr?.toString() || '';
            reject(new Error(`Signal command failed with exit code ${result.status}: ${stderr}`));
            return;
          }
        } else {
          // On Unix, use standard SIGTERM
          this.process.kill('SIGTERM');
        }
      } catch (err) {
        clearTimeout(timeout);
        reject(err);
      }
    });
  }

  /**
   * Check if the proxy is still running
   */
  isRunning(): boolean {
    if (!this.process) {
      return false;
    }

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
   */
  async getInventory(): Promise<Inventory> {
    const inventoryPath = path.join(this.inventoryDir, 'index.json');
    return loadInventory(inventoryPath);
  }
}

/**
 * Start a recording proxy
 */
export async function startRecording(options: RecordingOptions): Promise<Proxy> {
  await ensureBinary();

  const binaryPath = getFullBinaryPath();

  // Get an available port if not specified or if 0
  let port: number;
  if (options.port === undefined || options.port === 0) {
    port = await getAvailablePort();
  } else {
    port = options.port;
  }

  const deviceType = options.deviceType || 'mobile';
  const inventoryDir = options.inventoryDir || './inventory';

  // Build command
  const args: string[] = ['recording'];

  // Add entry URL if provided
  if (options.entryUrl) {
    args.push(options.entryUrl);
  }

  // Always specify the port explicitly
  args.push('--port', port.toString());

  // Add device type
  args.push('--device', deviceType);

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Add extra URLs
  if (options.extraUrls) {
    for (const url of options.extraUrls) {
      args.push('--extra-url', url);
    }
  }

  // Start the process
  const spawnOptions: any = {
    stdio: ['ignore', 'inherit', 'inherit'],
    detached: false,
  };

  if (process.platform === 'win32') {
    spawnOptions.windowsVerbatimArguments = false;
  }

  const proc = spawn(binaryPath, args, spawnOptions);

  const proxy = new Proxy('recording', port, inventoryDir, options.entryUrl, deviceType);
  proxy.setProcess(proc);

  // Handle early exit
  let exited = false;
  proc.on('exit', (code) => {
    exited = true;
    if (code !== 0 && code !== null) {
      console.error(`Proxy process exited early with code ${code}`);
    }
  });

  // Wait for the port to become available
  try {
    await waitForPort(port, 15000);
  } catch (err) {
    if (exited) {
      throw new Error('Proxy process exited before port became available');
    }
    throw err;
  }

  return proxy;
}

/**
 * Start a playback proxy
 */
export async function startPlayback(options: PlaybackOptions): Promise<Proxy> {
  await ensureBinary();

  const binaryPath = getFullBinaryPath();
  const inventoryDir = options.inventoryDir || './inventory';

  // Verify inventory exists
  const inventoryPath = getInventoryPath(inventoryDir);
  if (!fs.existsSync(inventoryPath)) {
    throw new Error(`Inventory file not found at ${inventoryPath}`);
  }

  // Get an available port if not specified or if 0
  let port: number;
  if (options.port === undefined || options.port === 0) {
    port = await getAvailablePort();
  } else {
    port = options.port;
  }

  // Build command
  const args: string[] = ['playback'];

  // Always specify the port explicitly
  args.push('--port', port.toString());

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Add full throttle flag
  if (options.fullThrottle) {
    args.push('--full-throttle');
  }

  // Add passthrough flag
  if (options.passthrough) {
    args.push('--passthrough');
  }

  // Start the process
  const spawnOptions: any = {
    stdio: ['ignore', 'inherit', 'inherit'],
    detached: false,
  };

  if (process.platform === 'win32') {
    spawnOptions.windowsVerbatimArguments = false;
  }

  const proc = spawn(binaryPath, args, spawnOptions);

  const proxy = new Proxy('playback', port, inventoryDir);
  proxy.setProcess(proc);

  // Handle early exit
  let exited = false;
  proc.on('exit', (code) => {
    exited = true;
    if (code !== 0 && code !== null) {
      console.error(`Proxy process exited early with code ${code}`);
    }
  });

  // Wait for the port to become available
  try {
    await waitForPort(port, 15000);
  } catch (err) {
    if (exited) {
      throw new Error('Proxy process exited before port became available');
    }
    throw err;
  }

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
 * Get the path to the index.json file
 */
export function getInventoryPath(inventoryDir: string): string {
  return path.join(inventoryDir, 'index.json');
}
