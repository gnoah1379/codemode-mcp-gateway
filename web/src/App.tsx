import { useCallback, useEffect, useState } from 'react'
import {
  Activity, ArrowDownToLine, ArrowRight, Ban, Check, ChevronDown, CircleHelp,
  Code2, Database, FileCode2, Gauge, GitBranch, LayoutDashboard,
  LoaderCircle, LockKeyhole, LogOut, Play, Plus, RefreshCw, Search, Server,
  Settings2, Shield, Terminal, X,
} from 'lucide-react'
import { parse, stringify } from 'yaml'

type Tab = 'Dashboard' | 'Upstreams' | 'Tools / Search' | 'Configuration / Policy' | 'Executions'
type Upstream = { name:string; description:string; transport:string; enabled:boolean; available:boolean; tool_count:number; last_error?:string|null }
type Tool = { namespace:string; name:string; description:string; inputSchema:unknown; outputSchema?:unknown; available:boolean; allowed:boolean; matchedPattern?:string|null }
type Execution = { id:string; status:string; startedAt:number; finishedAt?:number|null; durationMs?:number|null; codeBytes:number; outputBytes:number; errorCode?:string|null; revision:string; calls:Call[] }
type Call = { toolName:string; decision:string; matchedPattern?:string|null; durationMs:number; bytes:number; errorCode?:string|null }
type ApiError = { error?:{code?:string;message?:string} }
type Metrics = { executions:{total:number;succeeded:number;failed:number;timedOut:number;cancelled:number;active:number;averageDurationMs:number;outputBytes:number};toolCalls:number;tools:{discovered:number;available:number};upstreams:{total:number;healthy:number} }
type UpstreamDraft = { namespace:string;description:string;transport:'stdio'|'streamable_http';command:string;args:string;envKey:string;secretEnv:string;url:string;headerName:string }

const tabs: { name:Tab; icon:typeof Activity }[] = [
  { name:'Dashboard', icon:LayoutDashboard },
  { name:'Upstreams', icon:GitBranch },
  { name:'Tools / Search', icon:Search },
  { name:'Configuration / Policy', icon:Settings2 },
  { name:'Executions', icon:Activity },
]

async function api<T>(path:string, init:RequestInit = {}):Promise<T> {
  const headers = new Headers(init.headers)
  if (init.body && !(init.body instanceof FormData)) headers.set('content-type', headers.get('content-type') ?? 'application/json')
  if (init.method && !['GET','HEAD','OPTIONS'].includes(init.method.toUpperCase())) {
    const csrf = document.cookie.split('; ').find(item => item.startsWith('gateway_csrf='))?.split('=').slice(1).join('=')
    if (csrf) headers.set('x-csrf-token', decodeURIComponent(csrf))
  }
  const response = await fetch(path, { ...init, headers, credentials:'same-origin' })
  if (!response.ok) {
    let detail = `Request failed (${response.status})`
    try { const body = await response.json() as ApiError; detail = body.error?.message ?? detail } catch { /* response body is not JSON */ }
    throw new Error(detail)
  }
  if (response.status === 204) return undefined as T
  const contentType = response.headers.get('content-type') ?? ''
  return (contentType.includes('yaml') || contentType.includes('text/plain') ? await response.text() : await response.json()) as T
}

