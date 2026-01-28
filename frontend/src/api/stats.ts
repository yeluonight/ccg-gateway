import { invoke } from '@tauri-apps/api/core'
import type { DailyStats, ProviderStats } from '@/types/models'

export const statsApi = {
  getDaily: async (params?: { start_date?: string; end_date?: string; cli_type?: string; provider_name?: string }): Promise<{ data: DailyStats[] }> => {
    const data = await invoke<DailyStats[]>('get_daily_stats', {
      startDate: params?.start_date,
      endDate: params?.end_date,
      cliType: params?.cli_type
    })
    return { data }
  },
  getProviders: async (params?: { start_date?: string; end_date?: string }): Promise<{ data: ProviderStats[] }> => {
    const data = await invoke<ProviderStats[]>('get_provider_stats', {
      startDate: params?.start_date,
      endDate: params?.end_date
    })
    return { data }
  }
}
