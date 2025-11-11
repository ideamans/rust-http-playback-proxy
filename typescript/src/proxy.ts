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
   * Get the actual port number (may differ from requested port if 0 was used)
   */
  get port(): number {
    return this._port;
  }

  /**
   * Update the port number (used internally when OS assigns a port)
   */
  updatePort(port: number): void {
    this._port = port;
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
   * Node.js automatically converts to appropriate signal on Windows (CTRL_BREAK)
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

      // Send platform-appropriate signal:
      // Unix: SIGTERM (standard kill signal)
      // Windows: Use signal subcommand to send CTRL_BREAK via Windows API
      try {
        if (process.platform === 'win32') {
          // On Windows, use the signal subcommand to send CTRL_BREAK
          // The subcommand uses FreeConsole + AttachConsole + GenerateConsoleCtrlEvent
          // to properly deliver console control events to the target process
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

  // Set defaults to match CLI behavior
  const port = options.port !== undefined ? options.port : 18080;
  const deviceType = options.deviceType || 'mobile';
  const inventoryDir = options.inventoryDir || './inventory';

  // Build command
  const args: string[] = ['recording'];

  // Add entry URL if provided
  if (options.entryUrl) {
    args.push(options.entryUrl);
  }

  // Add port option (only if explicitly specified and not 0 or default)
  if (options.port !== undefined && options.port !== 0 && options.port !== 18080) {
    args.push('--port', port.toString());
  }

  // Add device type
  args.push('--device', deviceType);

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Note: control-port removed from recording mode - uses signal-based shutdown only

  // Start the process with piped stdout to capture port info
  const spawnOptions: any = {
    stdio: ['ignore', 'pipe', 'inherit'],
    detached: false,
  };

  if (process.platform === 'win32') {
    spawnOptions.windowsVerbatimArguments = false;
  }

  const proc = spawn(binaryPath, args, spawnOptions);

  const proxy = new Proxy('recording', port, inventoryDir, options.entryUrl, deviceType);
  proxy.setProcess(proc);

  // Capture stdout to extract actual port number when using port 0
  return new Promise((resolve, reject) => {
    let resolved = false;
    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        resolve(proxy);
      }
    }, 2000);

    proc.stdout?.on('data', (data: Buffer) => {
      const output = data.toString();
      // Look for "Recording proxy listening on 127.0.0.1:XXXXX" or "Playback proxy listening on 127.0.0.1:XXXXX"
      const match = output.match(/proxy listening on (?:127\.0\.0\.1|0\.0\.0\.0):(\d+)/i);
      if (match && match[1]) {
        const actualPort = parseInt(match[1], 10);
        proxy.updatePort(actualPort);
        if (!resolved) {
          resolved = true;
          clearTimeout(timeout);
          resolve(proxy);
        }
      }
      // Forward output to console
      process.stdout.write(data);
    });

    proc.on('error', (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(err);
      }
    });

    proc.on('exit', (code) => {
      if (!resolved && code !== 0) {
        resolved = true;
        clearTimeout(timeout);
        reject(new Error(`Proxy exited with code ${code}`));
      }
    });
  });
}

/**
 * Start a playback proxy
 */
export async function startPlayback(options: PlaybackOptions): Promise<Proxy> {
  await ensureBinary();

  const binaryPath = getFullBinaryPath();

  // Set defaults
  const port = options.port !== undefined ? options.port : 18080;
  const inventoryDir = options.inventoryDir || './inventory';

  // Verify inventory exists
  const inventoryPath = getInventoryPath(inventoryDir);
  if (!fs.existsSync(inventoryPath)) {
    throw new Error(`Inventory file not found at ${inventoryPath}`);
  }

  // Build command
  const args: string[] = ['playback'];

  // Add port option (only if not default)
  if (options.port !== undefined && options.port !== 18080) {
    args.push('--port', port.toString());
  }

  // Add inventory directory
  args.push('--inventory', inventoryDir);

  // Start the process with piped stdout to capture port info
  const spawnOptions: any = {
    stdio: ['ignore', 'pipe', 'inherit'],
    detached: false,
  };

  if (process.platform === 'win32') {
    spawnOptions.windowsVerbatimArguments = false;
  }

  const proc = spawn(binaryPath, args, spawnOptions);

  const proxy = new Proxy('playback', port, inventoryDir);
  proxy.setProcess(proc);

  // Capture stdout to extract actual port number when using port 0
  return new Promise((resolve, reject) => {
    let resolved = false;
    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        resolve(proxy);
      }
    }, 2000);

    proc.stdout?.on('data', (data: Buffer) => {
      const output = data.toString();
      // Look for "Playback proxy listening on 127.0.0.1:XXXXX"
      const match = output.match(/proxy listening on (?:127\.0\.0\.1|0\.0\.0\.0):(\d+)/i);
      if (match && match[1]) {
        const actualPort = parseInt(match[1], 10);
        proxy.updatePort(actualPort);
        if (!resolved) {
          resolved = true;
          clearTimeout(timeout);
          resolve(proxy);
        }
      }
      // Forward output to console
      process.stdout.write(data);
    });

    proc.on('error', (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(err);
      }
    });

    proc.on('exit', (code) => {
      if (!resolved && code !== 0) {
        resolved = true;
        clearTimeout(timeout);
        reject(new Error(`Proxy exited with code ${code}`));
      }
    });
  });
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