function App() {
  const [tab,setTab] = useState<Tab>('Dashboard')
  const [connected,setConnected] = useState(false)
  const [showLogin,setShowLogin] = useState(false)
  const [adminToken,setAdminToken] = useState('')
  const [busy,setBusy] = useState(false)
  const [toast,setToast] = useState('')
  const [upstreams,setUpstreams] = useState<Upstream[]>([])
  const [tools,setTools] = useState<Tool[]>([])
  const [toolTotal,setToolTotal] = useState(0)
  const [refreshKey,setRefreshKey] = useState(0)
  const [executions,setExecutions] = useState<Execution[]>([])
  const [metrics,setMetrics] = useState<Metrics>({executions:{total:0,succeeded:0,failed:0,timedOut:0,cancelled:0,active:0,averageDurationMs:0,outputBytes:0},toolCalls:0,tools:{discovered:0,available:0},upstreams:{total:0,healthy:0}})
  const [config,setConfig] = useState('')
  const [revision,setRevision] = useState('')
  const [query,setQuery] = useState('')
  const [selectedExecution,setSelectedExecution] = useState<Execution|null>(null)
  const [policyName,setPolicyName] = useState('')
  const [policyResult,setPolicyResult] = useState<{allowed:boolean;matchedPattern?:string|null}|null>(null)

  const refresh = useCallback(async () => {
    const [upstreamData,executionData,metricData] = await Promise.all([
      api<{upstreams:Upstream[]}>('/api/v1/upstreams'),
      api<{items:Execution[]}>('/api/v1/executions?limit=100'),
      api<Metrics>('/api/v1/metrics'),
    ])
    setUpstreams(upstreamData.upstreams); setExecutions(executionData.items); setMetrics(metricData); setRefreshKey(value=>value+1)
  },[])
  const searchTools = useCallback(async (search:string,page:number) => {
    try {
      const result = await api<{items:Tool[];total:number}>(`/api/v1/tools?query=${encodeURIComponent(search)}&page=${page}&page_size=200`)
      setTools(result.items); setToolTotal(result.total)
    } catch (error) { setToast(error instanceof Error ? error.message : 'Tool search failed.') }
  },[])

  useEffect(() => {
    let mounted = true
    refresh().then(() => mounted && setConnected(true)).catch(() => mounted && setShowLogin(true))
    const events = new EventSource('/api/v1/events')
    events.onmessage = () => { void refresh().catch(() => undefined) }
    for (const name of ['execution.queued','execution.started','execution.finished','execution.call','config.changed','upstreams.changed','resync_required']) events.addEventListener(name, () => { void refresh().catch(() => undefined) })
    return () => { mounted = false; events.close() }
  },[refresh])

  useEffect(() => {
    if (tab !== 'Configuration / Policy' || config) return
    void loadConfig()
  },[tab,config])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(''),3500)
    return () => window.clearTimeout(timer)
  },[toast])

  const runningCount = executions.filter(execution => ['running','queued'].includes(execution.status)).length

  async function connect() {
    setBusy(true)
    try {
      await fetch('/api/v1/session',{method:'POST',headers:{authorization:`Bearer ${adminToken}`}}).then(async response => {
        if (!response.ok) throw new Error('Could not start an admin session. Check the token and try again.')
      })
      await refresh(); setConnected(true); setShowLogin(false); setToast('Connected to the gateway.')
    } catch (error) { setToast(error instanceof Error ? error.message : 'Connection failed.') }
    finally { setBusy(false) }
  }

  async function loadConfig() {
    setBusy(true)
    try {
      const response = await fetch('/api/v1/config',{credentials:'same-origin'})
      if (!response.ok) throw new Error('Could not load YAML configuration.')
      setConfig(await response.text()); setRevision(response.headers.get('etag') ?? '')
    } catch (error) { setToast(error instanceof Error ? error.message : 'Could not load configuration.') }
    finally { setBusy(false) }
  }
  async function saveConfig() {
    setBusy(true)
    try {
      const response = await fetch('/api/v1/config',{method:'PUT',headers:{'content-type':'application/yaml','if-match':revision,...csrfHeaders()},body:config,credentials:'same-origin'})
      const body = await response.json() as {revision?:string;error?:{message?:string}}
      if (!response.ok) throw new Error(body.error?.message ?? 'Could not save configuration.')
      setRevision(body.revision ?? ''); await refresh(); setToast('Configuration saved and active.')
    } catch (error) { setToast(error instanceof Error ? error.message : 'Could not save configuration.') }
    finally { setBusy(false) }
  }
  async function addUpstream(draft:UpstreamDraft):Promise<boolean> {
    setBusy(true)
    try {
      const current = await fetch('/api/v1/config',{credentials:'same-origin'})
      if (!current.ok) throw new Error('Could not load the current YAML configuration.')
      const etag = current.headers.get('etag')
      if (!etag) throw new Error('Gateway did not provide a configuration revision.')
      const document = parse(await current.text()) as Record<string,any>
      document.upstreams ??= {}
      if (Object.prototype.hasOwnProperty.call(document.upstreams,draft.namespace)) throw new Error('That namespace already exists.')
      const secretMap = draft.envKey && draft.secretEnv ? {[draft.envKey]:{from_env:draft.secretEnv}} : {}
      let transport:Record<string,unknown>
      if (draft.transport === 'stdio') {
        if (Boolean(draft.envKey) !== Boolean(draft.secretEnv)) throw new Error('Enter both the child environment key and its gateway secret reference.')
        const args = JSON.parse(draft.args || '[]') as unknown
        if (!Array.isArray(args) || !args.every(value => typeof value === 'string')) throw new Error('Arguments must be a JSON array of strings.')
        transport = {type:'stdio',command:draft.command,args,env:secretMap}
      } else {
        if (Boolean(draft.headerName) !== Boolean(draft.secretEnv)) throw new Error('Enter both the HTTP header name and its environment variable reference.')
        transport = {type:'streamable_http',url:draft.url,headers:draft.headerName ? {[draft.headerName]:{from_env:draft.secretEnv}} : {}}
      }
      document.upstreams[draft.namespace] = {enabled:true,description:draft.description,transport}
      const yaml = stringify(document)
      const response = await fetch('/api/v1/config',{method:'PUT',headers:{'content-type':'application/yaml','if-match':etag,...csrfHeaders()},body:yaml,credentials:'same-origin'})
      const body = await response.json() as {revision?:string;error?:{message?:string}}
      if (!response.ok) throw new Error(body.error?.message ?? 'Could not save the upstream.')
      setConfig(yaml); setRevision(body.revision ?? response.headers.get('etag') ?? '')
      await refresh(); setToast(`${draft.namespace} added and activated.`)
      return true
    } catch (error) { setToast(error instanceof Error ? error.message : 'Could not add upstream.'); return false }
    finally { setBusy(false) }
  }
  async function validateConfig() {
    setBusy(true)
    try {
      const result = await api<{valid:boolean;degradedUpstreams:string[]}>('/api/v1/config/validate',{method:'POST',headers:{'content-type':'application/yaml',...csrfHeaders()},body:config})
      setToast(result.valid ? `YAML is valid${result.degradedUpstreams.length ? ` · ${result.degradedUpstreams.length} upstream(s) degraded` : ''}.` : 'Configuration is invalid.')
    } catch (error) { setToast(error instanceof Error ? error.message : 'Validation failed.') }
    finally { setBusy(false) }
  }
  async function reloadConfig() {
    setBusy(true)
    try { await api('/api/v1/config/reload',{method:'POST',body:'{}'}); await loadConfig(); await refresh(); setToast('Configuration reloaded.') }
    catch (error) { setToast(error instanceof Error ? error.message : 'Reload failed.') }
    finally { setBusy(false) }
  }
  async function upstreamAction(name:string,action:'test'|'refresh') {
    setBusy(true)
    try { const result = await api<{available?:boolean;toolCount?:number}>(`/api/v1/upstreams/${encodeURIComponent(name)}/${action}`,{method:'POST',body:'{}'}); await refresh(); setToast(action === 'test' ? `Connection ${result.available ? 'healthy' : 'unavailable'} · ${result.toolCount ?? 0} tools.` : `${name} catalog refreshed.`) }
    catch (error) { setToast(error instanceof Error ? error.message : 'Upstream action failed.') }
    finally { setBusy(false) }
  }
  async function evaluatePolicy() {
    try { const result = await api<{allowed:boolean;matchedPattern?:string|null}>('/api/v1/policy/evaluate',{method:'POST',body:JSON.stringify({name:policyName})}); setPolicyResult(result) }
    catch (error) { setToast(error instanceof Error ? error.message : 'Policy evaluation failed.') }
  }
  async function cancelExecution(id:string) {
    try { await api(`/api/v1/executions/${encodeURIComponent(id)}/cancel`,{method:'POST',body:'{}'}); await refresh(); setToast('Cancellation requested.') }
    catch (error) { setToast(error instanceof Error ? error.message : 'Could not cancel execution.') }
  }
  async function openExecution(id:string) {
    try { const record = await api<Execution>(`/api/v1/executions/${encodeURIComponent(id)}`); setSelectedExecution(record) }
    catch (error) { setToast(error instanceof Error ? error.message : 'Could not load execution.') }
  }
  async function logout() {
    try { await api('/api/v1/session',{method:'DELETE'}); setConnected(false); setShowLogin(true) } catch { setConnected(false); setShowLogin(true) }
  }

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><Code2 size={19}/></div><div><strong>code mode</strong><span>gateway console</span></div></div>
      <div className="workspace-label">WORKSPACE</div>
      <button className="workspace-select"><span className="workspace-dot"/>Local gateway<ChevronDown size={14}/></button>
      <nav>{tabs.map(item => { const Icon = item.icon; return <button key={item.name} className={`nav-item ${tab === item.name ? 'active' : ''}`} onClick={() => setTab(item.name)}><Icon size={17}/><span>{item.name}</span>{item.name === 'Executions' && runningCount > 0 && <b>{runningCount}</b>}</button> })}</nav>
      <div className="sidebar-bottom">
        <div className="version"><span className={`status-dot ${connected ? 'online' : ''}`}/><span>{connected ? 'Gateway connected' : 'Connecting'}</span><button className="icon-button" title="Refresh data" onClick={() => refresh().catch(e => setToast(e.message))}><RefreshCw size={14}/></button></div>
        <div className="user-card"><div className="avatar">GM</div><div className="user-meta"><b>Gateway admin</b><span>Owner access</span></div><button className="icon-button" title="End session" onClick={logout}><LogOut size={15}/></button></div>
      </div>
    </aside>
    <main className="main-panel">
      <header className="topbar"><div className="breadcrumbs"><span>Gateway</span><span className="crumb-slash">/</span><strong>{tab}</strong></div><div className="top-actions"><span className="environment"><span className="status-dot online"/>LOCAL</span><button className="icon-button" title="Help"><CircleHelp size={17}/></button><button className="icon-button" title="Refresh" onClick={() => refresh().catch(e => setToast(e.message))}><RefreshCw size={16}/></button></div></header>
      <div className="content">
        {tab === 'Dashboard' && <Dashboard upstreams={upstreams} executions={executions} metrics={metrics} runningCount={runningCount} onTab={setTab} onExecution={openExecution}/>}
        {tab === 'Upstreams' && <UpstreamsPage upstreams={upstreams} busy={busy} onAction={upstreamAction} onAdd={addUpstream}/>}
        {tab === 'Tools / Search' && <ToolsPage query={query} setQuery={setQuery} tools={tools} total={toolTotal} refreshKey={refreshKey} onSearch={searchTools}/>}
        {tab === 'Configuration / Policy' && <ConfigPage config={config} setConfig={setConfig} busy={busy} onSave={saveConfig} onValidate={validateConfig} onReload={reloadConfig} policyName={policyName} setPolicyName={setPolicyName} policyResult={policyResult} onEvaluate={evaluatePolicy} upstreams={upstreams}/>}
        {tab === 'Executions' && <ExecutionsPage executions={executions} onOpen={openExecution} onCancel={cancelExecution}/>}
      </div>
    </main>
    {toast && <div className="toast"><Check size={16}/>{toast}<button onClick={() => setToast('')}><X size={14}/></button></div>}
    {showLogin && <div className="modal-backdrop"><div className="login-card"><div className="brand-mark large"><LockKeyhole size={20}/></div><h2>Connect to your gateway</h2><p>Enter the admin token configured by your local deployment.</p><label>Admin token<input autoFocus type="password" value={adminToken} onChange={e => setAdminToken(e.target.value)} placeholder="Token from GATEWAY_ADMIN_TOKEN" onKeyDown={e => e.key === 'Enter' && void connect()}/></label><button className="primary-button full" onClick={connect} disabled={busy}>{busy ? <LoaderCircle className="spin" size={16}/> : <ArrowRight size={16}/>}Start admin session</button><span className="hint">On a loopback-only gateway without a token, leave this field blank.</span></div></div>}
    {selectedExecution && <ExecutionModal record={selectedExecution} onClose={() => setSelectedExecution(null)} onCancel={cancelExecution}/>}
  </div>
}

