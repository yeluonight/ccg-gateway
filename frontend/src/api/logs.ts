import api from './instance'
import type {
  RequestLogListResponse,
  RequestLogDetail,
  SystemLogListResponse,
  GatewaySettings,
  GatewaySettingsUpdate
} from '@/types/models'

export interface RequestLogQuery {
  page?: number
  page_size?: number
  cli_type?: string
  provider_name?: string
  success?: boolean
}

export interface SystemLogQuery {
  page?: number
  page_size?: number
  level?: string
  event_type?: string
  provider_name?: string
}

export const logsApi = {
  getSettings: () => api.get<GatewaySettings>('/logs/settings'),
  updateSettings: (data: GatewaySettingsUpdate) => api.put('/logs/settings', data),

  listRequestLogs: (params: RequestLogQuery) => api.get<RequestLogListResponse>('/logs/requests', { params }),
  getRequestLog: (id: number) => api.get<RequestLogDetail>(`/logs/requests/${id}`),
  clearRequestLogs: (before_timestamp?: number) => api.delete('/logs/requests', { data: { before_timestamp } }),

  listSystemLogs: (params: SystemLogQuery) => api.get<SystemLogListResponse>('/logs/system', { params }),
  clearSystemLogs: (before_timestamp?: number) => api.delete('/logs/system', { data: { before_timestamp } })
}
