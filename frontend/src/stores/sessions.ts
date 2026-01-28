import { defineStore } from 'pinia'
import { ref } from 'vue'
import { sessionsApi, type ProjectInfo, type SessionInfo, type SessionMessage } from '@/api/sessions'
import type { CliType } from '@/types/models'
import { useUiStore } from './ui'

export const useSessionStore = defineStore('sessions', () => {
  const projects = ref<ProjectInfo[]>([])
  const sessions = ref<SessionInfo[]>([])
  const messages = ref<SessionMessage[]>([])
  const loading = ref(false)
  const currentProject = ref<string>('')
  const currentProjectInfo = ref<ProjectInfo | null>(null)
  const currentSession = ref<string>('')

  // Pagination state
  const projectPage = ref(1)
  const projectTotal = ref(0)
  const sessionPage = ref(1)
  const sessionTotal = ref(0)
  const pageSize = ref(20)

  async function fetchProjects(page?: number, cliType?: CliType) {
    loading.value = true
    if (page !== undefined) {
      projectPage.value = page
    }
    try {
      const uiStore = useUiStore()
      const type = cliType || uiStore.sessionsActiveCliType
      const { data } = await sessionsApi.listProjects(type, projectPage.value, pageSize.value)
      projects.value = data.items
      projectTotal.value = data.total
    } catch (error: any) {
      console.error('Failed to fetch projects:', error)
      projects.value = []
      projectTotal.value = 0
    } finally {
      loading.value = false
    }
  }

  async function fetchSessions(projectName: string, page?: number, projectInfo?: ProjectInfo, cliType?: CliType) {
    loading.value = true
    currentProject.value = projectName
    if (projectInfo) {
      currentProjectInfo.value = projectInfo
    }
    if (page !== undefined) {
      sessionPage.value = page
    }
    try {
      const uiStore = useUiStore()
      const type = cliType || uiStore.sessionsActiveCliType
      const { data } = await sessionsApi.listSessions(type, projectName, sessionPage.value, pageSize.value)
      sessions.value = data.items
      sessionTotal.value = data.total
    } catch (error: any) {
      console.error('Failed to fetch sessions:', error)
      sessions.value = []
      sessionTotal.value = 0
    } finally {
      loading.value = false
    }
  }

  async function fetchMessages(projectName: string, sessionId: string, cliType?: CliType) {
    loading.value = true
    currentSession.value = sessionId
    try {
      const uiStore = useUiStore()
      const type = cliType || uiStore.sessionsActiveCliType
      const { data } = await sessionsApi.getSessionMessages(type, projectName, sessionId)
      messages.value = data
    } catch (error: any) {
      console.error('Failed to fetch messages:', error)
      messages.value = []
    } finally {
      loading.value = false
    }
  }

  async function deleteSession(projectName: string, sessionId: string, cliType?: CliType) {
    const uiStore = useUiStore()
    const type = cliType || uiStore.sessionsActiveCliType
    await sessionsApi.deleteSession(type, projectName, sessionId)
    sessions.value = sessions.value.filter(s => s.session_id !== sessionId)
    sessionTotal.value = Math.max(0, sessionTotal.value - 1)
  }

  async function deleteProject(projectName: string, cliType?: CliType) {
    const uiStore = useUiStore()
    const type = cliType || uiStore.sessionsActiveCliType
    await sessionsApi.deleteProject(type, projectName)
    projects.value = projects.value.filter(p => p.name !== projectName)
    projectTotal.value = Math.max(0, projectTotal.value - 1)
  }

  function clearMessages() {
    messages.value = []
    currentSession.value = ''
  }

  function clearSessions() {
    sessions.value = []
    currentProject.value = ''
    currentProjectInfo.value = null
    sessionPage.value = 1
    sessionTotal.value = 0
  }

  return {
    projects,
    sessions,
    messages,
    loading,
    currentProject,
    currentProjectInfo,
    currentSession,
    projectPage,
    projectTotal,
    sessionPage,
    sessionTotal,
    pageSize,
    fetchProjects,
    fetchSessions,
    fetchMessages,
    deleteSession,
    deleteProject,
    clearMessages,
    clearSessions
  }
})