function csrfHeaders():Record<string,string> { const csrf = document.cookie.split('; ').find(item => item.startsWith('gateway_csrf='))?.split('=').slice(1).join('='); return csrf ? {'x-csrf-token':decodeURIComponent(csrf)} : {} }

function PageHeading({eyebrow,title,description,action}:{eyebrow:string;title:string;description:string;action?:React.ReactNode}) { return <div className="page-heading"><div><div className="eyebrow">{eyebrow}</div><h1>{title}</h1><p>{description}</p></div>{action}</div> }
function Dashboard({upstreams,executions,metrics,runningCount,onTab,onExecution}:{upstreams:Upstream[];executions:Execution[];metrics:Metrics;runningCount:number;onTab:(tab:Tab)=>void;onExecution:(id:string)=>void}) {
  const recent = executions.slice(0,5)
  return <>
    <PageHeading eyebrow="OVERVIEW" title="Gateway overview" description="Your MCP tools, execution health, and upstream connections at a glance." action={<button className="secondary-button" onClick={() => onTab('Configuration / Policy')}><Settings2 size={15}/>Configure gateway</button>}/>
    <div className="stat-grid">
      <Stat icon={GitBranch} label="Upstreams" value={`${metrics.upstreams.healthy} / ${metrics.upstreams.total}`} caption="enabled and healthy" tone="mint"/>
      <Stat icon={Database} label="Available tools" value={String(metrics.tools.available)} caption={`of ${metrics.tools.discovered} discovered tools`} tone="blue"/>
      <Stat icon={Activity} label="Active executions" value={String(runningCount)} caption="running or queued" tone="orange"/>
      <Stat icon={Shield} label="Policy posture" value="Enforced" caption="checked on every call" tone="violet"/>
    </div>
    <div className="dashboard-grid">
      <section className="panel upstream-panel"><div className="panel-heading"><div><h2>Upstream connections</h2><p>Live discovery and availability</p></div><button className="text-button" onClick={() => onTab('Upstreams')}>Manage <ArrowRight size={14}/></button></div>
        {upstreams.length === 0 ? <Empty icon={GitBranch} title="No upstreams configured" text="Add a stdio or Streamable HTTP server in Configuration." action={() => onTab('Configuration / Policy')}/> : <div className="upstream-list">{upstreams.slice(0,5).map(upstream=><UpstreamLine key={upstream.name} upstream={upstream}/>)}</div>}
      </section>
      <section className="panel activity-panel"><div className="panel-heading"><div><h2>Recent executions</h2><p>Latest sandbox activity</p></div><button className="text-button" onClick={() => onTab('Executions')}>View all <ArrowRight size={14}/></button></div>
        {recent.length === 0 ? <Empty icon={Terminal} title="No executions yet" text="Tool executions will appear here when a client calls tools_execute."/> : <div className="activity-list">{recent.map(row=><button key={row.id} className="activity-row" onClick={() => onExecution(row.id)}><StatusDot status={row.status}/><span className="activity-copy"><b>{row.status === 'succeeded' ? 'Execution completed' : row.errorCode ?? row.status}</b><small>{timeAgo(row.startedAt)} · {row.calls.length} tool calls</small></span><span className="duration">{row.durationMs != null ? `${row.durationMs} ms` : '—'}</span></button>)}</div>}
      </section>
    </div>
    <section className="panel quick-panel"><div className="panel-heading"><div><h2>Quick links</h2><p>Common gateway tasks</p></div></div><div className="quick-links"><QuickLink icon={Server} title="Inspect upstreams" text="Test connections and refresh catalogs" onClick={() => onTab('Upstreams')}/><QuickLink icon={Search} title="Find a tool" text="Search tool names and schemas" onClick={() => onTab('Tools / Search')}/><QuickLink icon={Shield} title="Review policy" text="Evaluate an exact tool name" onClick={() => onTab('Configuration / Policy')}/></div></section>
    <section className="panel metrics-panel"><div className="panel-heading"><div><h2>Execution metrics</h2><p>Persisted audit totals within the retention window</p></div><span className="code-pill">SQLite</span></div><div className="metrics-row"><Metric label="Total executions" value={String(metrics.executions.total)}/><Metric label="Succeeded" value={String(metrics.executions.succeeded)}/><Metric label="Failed / timed out" value={String(metrics.executions.failed+metrics.executions.timedOut)}/><Metric label="Tool calls" value={String(metrics.toolCalls)}/><Metric label="Average duration" value={`${Math.round(metrics.executions.averageDurationMs)} ms`}/><Metric label="Output recorded" value={formatBytes(metrics.executions.outputBytes)}/></div></section>
  </>
}
function Stat({icon:Icon,label,value,caption,tone}:{icon:typeof Activity;label:string;value:string;caption:string;tone:string}) { return <div className="stat-card"><div className={`stat-icon ${tone}`}><Icon size={18}/></div><span className="stat-label">{label}</span><strong>{value}</strong><small>{caption}</small></div> }
function Metric({label,value}:{label:string;value:string}) { return <div className="metric-cell"><small>{label}</small><b>{value}</b></div> }
function UpstreamLine({upstream}:{upstream:Upstream}) { return <div className="upstream-line"><StatusDot status={!upstream.enabled?'disabled':upstream.available?'available':'unavailable'}/><span className="upstream-name"><b>{upstream.name}</b><small>{upstream.description}</small></span><span className="tool-count">{upstream.tool_count} tools</span><span className={`pill ${upstream.available?'success':'muted'}`}>{upstream.enabled?(upstream.available?'Healthy':'Degraded'):'Disabled'}</span></div> }
function Empty({icon:Icon,title,text,action}:{icon:typeof Activity;title:string;text:string;action?:()=>void}) { return <div className="empty-state"><div className="empty-icon"><Icon size={19}/></div><b>{title}</b><p>{text}</p>{action&&<button className="text-button" onClick={action}>Open configuration <ArrowRight size={13}/></button>}</div> }
function QuickLink({icon:Icon,title,text,onClick}:{icon:typeof Activity;title:string;text:string;onClick:()=>void}) { return <button className="quick-link" onClick={onClick}><span className="quick-icon"><Icon size={17}/></span><span><b>{title}</b><small>{text}</small></span><ArrowRight className="quick-arrow" size={15}/></button> }

