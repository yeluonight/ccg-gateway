import { invoke } from '@tauri-apps/api/core'
import type { Provider, ProviderCreate, ProviderUpdate } from '@/types/models'

export const providersApi = {
  list: async (cliType?: string): Promise<{ data: Provider[] }> => {
    const data = await invoke<Provider[]>('get_providers', { cliType })
    return { data }
  },
  get: async (id: number): Promise<{ data: Provider }> => {
    const data = await invoke<Provider>('get_provider', { id })
    return { data }
  },
  create: async (data: ProviderCreate): Promise<{ data: Provider }> => {
    const result = await invoke<Provider>('create_provider', { input: data })
    return { data: result }
  },
  update: async (id: number, data: ProviderUpdate): Promise<{ data: Provider }> => {
    const result = await invoke<Provider>('update_provider', { id, input: data })
    return { data: result }
  },
  delete: async (id: number) => {
    await invoke('delete_provider', { id })
    return { data: null }
  },
  reorder: async (ids: number[]) => {
    await invoke('reorder_providers', { ids })
    return { data: null }
  },
  resetFailures: async (id: number) => {
    await invoke('reset_provider_failures', { id })
    return { data: null }
  },
  unblacklist: async (id: number) => {
    await invoke('reset_provider_failures', { id })
    return { data: null }
  }
}
