import api from './instance'
import type { AllSettings, GatewaySettingsUpdate, TimeoutSettingsUpdate, CliSettingsUpdate, SystemStatus } from '@/types/models'

export const settingsApi = {
  getAll: () => api.get<AllSettings>('/settings'),
  updateGateway: (data: GatewaySettingsUpdate) => api.put('/settings/gateway', data),
  updateTimeouts: (data: TimeoutSettingsUpdate) => api.put('/settings/timeouts', data),
  updateCli: (cliType: string, data: CliSettingsUpdate) => api.put(`/settings/cli/${cliType}`, data),
  getStatus: () => api.get<SystemStatus>('/settings/status'),
  getVacuumStatus: () => api.get<{ mode: number; mode_name: string }>('/settings/db/vacuum-status'),
  migrateDatabase: () => api.post<{ message: string }>('/settings/db/migrate')
}
