/**
 * Tauri API bridge - Simplified for desktop-only mode
 * This project now only supports Tauri desktop applications
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core'

/**
 * Check if running in Tauri environment
 */
export const isTauri = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI__' in window
}

/**
 * Invoke Tauri command
 * All API calls now go through Tauri IPC
 */
export const invoke = tauriInvoke
