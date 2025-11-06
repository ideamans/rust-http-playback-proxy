import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import * as https from 'https';
import { createWriteStream } from 'fs';
import { pipeline } from 'stream/promises';
import * as tar from 'tar';

const GITHUB_USER = 'pagespeed-quest';
const GITHUB_REPO = 'http-playback-proxy';
const BASE_URL = `https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/download`;

/**
 * Get the version of this TypeScript wrapper
 */
export function getVersion(): string {
  try {
    const packageJsonPath = path.join(__dirname, '..', 'package.json');
    const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
    return packageJson.version;
  } catch {
    return '0.0.0';
  }
}

/**
 * Get the current platform identifier
 */
export function getPlatform(): string {
  const platform = process.platform;
  const arch = process.arch;

  if (platform === 'darwin') {
    return arch === 'arm64' ? 'darwin-arm64' : 'darwin-amd64';
  } else if (platform === 'linux') {
    return arch === 'arm64' ? 'linux-arm64' : 'linux-amd64';
  } else if (platform === 'win32') {
    return 'windows-amd64';
  }

  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

/**
 * Get the binary file name for the current platform
 */
export function getBinaryName(): string {
  return process.platform === 'win32' ? 'http-playback-proxy.exe' : 'http-playback-proxy';
}

/**
 * Get the expected path to the binary relative to package root
 */
export function getBinaryPath(): string {
  const platform = getPlatform();
  return path.join('bin', platform, getBinaryName());
}

/**
 * Get the cache directory for downloaded binaries
 */
export function getCacheDir(): string {
  // Check environment variable first
  const envCache = process.env.HTTP_PLAYBACK_PROXY_CACHE_DIR;
  if (envCache) {
    return envCache;
  }

  // Try XDG_CACHE_HOME
  const xdgCache = process.env.XDG_CACHE_HOME;
  if (xdgCache) {
    return path.join(xdgCache, 'http-playback-proxy');
  }

  // Fall back to user's home directory
  return path.join(os.homedir(), '.cache', 'http-playback-proxy');
}

/**
 * Get the package root directory
 * In development: typescript/dist/ -> typescript/
 * In production: node_modules/http-playback-proxy/dist/ -> node_modules/http-playback-proxy/
 */
export function getPackageRoot(): string {
  // __dirname in compiled code (dist/) points to dist/
  // So ../ goes to package root
  const packageRoot = path.join(__dirname, '..');

  // Verify this is actually the package root by checking for package.json
  const packageJsonPath = path.join(packageRoot, 'package.json');
  if (fs.existsSync(packageJsonPath)) {
    try {
      const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
      if (packageJson.name === 'http-playback-proxy') {
        return packageRoot;
      }
    } catch {
      // Fall through to return default path
    }
  }

  // Fallback to simple relative path
  return packageRoot;
}

/**
 * Check if the binary exists
 */
export function checkBinaryExists(): boolean {
  // Check package directory first
  const packagePath = path.join(getPackageRoot(), getBinaryPath());
  if (fs.existsSync(packagePath)) {
    return true;
  }

  // Check cache directory
  const cachePath = path.join(getCacheDir(), getBinaryPath());
  if (fs.existsSync(cachePath)) {
    return true;
  }

  return false;
}

/**
 * Download the binary from GitHub Releases
 */
export async function downloadBinary(version?: string): Promise<void> {
  const ver = version || getVersion();
  const platform = getPlatform();
  const archiveName = `http-playback-proxy-v${ver}-${platform}.tar.gz`;
  const url = `${BASE_URL}/v${ver}/${archiveName}`;

  // Try to download to cache directory
  const cacheDir = getCacheDir();
  let targetDir = cacheDir;

  try {
    fs.mkdirSync(cacheDir, { recursive: true });
  } catch (err) {
    console.warn(`Could not create cache directory, using package directory: ${err}`);
    targetDir = getPackageRoot();
  }

  console.log(`Downloading http-playback-proxy binary for ${platform}...`);
  console.log(`URL: ${url}`);
  console.log(`Target: ${targetDir}`);

  // Download the tar.gz archive
  const tmpFile = path.join(os.tmpdir(), archiveName);
  const tmpStream = createWriteStream(tmpFile);

  await new Promise<void>((resolve, reject) => {
    https.get(url, (response) => {
      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download binary: HTTP ${response.statusCode}`));
        return;
      }

      response.pipe(tmpStream);

      tmpStream.on('finish', () => {
        tmpStream.close();
        resolve();
      });

      tmpStream.on('error', reject);
    }).on('error', reject);
  });

  // Extract the tar.gz archive
  const binDir = path.join(targetDir, 'bin', platform);
  fs.mkdirSync(binDir, { recursive: true });

  await tar.extract({
    file: tmpFile,
    cwd: binDir,
  });

  // Make binary executable on Unix-like systems
  if (process.platform !== 'win32') {
    const binaryPath = path.join(binDir, getBinaryName());
    fs.chmodSync(binaryPath, 0o755);
  }

  // Clean up temp file
  fs.unlinkSync(tmpFile);

  console.log(`Successfully downloaded and extracted binary to ${targetDir}`);
}

/**
 * Ensure the binary is available, downloading if necessary
 */
export async function ensureBinary(): Promise<void> {
  if (checkBinaryExists()) {
    return;
  }

  console.log('Pre-built binary not found. Attempting to download from GitHub Releases...');

  try {
    await downloadBinary();
  } catch (err) {
    throw new Error(`Failed to download binary v${getVersion()}: ${err}`);
  }
}

/**
 * Get the full path to the binary
 */
export function getFullBinaryPath(): string {
  // Check package directory first
  const packagePath = path.join(getPackageRoot(), getBinaryPath());
  if (fs.existsSync(packagePath)) {
    return packagePath;
  }

  // Check cache directory
  const cachePath = path.join(getCacheDir(), getBinaryPath());
  if (fs.existsSync(cachePath)) {
    return cachePath;
  }

  throw new Error('Binary not found, please call ensureBinary() first');
}
