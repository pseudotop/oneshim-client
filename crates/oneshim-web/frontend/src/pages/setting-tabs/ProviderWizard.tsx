import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge, Button, Card, Input } from '../../components/ui'
import { colors, motion, radius, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

// Static SVG icon paths from @lobehub/icons-static-svg
const ICON_BASE = new URL('@lobehub/icons-static-svg/icons/', import.meta.url).href

interface ProviderDef {
  id: string
  name: string
  icon: string
  tier: 'recommended' | 'free' | 'local' | 'cloud' | 'oauth'
  tierLabel: string
  apiKeyEnv: string
  docsUrl: string
  placeholder: string
  surfaceId: string
  defaultModel: string
}

const PROVIDERS: ProviderDef[] = [
  {
    id: 'anthropic',
    name: 'Claude (Anthropic)',
    icon: 'anthropic.svg',
    tier: 'recommended',
    tierLabel: 'Recommended',
    apiKeyEnv: 'ANTHROPIC_API_KEY',
    docsUrl: 'https://console.anthropic.com/settings/keys',
    placeholder: 'sk-ant-...',
    surfaceId: 'provider_surface.anthropic.direct_api',
    defaultModel: 'claude-sonnet-4-20250514',
  },
  {
    id: 'openai',
    name: 'GPT (OpenAI)',
    icon: 'openai.svg',
    tier: 'recommended',
    tierLabel: 'Recommended',
    apiKeyEnv: 'OPENAI_API_KEY',
    docsUrl: 'https://platform.openai.com/api-keys',
    placeholder: 'sk-...',
    surfaceId: 'provider_surface.openai.direct_api',
    defaultModel: 'gpt-5.4',
  },
  {
    id: 'groq',
    name: 'Groq',
    icon: 'groq.svg',
    tier: 'free',
    tierLabel: 'Free tier',
    apiKeyEnv: 'GROQ_API_KEY',
    docsUrl: 'https://console.groq.com/keys',
    placeholder: 'gsk_...',
    surfaceId: 'provider_surface.groq.direct_api',
    defaultModel: 'llama-3.3-70b-versatile',
  },
  {
    id: 'ollama',
    name: 'Ollama',
    icon: 'ollama.svg',
    tier: 'local',
    tierLabel: 'Local',
    apiKeyEnv: '',
    docsUrl: 'https://ollama.com/download',
    placeholder: '',
    surfaceId: 'provider_surface.ollama.local_http',
    defaultModel: 'qwen3:8b',
  },
  {
    id: 'google',
    name: 'Gemini (Google)',
    icon: 'google-brand-color.svg',
    tier: 'cloud',
    tierLabel: 'Cloud',
    apiKeyEnv: 'GOOGLE_API_KEY',
    docsUrl: 'https://aistudio.google.com/apikey',
    placeholder: 'AIza...',
    surfaceId: 'provider_surface.google.direct_api',
    defaultModel: 'gemini-2.5-flash',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    icon: 'deepseek-color.svg',
    tier: 'cloud',
    tierLabel: 'Cloud',
    apiKeyEnv: 'DEEPSEEK_API_KEY',
    docsUrl: 'https://platform.deepseek.com/api_keys',
    placeholder: 'sk-...',
    surfaceId: 'provider_surface.deepseek.direct_api',
    defaultModel: 'deepseek-chat',
  },
  {
    id: 'mistral',
    name: 'Mistral AI',
    icon: 'mistral-color.svg',
    tier: 'cloud',
    tierLabel: 'Cloud',
    apiKeyEnv: 'MISTRAL_API_KEY',
    docsUrl: 'https://console.mistral.ai/api-keys',
    placeholder: '',
    surfaceId: 'provider_surface.mistral.direct_api',
    defaultModel: 'mistral-large-latest',
  },
  {
    id: 'xai',
    name: 'xAI (Grok)',
    icon: 'xai.svg',
    tier: 'cloud',
    tierLabel: 'Cloud',
    apiKeyEnv: 'XAI_API_KEY',
    docsUrl: 'https://console.x.ai/',
    placeholder: 'xai-...',
    surfaceId: 'provider_surface.xai.direct_api',
    defaultModel: 'grok-3',
  },
  {
    id: 'copilot',
    name: 'GitHub Copilot',
    icon: 'copilot-color.svg',
    tier: 'oauth',
    tierLabel: 'OAuth',
    apiKeyEnv: '',
    docsUrl: 'https://github.com/settings/copilot',
    placeholder: '',
    surfaceId: 'provider_surface.copilot.managed_oauth',
    defaultModel: 'gpt-5.4',
  },
  {
    id: 'bedrock',
    name: 'Amazon Bedrock',
    icon: 'bedrock-color.svg',
    tier: 'cloud',
    tierLabel: 'AWS',
    apiKeyEnv: 'AWS_ACCESS_KEY_ID',
    docsUrl: 'https://console.aws.amazon.com/bedrock',
    placeholder: 'AKIA...',
    surfaceId: 'provider_surface.bedrock.direct_api',
    defaultModel: 'anthropic.claude-3-5-sonnet-20241022-v2:0',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    icon: 'openrouter.svg',
    tier: 'cloud',
    tierLabel: 'Aggregator',
    apiKeyEnv: 'OPENROUTER_API_KEY',
    docsUrl: 'https://openrouter.ai/settings/keys',
    placeholder: 'sk-or-...',
    surfaceId: 'provider_surface.openrouter.direct_api',
    defaultModel: 'anthropic/claude-sonnet-4',
  },
  {
    id: 'nvidia',
    name: 'NVIDIA NIM',
    icon: 'nvidia-color.svg',
    tier: 'cloud',
    tierLabel: 'Cloud',
    apiKeyEnv: 'NVIDIA_API_KEY',
    docsUrl: 'https://build.nvidia.com/',
    placeholder: 'nvapi-...',
    surfaceId: 'provider_surface.nvidia.direct_api',
    defaultModel: 'nvidia/nemotron-3-super-120b',
  },
]

const TIER_COLORS: Record<string, string> = {
  recommended: colors.semantic.success,
  free: colors.semantic.info,
  local: 'bg-brand-signal/20 text-brand-text',
  cloud: colors.semantic.info,
  oauth: colors.semantic.warning,
}

function ProviderIcon({ icon, size = 28 }: { icon: string; size?: number }) {
  return <img src={`${ICON_BASE}${icon}`} alt="" width={size} height={size} className="shrink-0" loading="lazy" />
}

interface ProviderWizardProps {
  onSelect: (provider: ProviderDef, apiKey: string) => void
  className?: string
}

export default function ProviderWizard({ onSelect, className }: ProviderWizardProps) {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<ProviderDef | null>(null)
  const [apiKey, setApiKey] = useState('')
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<'idle' | 'success' | 'error'>('idle')
  const [showAll, setShowAll] = useState(false)

  const visibleProviders = showAll ? PROVIDERS : PROVIDERS.slice(0, 6)

  const handleTest = useCallback(async () => {
    if (!selected) return
    setTesting(true)
    setTestResult('idle')
    try {
      const resp = await fetch('/api/ai/providers/models', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          surface_id: selected.surfaceId,
          endpoint: null,
          api_key: apiKey || null,
        }),
      })
      setTestResult(resp.ok ? 'success' : 'error')
    } catch {
      setTestResult('error')
    } finally {
      setTesting(false)
    }
  }, [selected, apiKey])

  const handleSave = useCallback(() => {
    if (!selected) return
    onSelect(selected, apiKey)
    setSelected(null)
    setApiKey('')
    setTestResult('idle')
  }, [selected, apiKey, onSelect])

  // Step 2: Setup form for selected provider
  if (selected) {
    const needsApiKey = selected.tier !== 'local' && selected.tier !== 'oauth'

    return (
      <Card className={cn('relative', className)}>
        <button
          type="button"
          onClick={() => {
            setSelected(null)
            setApiKey('')
            setTestResult('idle')
          }}
          className={cn(
            'absolute top-4 right-4 px-2 py-1 text-xs',
            radius.sm,
            colors.text.secondary,
            'hover:bg-surface-hover',
          )}
        >
          Back
        </button>

        <div className="mb-6 flex items-center gap-3">
          <ProviderIcon icon={selected.icon} size={36} />
          <div>
            <h3 className={typography.h3}>{selected.name}</h3>
            <Badge className={cn('mt-1 text-[10px]', TIER_COLORS[selected.tier])}>{selected.tierLabel}</Badge>
          </div>
        </div>

        {needsApiKey && (
          <div className="mb-4">
            <label htmlFor="wizard-api-key" className={cn('mb-1.5 block text-sm', colors.text.secondary)}>
              API Key
            </label>
            <Input
              id="wizard-api-key"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={selected.placeholder}
              className={cn('w-full', typography.family.mono)}
            />
            <a
              href={selected.docsUrl}
              target="_blank"
              rel="noopener noreferrer"
              className={cn('mt-1.5 inline-block text-xs', colors.primary.text)}
            >
              Get API key &rarr;
            </a>
          </div>
        )}

        {selected.tier === 'local' && (
          <div className={cn('mb-4 p-3 text-sm', radius.md, 'bg-surface-hover')}>
            <p className={colors.text.secondary}>
              Make sure Ollama is running on <code className={typography.family.mono}>localhost:11434</code>
            </p>
            <a
              href={selected.docsUrl}
              target="_blank"
              rel="noopener noreferrer"
              className={cn('mt-1 inline-block text-xs', colors.primary.text)}
            >
              Download Ollama &rarr;
            </a>
          </div>
        )}

        {selected.tier === 'oauth' && (
          <div className={cn('mb-4 p-3 text-sm', radius.md, 'bg-surface-hover')}>
            <p className={colors.text.secondary}>
              GitHub Copilot requires OAuth authentication. Connect via the advanced settings below.
            </p>
          </div>
        )}

        <div className="mb-4">
          <label htmlFor="wizard-model" className={cn('mb-1.5 block text-sm', colors.text.secondary)}>
            Default Model
          </label>
          <Input
            id="wizard-model"
            type="text"
            value={selected.defaultModel}
            readOnly
            className={cn('w-full bg-surface-hover', typography.family.mono)}
          />
        </div>

        <div className="flex items-center gap-2">
          {needsApiKey && (
            <Button variant="secondary" onClick={handleTest} disabled={!apiKey || testing} className="text-sm">
              {testing ? 'Testing...' : 'Test Connection'}
            </Button>
          )}

          {testResult === 'success' && <span className={cn('text-xs', colors.semantic.success)}>Connected</span>}
          {testResult === 'error' && (
            <span className={cn('text-xs', colors.semantic.error)}>Connection failed — check your API key</span>
          )}

          <div className="flex-1" />

          <Button variant="primary" onClick={handleSave} disabled={needsApiKey && !apiKey} className="text-sm">
            Save &amp; Activate
          </Button>
        </div>
      </Card>
    )
  }

  // Step 1: Provider picker grid
  return (
    <Card className={className}>
      <h3 className={cn(typography.h3, 'mb-1')}>{t('settings.ai.quickSetup', 'Quick Setup')}</h3>
      <p className={cn('mb-4 text-sm', colors.text.secondary)}>
        {t('settings.ai.quickSetupDesc', 'Choose a provider to get started. You can change this anytime.')}
      </p>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {visibleProviders.map((provider) => (
          <button
            key={provider.id}
            type="button"
            onClick={() => setSelected(provider)}
            className={cn(
              'flex items-center gap-2.5 p-3 text-left',
              radius.md,
              'border border-border-default',
              'hover:border-brand hover:bg-surface-hover',
              motion.colors,
            )}
          >
            <ProviderIcon icon={provider.icon} />
            <div className="min-w-0">
              <div className={cn('truncate', typography.label, colors.text.primary)}>{provider.name}</div>
              <Badge className={cn('mt-0.5 text-[9px]', TIER_COLORS[provider.tier])}>{provider.tierLabel}</Badge>
            </div>
          </button>
        ))}
      </div>

      {!showAll && PROVIDERS.length > 6 && (
        <button
          type="button"
          onClick={() => setShowAll(true)}
          className={cn('mt-3 text-xs', colors.primary.text, 'hover:underline')}
        >
          Show all {PROVIDERS.length} providers &rarr;
        </button>
      )}
    </Card>
  )
}

export type { ProviderDef }