function UpstreamsPage({upstreams,busy,onAction,onAdd}:{upstreams:Upstream[];busy:boolean;onAction:(name:string,action:'test'|'refresh')=>void;onAdd:(draft:UpstreamDraft)=>Promise<boolean>}) {
  const [showForm,setShowForm]=useState(false)
  return <><PageHeading eyebrow="CONNECTIONS" title="Upstreams" description="Manage and monitor the MCP servers connected to this gateway." action={<button className="secondary-button" onClick={() => setShowForm(true)}><Plus size={15}/>Add upstream</button>}/>
    <div className="notice"><CircleHelp size={16}/><span>Credentials stay on the gateway host. The form writes environment variable references to YAML; secret values remain on the gateway host.</span></div>
    <div className="panel table-panel"><div className="table-toolbar"><div><b>{upstreams.length} upstream{upstreams.length === 1 ? '' : 's'}</b><span> · discovery snapshot</span></div><span className="live-indicator"><span className="status-dot online"/>Live status</span></div>
      {upstreams.length ? <div className="table-scroll"><table><thead><tr><th>UPSTREAM</th><th>TRANSPORT</th><th>STATUS</th><th>TOOLS</th><th>DESCRIPTION</th><th></th></tr></thead><tbody>{upstreams.map(u=><tr key={u.name}><td><div className="table-name"><StatusDot status={!u.enabled?'disabled':u.available?'available':'unavailable'}/><b>{u.name}</b></div></td><td><span className="code-pill">{u.transport==='streamable_http'?'Streamable HTTP':'stdio'}</span></td><td><span className={`pill ${u.available?'success':'muted'}`}>{!u.enabled?'Disabled':u.available?'Healthy':'Degraded'}</span></td><td>{u.tool_count}</td><td className="description-cell">{u.description}</td><td><div className="row-actions"><button className="small-button" onClick={()=>onAction(u.name,'test')} disabled={busy}><Play size={13}/>Test</button><button className="icon-button" title="Refresh tool catalog" onClick={()=>onAction(u.name,'refresh')} disabled={busy}><RefreshCw size={14}/></button></div></td></tr>)}</tbody></table></div> : <Empty icon={GitBranch} title="No upstreams configured" text="Add an upstream to make tools discoverable through the gateway."/>}
    </div>
    <div className="two-note-grid"><div className="note-card"><span className="note-icon green"><LockKeyhole size={16}/></span><div><b>Secrets stay on the host</b><p>YAML stores environment variable references. Secret values are read only by the host broker.</p></div></div><div className="note-card"><span className="note-icon blue"><RefreshCw size={16}/></span><div><b>Refresh is safe</b><p>Discovery reads the complete paginated tool catalog and swaps it atomically.</p></div></div></div>
    {showForm&&<UpstreamForm busy={busy} onClose={()=>setShowForm(false)} onSave={async draft=>{if(await onAdd(draft))setShowForm(false)}}/>}
  </>
}

