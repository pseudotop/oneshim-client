import { ChevronDown, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ProviderSurfaceSpec } from '../../api/contracts'
import { Badge, Button, Input, Select } from '../../components/ui'
import { defaultSurfaceModel } from '../../features/providerSurfaces'
import { colors, iconSize, interaction, motion, radius, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { STATE_DOT } from './constants'
import type { SessionInfo, Transport } from './types'

interface ChatSidebarProps {
  sessions: SessionInfo[]
  activeId: string | null
  transport: Transport
  setTransport: (t: Transport) => void
  httpApiSurfaces: ProviderSurfaceSpec[]
  selectedHttpSurface: ProviderSurfaceSpec | null
  setHttpSurfaceId: (id: string) => void
  modelOverride: string
  setModelOverride: (v: string) => void
  systemPrompt: string
  setSystemPrompt: (v: string) => void
  showAdvanced: boolean
  setShowAdvanced: (v: boolean | ((p: boolean) => boolean)) => void
  creating: boolean
  createDisabled: boolean
  onRefresh: () => void
  onSelectSession: (id: string) => void
  onCreate: () => void
  onDelete: (id: string) => void
  isHistorical: (s: SessionInfo) => boolean
}

export function ChatSidebar({
  sessions,
  activeId,
  transport,
  setTransport,
  httpApiSurfaces,
  selectedHttpSurface,
  setHttpSurfaceId,
  modelOverride,
  setModelOverride,
  systemPrompt,
  setSystemPrompt,
  showAdvanced,
  setShowAdvanced,
  creating,
  createDisabled,
  onRefresh,
  onSelectSession,
  onCreate,
  onDelete,
  isHistorical,
}: ChatSidebarProps) {
  const { t } = useTranslation()

  return (
    <div className="flex w-64 shrink-0 flex-col border-muted border-r bg-surface-base">
      <div className="flex items-center justify-between border-muted border-b px-3 py-2">
        <span className={cn(typography.label, colors.text.primary)}>{t('chat.title')}</span>
        <Button variant="ghost" size="sm" onClick={onRefresh}>
          <RefreshCw className={iconSize.sm} />
        </Button>
      </div>
      <div className="flex items-center gap-1 border-muted border-b px-2 py-2">
        <Select
          selectSize="sm"
          value={transport}
          onChange={(e) => setTransport(e.target.value as Transport)}
          className="flex-1 text-xs"
        >
          <option value="subprocess">Subprocess</option>
          <option value="http_api">HTTP API</option>
          <option value="local_llm">Local LLM</option>
        </Select>
        <Button variant="primary" size="sm" onClick={onCreate} isLoading={creating} disabled={createDisabled}>
          <Plus className={iconSize.sm} />
        </Button>
      </div>

      {/* Advanced settings toggle */}
      <button
        type="button"
        onClick={() => setShowAdvanced((p: boolean) => !p)}
        className={cn(
          'flex items-center gap-1 border-muted border-b px-3 py-1.5 text-xs',
          interaction.interactive,
          colors.text.secondary,
        )}
      >
        <ChevronDown className={cn(iconSize.xs, motion.transform, showAdvanced && 'rotate-180')} />
        {t('chat.advanced')}
      </button>
      {showAdvanced && (
        <div className="space-y-3 border-muted border-b px-2 py-2">
          {transport === 'http_api' && (
            <div className="space-y-1">
              <p
                className={cn(
                  'text-[10px] uppercase tracking-[0.12em]',
                  typography.weight.medium,
                  colors.text.secondary,
                )}
              >
                {t('chat.http_surface_label')}
              </p>
              <Select
                selectSize="sm"
                value={selectedHttpSurface?.surface_id ?? ''}
                onChange={(e) => setHttpSurfaceId(e.target.value)}
                className="w-full text-xs"
              >
                {httpApiSurfaces.map((surface) => (
                  <option key={surface.surface_id} value={surface.surface_id}>
                    {surface.display_name}
                  </option>
                ))}
              </Select>
              <p className={cn('text-[10px]', colors.text.secondary)}>{t('chat.http_surface_help')}</p>
            </div>
          )}
          <div className="space-y-1">
            <p
              className={cn('text-[10px] uppercase tracking-[0.12em]', typography.weight.medium, colors.text.secondary)}
            >
              {t('chat.model_label')}
            </p>
            <Input
              value={modelOverride}
              onChange={(e) => setModelOverride(e.target.value)}
              placeholder={
                transport === 'http_api'
                  ? (defaultSurfaceModel(selectedHttpSurface ?? undefined, 'llm_api') ?? t('chat.model_placeholder'))
                  : t('chat.model_placeholder')
              }
              className="text-xs"
            />
          </div>
          <textarea
            value={systemPrompt}
            onChange={(e) => setSystemPrompt(e.target.value)}
            placeholder={t('chat.system_prompt_placeholder')}
            rows={3}
            className={cn(
              'w-full resize-none border bg-surface-base px-2 py-1.5 text-xs placeholder-content-tertiary',
              radius.md,
              interaction.focusRing,
              colors.text.primary,
              'border-DEFAULT focus:border-brand-signal',
            )}
          />
        </div>
      )}

      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <p className={cn('px-3 py-4 text-center text-xs', colors.text.secondary)}>{t('chat.no_sessions')}</p>
        ) : (
          sessions.map((s) => (
            <button
              key={s.session_id}
              type="button"
              onClick={() => onSelectSession(s.session_id)}
              className={cn(
                'group flex w-full items-center gap-2 px-3 py-2 text-left',
                interaction.interactive,
                activeId === s.session_id ? 'bg-surface-elevated' : 'hover:bg-hover',
              )}
            >
              <span className={cn('h-2 w-2 shrink-0 rounded-full', STATE_DOT[s.state] ?? 'bg-status-disconnected')} />
              <div className="min-w-0 flex-1">
                <p className={cn('truncate text-xs', typography.weight.medium, colors.text.primary)}>
                  {s.model || s.provider_name}
                </p>
                <p className={cn('truncate text-[10px]', colors.text.secondary)}>
                  {s.transport} -- {s.turn_count} {t('chat.turns')}
                </p>
              </div>
              {isHistorical(s) ? (
                <Badge size="xs" className="bg-surface-muted text-content-secondary">
                  {t('chat.history', 'History')}
                </Badge>
              ) : null}
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation()
                  onDelete(s.session_id)
                }}
                className="hidden text-content-muted hover:text-semantic-error group-hover:block"
              >
                <Trash2 className={iconSize.xs} />
              </button>
            </button>
          ))
        )}
      </div>
    </div>
  )
}
