import { useCallback, useEffect, useState } from 'react'
import { gatewayApi } from './api/gateway'
import { errorMessage, isUnauthorized } from './api/client'
import { isActiveExecution, type Execution } from './api/types'
import { pageLabel, toPageId, type PageId } from './app/navigation'
import { ToastProvider } from './components/feedback/ToastProvider'
import { SignInScreen } from './components/layout/SignInScreen'
import { Sidebar } from './components/layout/Sidebar'
import { Topbar } from './components/layout/Topbar'
import { useGatewayConfig } from './hooks/useGatewayConfig'
import { useGatewayData } from './hooks/useGatewayData'
import { useRoute } from './hooks/useRoute'
import { useTheme } from './hooks/useTheme'
import { useToast } from './hooks/useToast'
import { ConfigurationPage } from './pages/configuration/ConfigurationPage'
import { toSectionId } from './pages/configuration/sectionList'
import { DashboardPage } from './pages/dashboard/DashboardPage'
import { ExecutionDialog } from './pages/executions/ExecutionDialog'
import { ExecutionsPage } from './pages/executions/ExecutionsPage'
import { ToolsPage } from './pages/tools/ToolsPage'
import { UpstreamsPage } from './pages/upstreams/UpstreamsPage'

type Session = 'checking' | 'signed-in' | 'signed-out'

export default function App() {
  return (
    <ToastProvider>
      <GatewayConsole />
    </ToastProvider>
  )
}

function GatewayConsole() {
  const notify = useToast()
  const theme = useTheme()
  const { segments, navigate } = useRoute()
  const page = toPageId(segments[0])

  const [session, setSession] = useState<Session>('checking')
  const signedIn = session === 'signed-in'
  const handleUnauthorized = useCallback(() => setSession('signed-out'), [])

  const data = useGatewayData(signedIn, handleUnauthorized)
  const config = useGatewayConfig(signedIn, data.configEpoch)
  const [openExecution, setOpenExecution] = useState<Execution | null>(null)

  // A protected endpoint tells us whether a session exists (or sign-in is disabled).
  useEffect(() => {
    gatewayApi
      .metrics()
      .then(() => setSession('signed-in'))
      .catch(error => setSession(isUnauthorized(error) ? 'signed-out' : 'signed-in'))
  }, [])

  // Warn before closing the tab with unsaved settings.
  useEffect(() => {
    if (!config.dirty) return
    const onBeforeUnload = (event: BeforeUnloadEvent) => event.preventDefault()
    window.addEventListener('beforeunload', onBeforeUnload)
    return () => window.removeEventListener('beforeunload', onBeforeUnload)
  }, [config.dirty])

  const { refresh: refreshData } = data
  const refresh = useCallback(() => {
    refreshData().catch(error => notify('error', errorMessage(error, 'Could not refresh gateway data.')))
  }, [refreshData, notify])

  const showExecution = async (id: string) => {
    try {
      setOpenExecution(await gatewayApi.execution(id))
    } catch (error) {
      notify('error', errorMessage(error, 'Could not load the execution.'))
    }
  }

  const cancelExecution = async (id: string) => {
    try {
      await gatewayApi.cancelExecution(id)
      notify('info', 'Cancellation requested.')
      await data.refresh()
      if (openExecution?.id === id) setOpenExecution(await gatewayApi.execution(id))
    } catch (error) {
      notify('error', errorMessage(error, 'Could not cancel the execution.'))
    }
  }

  const signOut = async () => {
    await gatewayApi.signOut().catch(() => undefined)
    setSession('signed-out')
  }

  if (session === 'checking') return <div className="boot" aria-busy="true" />
  if (session === 'signed-out') return <SignInScreen onSignedIn={() => setSession('signed-in')} />

  const goTo = (target: PageId) => navigate(target)

  return (
    <div className="app">
      <Sidebar
        active={page}
        onNavigate={goTo}
        activeExecutions={data.executions.filter(isActiveExecution).length}
        connected={data.dataEpoch > 0}
        onSignOut={() => void signOut()}
      />
      <div className="main">
        <Topbar title={pageLabel(page)} theme={theme.preference} onThemeChange={theme.setPreference} onRefresh={refresh} />
        <main className="content">
          {page === 'dashboard' && (
            <DashboardPage upstreams={data.upstreams} executions={data.executions} metrics={data.metrics} onNavigate={goTo} onOpenExecution={id => void showExecution(id)} />
          )}
          {page === 'upstreams' && <UpstreamsPage upstreams={data.upstreams} config={config} onDataChanged={data.refresh} />}
          {page === 'tools' && <ToolsPage dataEpoch={data.dataEpoch} />}
          {page === 'executions' && (
            <ExecutionsPage
              executions={data.executions}
              retentionDays={config.saved?.observability.retention_days}
              onOpen={id => void showExecution(id)}
              onCancel={id => void cancelExecution(id)}
            />
          )}
          {page === 'configuration' && (
            <ConfigurationPage
              config={config}
              section={toSectionId(segments[1])}
              onSectionChange={section => navigate('configuration', section)}
              dataEpoch={data.dataEpoch}
              onApplied={data.refresh}
            />
          )}
        </main>
      </div>

      {openExecution && (
        <ExecutionDialog execution={openExecution} onClose={() => setOpenExecution(null)} onCancel={id => void cancelExecution(id)} />
      )}
    </div>
  )
}