function UpstreamForm({busy,onClose,onSave}:{busy:boolean;onClose:()=>void;onSave:(draft:UpstreamDraft)=>void}) {
  const [draft,setDraft]=useState<UpstreamDraft>({namespace:'',description:'',transport:'stdio',command:'',args:'[]',envKey:'',secretEnv:'',url:'',headerName:''})
  const update=(key:keyof UpstreamDraft,value:string)=>setDraft(previous=>({...previous,[key]:value}))
  return <div className="modal-backdrop" onMouseDown={event=>event.target===event.currentTarget&&onClose()}><form className="upstream-modal" onSubmit={event=>{event.preventDefault();onSave(draft)}}><div className="modal-heading"><div><div className="eyebrow">NEW CONNECTION</div><h2>Add upstream</h2></div><button type="button" className="icon-button" onClick={onClose}><X size={17}/></button></div><div className="upstream-form-grid">
    <label className="form-field">Namespace<input required pattern="[a-z][a-z0-9_-]*" maxLength={64} value={draft.namespace} onChange={event=>update('namespace',event.target.value)} placeholder="github"/><small>Stable lowercase prefix used in tool names.</small></label>
    <label className="form-field">Description<input required maxLength={512} value={draft.description} onChange={event=>update('description',event.target.value)} placeholder="Repository and issue operations"/><small>Shown in the MCP search catalog.</small></label>
    <label className="form-field">Transport<select value={draft.transport} onChange={event=>update('transport',event.target.value)}><option value="stdio">stdio</option><option value="streamable_http">Streamable HTTP</option></select></label>
    {draft.transport==='stdio'?<>
      <label className="form-field">Executable path<input required value={draft.command} onChange={event=>update('command',event.target.value)} placeholder="/usr/local/bin/github-mcp-server"/><small>Spawned directly; shell syntax is not interpreted.</small></label>
      <label className="form-field">Arguments (JSON array)<input value={draft.args} onChange={event=>update('args',event.target.value)} placeholder={'["stdio"]'}/></label>
      <label className="form-field">Child environment key<input value={draft.envKey} onChange={event=>update('envKey',event.target.value)} placeholder="GITHUB_TOKEN"/></label>
      <label className="form-field">Secret environment reference<input value={draft.secretEnv} onChange={event=>update('secretEnv',event.target.value)} placeholder="GITHUB_TOKEN"/><small>Only the variable name is written to YAML.</small></label>
    </>:<>
      <label className="form-field">MCP URL<input required type="url" value={draft.url} onChange={event=>update('url',event.target.value)} placeholder="https://mcp.example.com/mcp"/></label>
      <label className="form-field">Authentication header<input value={draft.headerName} onChange={event=>update('headerName',event.target.value)} placeholder="Authorization"/></label>
      <label className="form-field">Secret environment reference<input value={draft.secretEnv} onChange={event=>update('secretEnv',event.target.value)} placeholder="UPSTREAM_AUTH"/><small>Set the header value in the gateway environment.</small></label>
    </>}
  </div><div className="modal-footer"><span>Saved to the active YAML configuration.</span><div><button type="button" className="secondary-button" onClick={onClose}>Cancel</button><button className="primary-button" disabled={busy}><Plus size={14}/>Add upstream</button></div></div></form></div>
}

