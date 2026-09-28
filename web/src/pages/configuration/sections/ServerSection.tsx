import { KeyRound, Network } from 'lucide-react'
import { Callout, Card, CardBody, CardHeader, Switch, TextField } from '../../../components/ui'
import type { ServerConfig } from '../../../config/types'
import { isLoopbackListen } from '../../../config/validation'
import type { SectionProps } from './types'

export function ServerSection({ value, onChange, errors }: SectionProps<ServerConfig>) {
  const setAuth = (patch: Partial<ServerConfig['auth']>) => onChange({ ...value, auth: { ...value.auth, ...patch } })
  const remote = !errors['server.listen'] && !isLoopbackListen(value.listen)

  return (
    <>
      <Card>
        <CardHeader icon={Network} title="Network" description="Where the gateway listens for MCP clients and this console." />
        <CardBody>
          <div className="form-grid">
            <TextField
              label="Listen address"
              value={value.listen}
              onChange={listen => onChange({ ...value, listen })}
              placeholder="127.0.0.1:8080"
              monospace
              error={errors['server.listen']}
              hint="host:port. Keep 127.0.0.1 unless the gateway sits behind a TLS proxy."
            />
            <TextField
              label="MCP endpoint path"
              value={value.mcp_path}
              onChange={mcp_path => onChange({ ...value, mcp_path })}
              placeholder="/mcp"
              monospace
              error={errors['server.mcp_path']}
              hint={<>Clients connect to <code>http://{value.listen || 'host:port'}{value.mcp_path}</code>.</>}
            />
          </div>
          <Callout>Address and path changes take effect after the gateway restarts: <code>codemode service stop</code>, then <code>codemode service start</code>.</Callout>
          {remote && (
            <Callout tone="warning" title="Listening beyond localhost">
              The gateway refuses to start on a non-loopback address unless both authentication options are on and the
              environment sets <code>GATEWAY_TLS_TERMINATED=true</code>, <code>GATEWAY_ALLOWED_HOSTS</code> and <code>GATEWAY_ALLOWED_ORIGINS</code>.
            </Callout>
          )}
        </CardBody>
      </Card>

      <Card>
        <CardHeader icon={KeyRound} title="Authentication" description="Who may use the MCP endpoint and this console." />
        <CardBody>
          <Switch
            label="Require an API key from MCP clients"
            description={<>Clients send <code>Authorization: Bearer &lt;key&gt;</code>. Print the key with <code>codemode token client</code>.</>}
            checked={value.auth.client_enabled}
            onChange={client_enabled => setAuth({ client_enabled })}
            error={errors['server.auth.client_enabled']}
          />
          <Switch
            label="Require sign-in for the console and HTTP API"
            description="Uses the admin account managed with codemode admin setup / reset-password."
            checked={value.auth.admin_enabled}
            onChange={admin_enabled => setAuth({ admin_enabled })}
            error={errors['server.auth.admin_enabled']}
          />
          {!value.auth.admin_enabled && (
            <Callout tone="warning">Without sign-in, any local process that can reach the gateway can change its configuration.</Callout>
          )}
        </CardBody>
      </Card>
    </>
  )
}
