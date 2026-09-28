import { useEffect, useState } from 'react'
import { Bot, FileCode2 } from 'lucide-react'
import { gatewayApi } from '../../../api/gateway'
import { Card, CardBody, CardHeader, CodeBlock } from '../../../components/ui'
import type { GatewayConfig } from '../../../config/types'
import { toYaml } from '../../../config/yaml'

type PreviewSectionProps = { draft: GatewayConfig; dataEpoch: number }

/** Read-only views: the YAML that Save will write, and the catalog text agents receive. */
export function PreviewSection({ draft, dataEpoch }: PreviewSectionProps) {
  const [catalog, setCatalog] = useState('Loading…')

  useEffect(() => {
    gatewayApi.catalogPreview().then(setCatalog).catch(() => setCatalog('Catalog preview is unavailable.'))
  }, [dataEpoch])

  return (
    <>
      <Card>
        <CardHeader icon={FileCode2} title="config.yaml" description="Exactly what Save writes to disk, including unsaved changes." />
        <CardBody>
          <CodeBlock title="YAML" code={toYaml(draft)} maxHeight={480} />
        </CardBody>
      </Card>
      <Card>
        <CardHeader icon={Bot} title="What agents see" description="The current tools_search description, listing each upstream namespace." />
        <CardBody>
          <CodeBlock title="tools_search description" code={catalog} maxHeight={360} />
        </CardBody>
      </Card>
    </>
  )
}