function ToolsPage({query,setQuery,tools,total,refreshKey,onSearch}:{query:string;setQuery:(value:string)=>void;tools:Tool[];total:number;refreshKey:number;onSearch:(query:string,page:number)=>void}) {
  const [expanded,setExpanded]=useState<string|null>(null)
  const [page,setPage]=useState(1)
  useEffect(()=>{const timer=window.setTimeout(()=>onSearch(query.trim(),page),160);return()=>window.clearTimeout(timer)},[query,page,refreshKey,onSearch])
  return <><PageHeading eyebrow="CATALOG" title="Tools & search" description="Search the available MCP catalog and inspect the schemas clients receive."/>
    <div className="search-box"><Search size={17}/><input value={query} onChange={e=>{setQuery(e.target.value);setPage(1)}} placeholder="Search by operation, namespace, or description…"/><kbd>⌘ K</kbd></div>
    <div className="result-meta"><span><b>{total}</b> matching catalog result{total===1?'':'s'}</span><span>Keyword provider <ChevronDown size={13}/></span></div>
    <div className="tool-results">{tools.length ? tools.map(tool=>{const id=`${tool.namespace}.${tool.name}`;const open=expanded===id;return <article className="tool-card" key={id}><button className="tool-summary" onClick={()=>setExpanded(open?null:id)}><span className="tool-glyph"><Code2 size={16}/></span><span className="tool-title"><span><b>{tool.name}</b><span className="namespace-tag">{tool.namespace}</span></span><small>{tool.description || 'No description provided by upstream.'}</small></span><span className="tool-card-status"><span className={`pill ${tool.allowed&&tool.available?'success':'muted'}`}>{!tool.available?'Unavailable':tool.allowed?'Allowed':'Denied'}</span><ChevronDown className={open?'rotated':''} size={15}/></span></button>{open&&<div className="schema-grid"><Schema label="Input schema" value={tool.inputSchema}/>{Boolean(tool.outputSchema)&&<Schema label="Output schema" value={tool.outputSchema}/>}</div>}</article>}) : <Empty icon={Search} title="No matching tools" text="Try another query or configure an upstream MCP server."/>}</div>
    {total>200&&<div className="catalog-pagination"><button className="secondary-button" onClick={()=>setPage(value=>Math.max(1,value-1))} disabled={page===1}>Previous</button><span>Page {page} of {Math.ceil(total/200)}</span><button className="secondary-button" onClick={()=>setPage(value=>Math.min(Math.ceil(total/200),value+1))} disabled={page>=Math.ceil(total/200)}>Next</button></div>}
  </>
}
function Schema({label,value}:{label:string;value:unknown}) { return <div className="schema-block"><div className="schema-heading"><b>{label}</b><button className="icon-button" title="Copy schema" onClick={()=>void navigator.clipboard?.writeText(JSON.stringify(value,null,2))}><ArrowDownToLine size={13}/></button></div><pre>{JSON.stringify(value,null,2)}</pre></div> }

