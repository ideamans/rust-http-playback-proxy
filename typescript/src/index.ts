/**
 * HTTP Playback Proxy - TypeScript/Node.js Wrapper
 *
 * A wrapper for the HTTP Playback Proxy Rust binary that provides:
 * - Recording HTTP traffic with accurate timing
 * - Playing back recorded traffic with precise timing control
 * - Inventory management for recorded resources
 */

export {
  Proxy,
  startRecording,
  startPlayback,
  loadInventory,
  saveInventory,
  getResourceContentPath,
  getInventoryPath,
} from './proxy';

export {
  ensureBinary,
  checkBinaryExists,
  downloadBinary,
  getVersion,
  getPlatform,
  getBinaryName,
} from './binary';

export type {
  DeviceType,
  ContentEncodingType,
  HttpHeaders,
  Resource,
  Inventory,
  RecordingOptions,
  PlaybackOptions,
  ProxyMode,
} from './types';
