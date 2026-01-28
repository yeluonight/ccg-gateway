import { invoke } from '@tauri-apps/api/core'

export interface WebdavSettings {
  url: string
  username: string
  password: string
}

export interface WebdavBackup {
  filename: string
  size: number
  modified: string
}

export const getWebdavSettings = async (): Promise<{ data: WebdavSettings }> => {
  const data = await invoke<WebdavSettings>('get_webdav_settings')
  return { data }
}

export const updateWebdavSettings = async (data: Partial<WebdavSettings>): Promise<{ data: WebdavSettings }> => {
  const result = await invoke<WebdavSettings>('update_webdav_settings', { input: data })
  return { data: result }
}

export const testWebdavConnection = async (data: WebdavSettings): Promise<{ data: { success: boolean } }> => {
  const success = await invoke<boolean>('test_webdav_connection', {
    url: data.url,
    username: data.username,
    password: data.password
  })
  return { data: { success } }
}

export const exportToLocal = async (): Promise<Blob> => {
  const data = await invoke<number[]>('export_to_local')
  return new Blob([new Uint8Array(data)], { type: 'application/octet-stream' })
}

export const importFromLocal = async (file: File): Promise<{ data: { success: boolean; message: string } }> => {
  const arrayBuffer = await file.arrayBuffer()
  const data = Array.from(new Uint8Array(arrayBuffer))
  await invoke('import_from_local', { data })
  return { data: { success: true, message: 'Database imported successfully' } }
}

export const exportToWebdav = async (): Promise<{ data: { success: boolean; filename: string } }> => {
  const filename = await invoke<string>('export_to_webdav')
  return { data: { success: true, filename } }
}

export const listWebdavBackups = async (): Promise<{ data: { backups: WebdavBackup[] } }> => {
  const backups = await invoke<WebdavBackup[]>('list_webdav_backups')
  return { data: { backups } }
}

export const importFromWebdav = async (filename: string): Promise<{ data: { success: boolean; message: string } }> => {
  await invoke('import_from_webdav', { filename })
  return { data: { success: true, message: 'Database imported successfully' } }
}