function ConfigPage({config,setConfig,busy,onSave,onValidate,onReload,policyName,setPolicyName,policyResult,onEvaluate,upstreams}:{config:string;setConfig:(value:string)=>void;busy:boolean;onSave:()=>void;onValidate:()=>void;onReload:()=>void;policyName:string;setPolicyName:(value:string)=>void;policyResult:{allowed:boolean;matchedPattern?:string|null}|null;onEvaluate:()=>void;upstreams:Upstream[]}) {
  const [catalog,setCatalog]=useState('')
  let currentSearchProvider='keyword',currentSandboxProvider='deno'
  try { const doc=parse(config) as Record<string,any>; currentSearchProvider=doc.search?.provider??'keyword'; currentSandboxProvider=doc.sandbox?.provider??'deno' } catch { /* keep the available provider defaults while YAML is being edited */ }
  function changeProvider(section:'search'|'sandbox',provider:string) {
    try { const doc=parse(config) as Record<string,any>; doc[section]??={}; doc[section].provider=provider; setConfig(stringify(doc)) }
    catch { /* invalid YAML remains editable in the main editor */ }
  }
  useEffect(()=>{api<{description:string}>('/api/v1/catalog-preview').then(value=>setCatalog(value.description)).catch(()=>setCatalog('Catalog preview unavailable.'))},[upstreams])
  return <><PageHeading eyebrow="SETTINGS" title="Configuration & policy" description="The YAML file is the single source of truth for upstreams, search, sandbox, and access rules." action={<button className="secondary-button" onClick={onReload} disabled={busy}><RefreshCw size={15}/>Reload file</button>}/>
    <div className="config-layout"><section className="panel editor-panel"><div className="panel-heading"><div><h2><FileCode2 size={16}/> Active YAML config</h2><p>Edit settings and persist them to disk.</p></div><span className="code-pill">YAML</span></div><textarea className="yaml-editor" spellCheck={false} value={config} onChange={e=>setConfig(e.target.value)} placeholder="Loading configuration…"/><div className="editor-footer"><span>Changes are validated before activation.</span><div><button className="secondary-button" onClick={onValidate} disabled={busy}>Validate</button><button className="primary-button" onClick={onSave} disabled={busy}><Check size={15}/>Save configuration</button></div></div></section>
      <div className="config-side"><section className="panel provider-panel"><div className="panel-heading"><div><h2><Settings2 size={16}/>Provider settings</h2><p>Choose an implemented provider; validate options in YAML.</p></div></div><label className="field-label">Search provider<select className="provider-select" value={currentSearchProvider} onChange={event=>changeProvider('search',event.target.value)}>{currentSearchProvider!=='keyword'&&<option value={currentSearchProvider}>{currentSearchProvider} · unsupported</option>}<option value="keyword">keyword</option></select></label><label className="field-label">Sandbox provider<select className="provider-select" value={currentSandboxProvider} onChange={event=>changeProvider('sandbox',event.target.value)}>{currentSandboxProvider!=='deno'&&<option value={currentSandboxProvider}>{currentSandboxProvider} · unsupported</option>}<option value="deno">deno</option></select></label><div className="provider-note">Provider-specific options are strictly validated when you select Validate.</div></section>
        <section className="panel policy-panel"><div className="panel-heading"><div><h2><Shield size={16}/>Policy preview</h2><p>Check the current decision for one tool.</p></div></div><label className="field-label">Full tool name<input value={policyName} onChange={e=>setPolicyName(e.target.value)} placeholder="github.delete_issue" onKeyDown={e=>e.key==='Enter'&&onEvaluate()}/></label><button className="secondary-button full" onClick={onEvaluate} disabled={!policyName}><Gauge size={14}/>Evaluate policy</button>{policyResult&&<div className={`policy-result ${policyResult.allowed?'allow':'deny'}`}><span>{policyResult.allowed?<Check size={15}/>:<Ban size={15}/>} {policyResult.allowed?'Allowed':'Denied'}</span><small>{policyResult.matchedPattern?`Matched ${policyResult.matchedPattern}`:'No pattern matched · default policy applied'}</small></div>}</section>
        <section className="panel mini-panel"><div className="panel-heading"><div><h2>Configuration state</h2><p>Current discovery status</p></div></div><div className="mini-stats"><div><b>{upstreams.length}</b><span>upstreams</span></div><div><b>{upstreams.reduce((sum,item)=>sum+item.tool_count,0)}</b><span>discovered tools</span></div></div><div className="mini-list">{upstreams.slice(0,4).map(item=><div key={item.name}><StatusDot status={item.available?'available':'unavailable'}/><span>{item.name}</span><small>{item.available?'ready':'degraded'}</small></div>)}</div></section>
      </div></div>
    <section className="notice catalog-notice"><Terminal size={16}/><div><b>Catalog preview</b><p>Current <code>tools_search</code> description, including the namespace XML sent to MCP clients.</p><pre>{catalog||'Loading catalog preview…'}</pre></div></section>
  </>
}

function ExecutionsPage({executions,onOpen,onCancel}:{executions:Execution[];onOpen:(id:string)=>void;onCancel:(id:string)=>void}) {
  return <><PageHeading eyebrow="OBSERVABILITY" title="Executions" description="Review execution status and tool call audit metadata. Code and payloads are not stored." action={<span className="live-indicator"><span className="status-dot online"/>Live updates</span>}/>
    <div className="panel table-panel"><div className="table-toolbar"><div><b>{executions.length} executions</b><span> · most recent first</span></div><span className="code-pill">7 day retention</span></div>{executions.length?<div className="table-scroll"><table><thead><tr><th>EXECUTION</th><th>STATUS</th><th>TOOL CALLS</th><th>DURATION</th><th>STARTED</th><th></th></tr></thead><tbody>{executions.map(row=><tr key={row.id}><td><button className="id-button" onClick={()=>onOpen(row.id)}>{row.id.slice(0,8)}…</button></td><td><span className={`status-label ${row.status}`}><StatusDot status={row.status}/>{row.status}</span></td><td>{row.calls.length}</td><td>{row.durationMs!=null?`${row.durationMs} ms`:'—'}</td><td>{timeAgo(row.startedAt)}</td><td>{['queued','running'].includes(row.status)&&<button className="small-button danger" onClick={()=>onCancel(row.id)}><X size={13}/>Cancel</button>}</td></tr>)}</tbody></table></div>:<Empty icon={Activity} title="No executions recorded" text="A client execution appears here after calling tools_execute."/>}</div>
  </>
}
function ExecutionModal({record,onClose,onCancel}:{record:Execution;onClose:()=>void;onCancel:(id:string)=>void}) { return <div className="modal-backdrop" onMouseDown={e=>e.target===e.currentTarget&&onClose()}><div className="execution-modal"><div className="modal-heading"><div><div className="eyebrow">EXECUTION DETAIL</div><h2>{record.id}</h2></div><button className="icon-button" onClick={onClose}><X size={17}/></button></div><div className="modal-kpis"><div><small>STATUS</small><b><StatusDot status={record.status}/>{record.status}</b></div><div><small>DURATION</small><b>{record.durationMs==null?'—':`${record.durationMs} ms`}</b></div><div><small>OUTPUT</small><b>{formatBytes(record.outputBytes)}</b></div><div><small>ERROR</small><b>{record.errorCode??'—'}</b></div></div><div className="audit-heading"><b>Tool call timeline</b><span>{record.calls.length} calls</span></div>{record.calls.length?record.calls.map((call,index)=><div className="call-row" key={`${call.toolName}-${index}`}><StatusDot status={call.decision}/><code>{call.toolName}</code><span className={`pill ${call.decision==='allow'?'success':'muted'}`}>{call.decision}</span><small>{call.matchedPattern??''}</small><span>{call.durationMs} ms</span></div>):<div className="empty-inline">No tool calls were recorded.</div>}<div className="modal-footer"><span>Arguments and results are omitted from the audit log.</span>{['queued','running'].includes(record.status)&&<button className="small-button danger" onClick={()=>onCancel(record.id)}>Cancel execution</button>}</div></div></div> }

function StatusDot({status}:{status:string}) { const tone=['available','healthy','succeeded','allow','online'].includes(status)?'online':['unavailable','failed','timed_out','deny','error'].includes(status)?'bad':status==='running'?'running':''; return <span className={`status-dot ${tone}`} /> }
function timeAgo(timestamp:number) { if(!timestamp)return'—'; const ms=timestamp<1e12?timestamp*1000:timestamp; const seconds=Math.max(0,Math.floor((Date.now()-ms)/1000)); if(seconds<60)return'just now'; if(seconds<3600)return`${Math.floor(seconds/60)}m ago`; if(seconds<86400)return`${Math.floor(seconds/3600)}h ago`; return new Date(ms).toLocaleDateString() }
function formatBytes(bytes:number) { if(bytes<1024)return`${bytes} B`; if(bytes<1024*1024)return`${(bytes/1024).toFixed(1)} KB`; return`${(bytes/1024/1024).toFixed(1)} MB` }

export default App
