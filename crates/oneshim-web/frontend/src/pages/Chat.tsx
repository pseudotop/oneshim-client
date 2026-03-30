import {
  AlertTriangle,
  Bot,
  Check,
  ChevronDown,
  Copy,
  Loader2,
  MessageSquarePlus,
  Paperclip,
  Plus,
  RefreshCw,
  Search,
  Send,
  Trash2,
  User,
  Wrench,
  X,
} from 'lucide-react'
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { DEFAULT_PROVIDER_SURFACE_CATALOG } from '../api/defaultProviderSurfaceCatalog'

// Lazy-loaded syntax highlighter — only fetched when a fenced code block is rendered
const LazySyntaxHighlighter = React.lazy(async () => {
  const [
    { default: SyntaxHighlighter },
    { oneDark },
    javascript,
    typescript,
    python,
    bash,
    jsonLang,
    cssLang,
    rust,
    sql,
    yaml,
    markdownLang,
  ] = await Promise.all([
    import('react-syntax-highlighter/dist/esm/prism-light'),
    import('react-syntax-highlighter/dist/esm/styles/prism'),
    import('react-syntax-highlighter/dist/esm/languages/prism/javascript'),
    import('react-syntax-highlighter/dist/esm/languages/prism/typescript'),
    import('react-syntax-highlighter/dist/esm/languages/prism/python'),
    import('react-syntax-highlighter/dist/esm/languages/prism/bash'),
    import('react-syntax-highlighter/dist/esm/languages/prism/json'),
    import('react-syntax-highlighter/dist/esm/languages/prism/css'),
    import('react-syntax-highlighter/dist/esm/languages/prism/rust'),
    import('react-syntax-highlighter/dist/esm/languages/prism/sql'),
    import('react-syntax-highlighter/dist/esm/languages/prism/yaml'),
    import('react-syntax-highlighter/dist/esm/languages/prism/markdown'),
  ])

  for (const [name, lang] of [
    ['javascript', javascript.default],
    ['js', javascript.default],
    ['jsx', javascript.default],
    ['typescript', typescript.default],
    ['ts', typescript.default],
    ['tsx', typescript.default],
    ['python', python.default],
    ['py', python.default],
    ['bash', bash.default],
    ['sh', bash.default],
    ['shell', bash.default],
    ['json', jsonLang.default],
    ['css', cssLang.default],
    ['rust', rust.default],
    ['rs', rust.default],
    ['sql', sql.default],
    ['yaml', yaml.default],
    ['yml', yaml.default],
    ['markdown', markdownLang.default],
    ['md', markdownLang.default],
  ] as const) {
    SyntaxHighlighter.registerLanguage(name, lang)
  }

  // Wrap in a component that accepts our props and forwards to the real highlighter
  function LazyHighlighterWrapper(props: { language: string; children: string }) {
    return (
      <SyntaxHighlighter
        style={oneDark}
        language={props.language}
        PreTag="div"
        customStyle={{ margin: 0, borderRadius: '0.375rem', fontSize: '0.8rem' }}
      >
        {props.children}
      </SyntaxHighlighter>
    )
  }
  return { default: LazyHighlighterWrapper }
})

import { Button, Card, CardContent, Input, Select } from '../components/ui'
import { defaultSurfaceModel, sortProviderSurfaces } from '../features/providerSurfaces'
import { colors, iconSize, interaction, motion, radius, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

type Transport = 'subprocess' | 'http_api' | 'local_llm'
type SessionState = 'starting' | 'active' | 'idle' | 'recovering' | 'failed' | 'terminated'
interface SessionConfig {
  transport: Transport
  surface_id?: string
  model?: string
  system_prompt?: string
  tools_enabled: boolean
}
interface SessionInfo {
  session_id: string
  provider_name: string
  model: string
  state: SessionState
  transport: Transport
  created_at: string
  last_active: string
  turn_count: number
}
type OutboundMessage =
  | { type: 'text'; content: string; done: boolean }
  | { type: 'thinking'; content: string; done: boolean }
  | {
      type: 'result'
      content: string
      done: boolean
      usage?: { input_tokens: number; output_tokens: number }
    }
  | {
      type: 'tool_use'
      tool: string
      status: 'started' | 'completed' | 'failed'
      input?: unknown
      result?: string
    }
  | {
      type: 'tool_call_delta'
      index: number
      id: string
      name: string
      arguments_chunk: string
    }
  | { type: 'error'; code: string; message: string; retryable: boolean }
  | { type: 'control'; action: string }
interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
  streaming?: boolean
  thinking?: { content: string; done: boolean }
  tool_use?: { tool: string; status: string; input?: Record<string, unknown>; result?: string }
  tool_call_delta?: { index: number; id: string; name: string; arguments: string }
  usage?: { input_tokens: number; output_tokens: number }
  error?: { code: string; message: string; retryable: boolean }
}

type AttachmentPayload =
  | { kind: 'image'; mime: string; data?: string | null; path?: string | null }
  | { kind: 'file'; path: string; mime?: string | null; data?: string | null }

function parseDataUrl(dataUrl: string): { mime: string; data: string } | null {
  const match = dataUrl.match(/^data:([^;,]+)?(?:;base64)?,(.*)$/)
  if (!match) return null
  const mime = match[1] || 'application/octet-stream'
  const data = match[2] || ''
  return { mime, data }
}

const STATE_DOT: Record<string, string> = {
  active: 'bg-status-connected',
  idle: 'bg-status-connecting',
  starting: 'bg-status-connecting',
  recovering: 'bg-semantic-warning',
  failed: 'bg-status-error',
  terminated: 'bg-status-disconnected',
}

const MAX_CACHED_SESSIONS = 20
const HTTP_API_SURFACES = sortProviderSurfaces(
  DEFAULT_PROVIDER_SURFACE_CATALOG.surfaces.filter(
    (surface) =>
      surface.supports.llm &&
      surface.execution_kind === 'direct_http' &&
      surface.llm_transport?.auth_scheme !== 'aws_signature_v4',
  ),
)

async function ipc<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

function now() {
  return new Date().toISOString()
}

/* ---- Copy button for code blocks ---- */

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch (e) {
      console.warn('clipboard.writeText failed:', e)
    }
  }, [text])
  return (
    <button
      type="button"
      onClick={handleCopy}
      className={cn(
        'absolute top-2 right-2 rounded bg-surface-elevated/40 p-1.5 text-content-inverse/60 opacity-0 hover:bg-surface-elevated/60 group-hover:opacity-100',
        motion.opacity,
      )}
      title="Copy"
    >
      {copied ? <Check size={14} /> : <Copy size={14} />}
    </button>
  )
}

/* ---- Highlight helper ---- */

function highlightText(text: string, query: string): React.ReactNode {
  if (!query) return text
  const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const regex = new RegExp(`(${escaped})`, 'gi')
  const parts = text.split(regex)
  let offset = 0
  return parts.map((part) => {
    const key = `hl-${offset}`
    offset += part.length
    if (regex.test(part)) {
      return (
        <mark key={key} className="rounded bg-semantic-warning/25 px-0.5">
          {part}
        </mark>
      )
    }
    return <span key={key}>{part}</span>
  })
}

/* ---- Chat page ---- */

export default function Chat() {
  const { t } = useTranslation()
  const [sessions, setSessions] = useState<SessionInfo[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [input, setInput] = useState('')
  const [sending, setSending] = useState(false)
  const [transport, setTransport] = useState<Transport>('subprocess')
  const [creating, setCreating] = useState(false)
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [systemPrompt, setSystemPrompt] = useState('')
  const [httpSurfaceId, setHttpSurfaceId] = useState<string>(HTTP_API_SURFACES[0]?.surface_id ?? '')
  const [modelOverride, setModelOverride] = useState('')
  const [searchQuery, setSearchQuery] = useState('')
  const [searchOpen, setSearchOpen] = useState(false)
  const [attachments, setAttachments] = useState<Array<{ name: string; type: string; data: string }>>([])
  const scrollRef = useRef<HTMLDivElement>(null)
  const isNearBottom = useRef(true)
  const rafRef = useRef<number | null>(null)
  const messagesCache = useRef<Map<string, ChatMessage[]>>(new Map())
  const fileInputRef = useRef<HTMLInputElement>(null)

  // Smart auto-scroll: RAF-throttled to avoid forced layout on every scroll event
  const handleScroll = useCallback(() => {
    if (rafRef.current) return
    rafRef.current = requestAnimationFrame(() => {
      const el = scrollRef.current
      if (el) {
        isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100
      }
      rafRef.current = null
    })
  }, [])

  // Clean up pending RAF on unmount
  useEffect(() => {
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current)
    }
  }, [])

  useEffect(() => {
    if (isNearBottom.current) {
      scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' })
    }
  }, [])

  useEffect(() => {
    if (HTTP_API_SURFACES.length === 0) return
    if (!HTTP_API_SURFACES.some((surface) => surface.surface_id === httpSurfaceId)) {
      setHttpSurfaceId(HTTP_API_SURFACES[0].surface_id)
    }
  }, [httpSurfaceId])

  useEffect(() => {
    ipc<SessionInfo[]>('list_ai_sessions')
      .then(setSessions)
      .catch((e) => console.warn('list_ai_sessions failed:', e))
  }, [])

  const selectedHttpSurface = useMemo(
    () => HTTP_API_SURFACES.find((surface) => surface.surface_id === httpSurfaceId) ?? HTTP_API_SURFACES[0] ?? null,
    [httpSurfaceId],
  )
  const resolvedModel = useMemo(() => {
    const override = modelOverride.trim()
    if (override) return override
    if (transport === 'http_api') {
      return defaultSurfaceModel(selectedHttpSurface ?? undefined, 'llm_api') ?? undefined
    }
    return undefined
  }, [modelOverride, selectedHttpSurface, transport])

  useEffect(() => {
    if (!activeId) return
    let unlisten: (() => void) | null = null
    ;(async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen<OutboundMessage>(`ai-session:${activeId}`, ({ payload: p }) => {
        setMessages((prev) => {
          const finalizeThinking = (items: ChatMessage[]) => {
            const lastItem = items[items.length - 1]
            if (lastItem?.thinking && !lastItem.thinking.done) {
              return [...items.slice(0, -1), { ...lastItem, thinking: { ...lastItem.thinking, done: true } }]
            }
            return items
          }
          const appendStream = (items: ChatMessage[], c: string, done: boolean, extra?: Partial<ChatMessage>) => {
            const base = finalizeThinking(items)
            const lastItem = base[base.length - 1]
            if (lastItem?.role === 'assistant' && lastItem.streaming)
              return [...base.slice(0, -1), { ...lastItem, content: lastItem.content + c, streaming: !done, ...extra }]
            return [...base, { role: 'assistant' as const, content: c, timestamp: now(), streaming: !done, ...extra }]
          }
          const appendThinking = (items: ChatMessage[], c: string, done: boolean) => {
            const lastItem = items[items.length - 1]
            if (lastItem?.thinking && !lastItem.thinking.done) {
              return [
                ...items.slice(0, -1),
                {
                  ...lastItem,
                  content: lastItem.content + c,
                  thinking: { content: lastItem.thinking.content + c, done },
                },
              ]
            }
            return [
              ...items,
              {
                role: 'system' as const,
                content: c,
                timestamp: now(),
                thinking: { content: c, done },
              },
            ]
          }
          const appendToolCallDelta = (
            items: ChatMessage[],
            payload: Extract<OutboundMessage, { type: 'tool_call_delta' }>,
          ) => {
            const base = finalizeThinking(items)
            let existingIndex = -1
            for (let index = base.length - 1; index >= 0; index -= 1) {
              if (base[index]?.tool_call_delta?.id === payload.id) {
                existingIndex = index
                break
              }
            }

            if (existingIndex >= 0) {
              const existing = base[existingIndex]
              if (!existing?.tool_call_delta) {
                return base
              }

              return [
                ...base.slice(0, existingIndex),
                {
                  ...existing,
                  content: `${existing.content}${payload.arguments_chunk}`,
                  tool_call_delta: {
                    ...existing.tool_call_delta,
                    arguments: `${existing.tool_call_delta.arguments}${payload.arguments_chunk}`,
                  },
                },
                ...base.slice(existingIndex + 1),
              ]
            }

            return [
              ...base,
              {
                role: 'system' as const,
                content: payload.arguments_chunk,
                timestamp: now(),
                tool_call_delta: {
                  index: payload.index,
                  id: payload.id,
                  name: payload.name,
                  arguments: payload.arguments_chunk,
                },
              },
            ]
          }
          if (p.type === 'thinking') return appendThinking(prev, p.content, p.done)
          if (p.type === 'text') return appendStream(prev, p.content, p.done)
          if (p.type === 'result') return appendStream(prev, p.content, true, { usage: p.usage, streaming: false })
          if (p.type === 'tool_call_delta') return appendToolCallDelta(prev, p)
          if (p.type === 'tool_use')
            return [
              ...finalizeThinking(prev),
              {
                role: 'system',
                content: `Tool: ${p.tool} [${p.status}]`,
                timestamp: now(),
                tool_use: {
                  tool: p.tool,
                  status: p.status,
                  input: p.input as Record<string, unknown> | undefined,
                  result: p.result,
                },
              },
            ]
          if (p.type === 'error')
            return [
              ...finalizeThinking(prev),
              {
                role: 'system',
                content: p.message,
                timestamp: now(),
                error: { code: p.code, message: p.message, retryable: p.retryable },
              },
            ]
          if (p.type === 'control' && p.action === 'done') setSending(false)
          return prev
        })
      })
    })()
    return () => {
      unlisten?.()
    }
  }, [activeId])

  const refresh = useCallback(() => {
    ipc<SessionInfo[]>('list_ai_sessions')
      .then(setSessions)
      .catch((e) => console.warn('list_ai_sessions failed:', e))
  }, [])

  const handleSelectSession = useCallback(
    (id: string) => {
      // Save current messages to cache before switching
      if (activeId) messagesCache.current.set(activeId, messages)
      setActiveId(id)
      setMessages(messagesCache.current.get(id) ?? [])
      // FIFO eviction when cache exceeds limit
      if (messagesCache.current.size > MAX_CACHED_SESSIONS) {
        const oldest = messagesCache.current.keys().next().value
        if (oldest) messagesCache.current.delete(oldest)
      }
    },
    [activeId, messages],
  )

  const handleCreate = useCallback(async () => {
    setCreating(true)
    try {
      const info = await ipc<SessionInfo>('create_ai_session', {
        config: {
          transport,
          surface_id: transport === 'http_api' ? selectedHttpSurface?.surface_id : undefined,
          model: resolvedModel,
          system_prompt: systemPrompt || undefined,
          tools_enabled: true,
        } satisfies SessionConfig,
      })
      setSessions((p) => [info, ...p])
      setActiveId(info.session_id)
      setMessages([])
    } catch (e) {
      console.warn('create_ai_session failed:', e)
    }
    setCreating(false)
  }, [transport, selectedHttpSurface, resolvedModel, systemPrompt])

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await ipc('kill_ai_session', { sessionId: id })
        setSessions((p) => p.filter((s) => s.session_id !== id))
        messagesCache.current.delete(id)
        if (activeId === id) {
          setActiveId(null)
          setMessages([])
        }
      } catch (e) {
        console.warn('kill_ai_session failed:', e)
      }
    },
    [activeId],
  )

  const searchMatchCount = useMemo(() => {
    if (!searchQuery) return 0
    const q = searchQuery.toLowerCase()
    return messages.filter((m) => m.content.toLowerCase().includes(q)).length
  }, [messages, searchQuery])

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files
    if (!files) return
    for (const file of Array.from(files)) {
      const reader = new FileReader()
      reader.onload = () => {
        setAttachments((prev) => [...prev, { name: file.name, type: file.type, data: reader.result as string }])
      }
      reader.readAsDataURL(file)
    }
    e.target.value = ''
  }, [])

  const handleSend = useCallback(async () => {
    if ((!input.trim() && attachments.length === 0) || !activeId || sending) return
    const text = input.trim()
    const attachmentPayload: AttachmentPayload[] = attachments.map((attachment) => {
      const parsed = parseDataUrl(attachment.data)
      if (attachment.type.startsWith('image/')) {
        return {
          kind: 'image',
          mime: parsed?.mime || attachment.type || 'application/octet-stream',
          data: parsed?.data ?? null,
          path: null,
        }
      }

      return {
        kind: 'file',
        path: attachment.name,
        mime: parsed?.mime || attachment.type || null,
        data: parsed?.data ?? null,
      }
    })
    const attachmentSummary = attachments
      .map((attachment) =>
        attachment.type.startsWith('image/')
          ? `[Image attachment: ${attachment.name}]`
          : `[Attachment: ${attachment.name}]`,
      )
      .join('\n')
    const displayText =
      attachments.length > 0 ? [attachmentSummary, text].filter((section) => section.length > 0).join('\n') : text
    setInput('')
    setAttachments([])
    // Reset textarea height after clearing input
    const ta = document.querySelector<HTMLTextAreaElement>('form textarea')
    if (ta) ta.style.height = 'auto'
    setMessages((p) => [...p, { role: 'user', content: displayText, timestamp: now() }])
    setSending(true)
    try {
      await ipc('send_session_message', {
        sessionId: activeId,
        message: text,
        attachments: attachmentPayload,
      })
    } catch (e) {
      console.warn('send_session_message failed:', e)
      setSending(false)
    }
  }, [input, activeId, sending, attachments])

  const handleRetry = useCallback(async () => {
    if (!activeId) return
    try {
      const info = await ipc<SessionInfo>('retry_ai_session', { sessionId: activeId })
      setSessions((p) => p.map((s) => (s.session_id === info.session_id ? info : s)))
    } catch (e) {
      console.warn('retry_ai_session failed:', e)
    }
  }, [activeId])

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value)
    const el = e.target
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 128)}px`
  }, [])

  const active = sessions.find((s) => s.session_id === activeId)

  // Virtualization guard: only render the last 500 messages to prevent DOM bloat
  const MAX_VISIBLE_MESSAGES = 500
  const isTruncated = messages.length > MAX_VISIBLE_MESSAGES
  const visibleMessages = isTruncated ? messages.slice(-MAX_VISIBLE_MESSAGES) : messages
  const createDisabled = creating || (transport === 'http_api' && !selectedHttpSurface)

  return (
    <div className="flex h-full min-h-0">
      {/* Sidebar */}
      <div className="flex w-64 shrink-0 flex-col border-muted border-r bg-surface-base">
        <div className="flex items-center justify-between border-muted border-b px-3 py-2">
          <span className={cn(typography.label, colors.text.primary)}>{t('chat.title')}</span>
          <Button variant="ghost" size="sm" onClick={refresh}>
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
          <Button variant="primary" size="sm" onClick={handleCreate} isLoading={creating} disabled={createDisabled}>
            <Plus className={iconSize.sm} />
          </Button>
        </div>

        {/* Advanced settings toggle */}
        <button
          type="button"
          onClick={() => setShowAdvanced((p) => !p)}
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
                  {HTTP_API_SURFACES.map((surface) => (
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
                className={cn(
                  'text-[10px] uppercase tracking-[0.12em]',
                  typography.weight.medium,
                  colors.text.secondary,
                )}
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
                onClick={() => handleSelectSession(s.session_id)}
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
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation()
                    handleDelete(s.session_id)
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

      {/* Main area */}
      <div className="flex min-w-0 flex-1 flex-col bg-surface-sunken">
        {!activeId ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-surface-elevated">
              <MessageSquarePlus className="h-6 w-6 text-content-muted" />
            </div>
            <p className={cn('text-sm', typography.weight.medium, colors.text.primary)}>{t('chat.create_session')}</p>
            <p className={cn('text-xs', colors.text.secondary)}>{t('chat.create_hint')}</p>
          </div>
        ) : (
          <>
            <div className="flex items-center gap-2 border-muted border-b bg-surface-base px-4 py-2">
              <span className={cn('h-2 w-2 rounded-full', STATE_DOT[active?.state ?? 'terminated'])} />
              <span className={cn('text-xs', typography.weight.medium, colors.text.primary)}>
                {active?.model || active?.provider_name || 'Session'}
              </span>
              <span className={cn('text-[10px]', colors.text.secondary)}>({active?.transport})</span>
              <div className="ml-auto flex items-center gap-1">
                {searchOpen && (
                  <div className="flex items-center gap-1">
                    <input
                      type="text"
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Escape') {
                          setSearchQuery('')
                          setSearchOpen(false)
                        }
                      }}
                      placeholder="Search messages..."
                      className={cn(
                        'w-40 border bg-surface-base px-2 py-1 text-xs placeholder-content-tertiary',
                        radius.md,
                        interaction.focusRing,
                        colors.text.primary,
                        'border-DEFAULT focus:border-brand-signal',
                      )}
                    />
                    {searchQuery && (
                      <span className={cn('whitespace-nowrap text-[10px]', colors.text.secondary)}>
                        {searchMatchCount} {searchMatchCount === 1 ? 'match' : 'matches'}
                      </span>
                    )}
                  </div>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setSearchOpen((p) => !p)
                    if (searchOpen) setSearchQuery('')
                  }}
                >
                  <Search className={iconSize.xs} />
                </Button>
                {active?.state === 'failed' && (
                  <Button variant="ghost" size="sm" onClick={handleRetry} className="text-xs">
                    <RefreshCw className={iconSize.xs} /> {t('chat.retry')}
                  </Button>
                )}
              </div>
            </div>
            <div ref={scrollRef} onScroll={handleScroll} className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
              {isTruncated && (
                <button
                  type="button"
                  onClick={() => {
                    // Scroll-to-top hint; full history is in memory
                    scrollRef.current?.scrollTo({ top: 0 })
                  }}
                  className={cn(
                    'mx-auto block rounded-full border px-3 py-1 text-xs',
                    'border-muted bg-surface-elevated text-content-secondary',
                    interaction.interactive,
                  )}
                >
                  {messages.length - MAX_VISIBLE_MESSAGES} earlier messages not shown
                </button>
              )}
              {visibleMessages.map((msg) => {
                const matches = searchQuery && msg.content.toLowerCase().includes(searchQuery.toLowerCase())
                const dimmed = searchQuery && !matches
                return (
                  <div key={`${msg.timestamp}-${msg.role}`} className={cn(dimmed && 'opacity-30', motion.opacity)}>
                    <Bubble msg={msg} onRetry={handleRetry} highlight={matches ? searchQuery : undefined} />
                  </div>
                )
              })}
              {sending &&
                messages[messages.length - 1]?.role !== 'assistant' &&
                !messages[messages.length - 1]?.thinking && (
                  <div className="flex items-center gap-2 text-content-secondary text-xs">
                    <Loader2 className={cn(iconSize.sm, 'animate-spin')} /> {t('chat.thinking')}
                  </div>
                )}
            </div>
            {/* Attachment chips */}
            {attachments.length > 0 && (
              <div className="flex flex-wrap gap-1.5 border-muted border-t bg-surface-base px-4 pt-2">
                {attachments.map((a) => (
                  <div
                    key={`${a.name}-${a.data.slice(-16)}`}
                    className="flex items-center gap-1 rounded-full bg-surface-elevated px-2 py-0.5"
                  >
                    {a.type.startsWith('image/') && (
                      <img src={a.data} alt={a.name} className={cn(iconSize.md, 'rounded object-cover')} />
                    )}
                    <span className={cn('max-w-[120px] truncate text-[10px]', colors.text.primary)}>{a.name}</span>
                    <button
                      type="button"
                      onClick={() => setAttachments((prev) => prev.filter((x) => x !== a))}
                      className="text-content-muted hover:text-semantic-error"
                    >
                      <X className={iconSize.xs} />
                    </button>
                  </div>
                ))}
              </div>
            )}
            <form
              onSubmit={(e) => {
                e.preventDefault()
                handleSend()
              }}
              className={cn(
                'flex items-end gap-2 border-muted border-t bg-surface-base px-4 py-3',
                attachments.length > 0 && 'border-t-0 pt-1.5',
              )}
            >
              <input ref={fileInputRef} type="file" multiple onChange={handleFileSelect} className="hidden" />
              <Button variant="ghost" size="sm" type="button" onClick={() => fileInputRef.current?.click()}>
                <Paperclip className={iconSize.sm} />
              </Button>
              <textarea
                value={input}
                onChange={handleInputChange}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault()
                    handleSend()
                  }
                }}
                placeholder={t('chat.input_placeholder')}
                rows={1}
                style={{ overflow: 'hidden' }}
                className={cn(
                  'flex-1 resize-none border bg-surface-base px-3 py-2 text-sm placeholder-content-tertiary',
                  radius.md,
                  interaction.focusRing,
                  interaction.interactive,
                  colors.text.primary,
                  'max-h-32 border-DEFAULT focus:border-brand-signal',
                )}
              />
              <Button variant="primary" size="sm" type="submit" disabled={!input.trim() || sending}>
                <Send className={iconSize.sm} />
              </Button>
            </form>
          </>
        )}
      </div>
    </div>
  )
}

/* ---- Message bubble ---- */

function Bubble({ msg, onRetry, highlight }: { msg: ChatMessage; onRetry: () => void; highlight?: string }) {
  const { t } = useTranslation()

  // Memoize markdown components to capture msg.streaming in closure
  // Must be called before any early returns (rules of hooks)
  const mdComponents = useMemo(
    () => ({
      code({ className, children, ...props }: { className?: string; children?: React.ReactNode }) {
        const match = /language-(\w+)/.exec(className || '')
        const code = String(children).replace(/\n$/, '')

        // Fenced code block with language
        if (match) {
          // During streaming: plain pre (avoid repeated highlighting runs)
          if (msg.streaming) {
            return (
              <pre className={cn('my-2 overflow-x-auto rounded bg-surface-sunken p-3 text-xs', typography.family.mono)}>
                {code}
              </pre>
            )
          }
          // After done: full syntax highlighting (lazy-loaded)
          return (
            <div className="group relative my-2">
              <CopyButton text={code} />
              <React.Suspense
                fallback={
                  <pre className={cn('overflow-x-auto rounded bg-surface-sunken p-3 text-xs', typography.family.mono)}>
                    {code}
                  </pre>
                }
              >
                <LazySyntaxHighlighter language={match[1]}>{code}</LazySyntaxHighlighter>
              </React.Suspense>
            </div>
          )
        }

        // Inline code
        return (
          <code className={cn('rounded bg-surface-sunken px-1 py-0.5 text-xs', typography.family.mono)} {...props}>
            {children}
          </code>
        )
      },
      p: ({ children }: { children?: React.ReactNode }) => <p className="mb-2 last:mb-0">{children}</p>,
      ul: ({ children }: { children?: React.ReactNode }) => <ul className="mb-2 ml-4 list-disc">{children}</ul>,
      ol: ({ children }: { children?: React.ReactNode }) => <ol className="mb-2 ml-4 list-decimal">{children}</ol>,
      li: ({ children }: { children?: React.ReactNode }) => <li className="mb-0.5">{children}</li>,
      h3: ({ children }: { children?: React.ReactNode }) => (
        <h3 className={cn('mt-2 mb-1', typography.weight.semibold)}>{children}</h3>
      ),
      a: ({ href, children }: { href?: string; children?: React.ReactNode }) => (
        <a href={href} target="_blank" rel="noopener noreferrer" className="text-brand-text underline">
          {children}
        </a>
      ),
      blockquote: ({ children }: { children?: React.ReactNode }) => (
        <blockquote className="border-brand/30 border-l-2 pl-3 text-content-secondary italic">{children}</blockquote>
      ),
      table: ({ children }: { children?: React.ReactNode }) => (
        <div className="overflow-x-auto">
          <table className="border-collapse text-xs">{children}</table>
        </div>
      ),
      th: ({ children }: { children?: React.ReactNode }) => (
        <th className={cn('border border-muted px-2 py-1', typography.weight.medium)}>{children}</th>
      ),
      td: ({ children }: { children?: React.ReactNode }) => (
        <td className="border border-muted px-2 py-1">{children}</td>
      ),
    }),
    [msg.streaming],
  )

  if (msg.error) {
    return (
      <Card variant="default" padding="sm" className="border-semantic-error/30 bg-semantic-error/5">
        <CardContent>
          <div className="flex items-start gap-2">
            <AlertTriangle className={cn(iconSize.base, 'mt-0.5 shrink-0 text-semantic-error')} />
            <div className="min-w-0 flex-1">
              <p className={cn('text-semantic-error text-xs', typography.weight.medium)}>{msg.error.code}</p>
              <p className="mt-0.5 text-content-secondary text-xs">{msg.error.message}</p>
              {msg.error.retryable && (
                <Button variant="ghost" size="sm" onClick={onRetry} className="mt-1 text-xs">
                  <RefreshCw className={iconSize.xs} /> {t('chat.retry')}
                </Button>
              )}
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }

  if (msg.tool_use) {
    const statusCls =
      msg.tool_use.status === 'completed'
        ? 'bg-semantic-success/20 text-semantic-success'
        : msg.tool_use.status === 'failed'
          ? 'bg-semantic-error/20 text-semantic-error'
          : 'bg-surface-elevated text-content-secondary'
    return (
      <Card variant="default" padding="sm" className="border-border/50">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            {msg.tool_use.status === 'started' ? (
              <Loader2 className={cn(iconSize.xs, 'animate-spin text-content-secondary')} />
            ) : (
              <Wrench className={iconSize.xs} />
            )}
            <span className={cn('text-xs', typography.weight.medium)}>{msg.tool_use.tool}</span>
          </div>
          <span className={cn('rounded px-1.5 py-0.5 text-[10px]', statusCls)}>{msg.tool_use.status}</span>
        </div>
        {msg.tool_use.input && (
          <details className="mt-1">
            <summary className="cursor-pointer text-content-secondary text-xs">Input</summary>
            <pre className="mt-1 overflow-x-auto rounded bg-surface-sunken p-2 text-[10px]">
              {JSON.stringify(msg.tool_use.input, null, 2)}
            </pre>
          </details>
        )}
        {msg.tool_use.result && (
          <details open className="mt-1">
            <summary className="cursor-pointer text-content-secondary text-xs">Result</summary>
            <pre className="mt-1 overflow-x-auto whitespace-pre-wrap rounded bg-surface-sunken p-2 text-[10px]">
              {msg.tool_use.result}
            </pre>
          </details>
        )}
      </Card>
    )
  }

  if (msg.tool_call_delta) {
    return (
      <Card variant="default" padding="sm" className="border-border/50 bg-surface-base">
        <CardContent>
          <div className="flex items-start gap-2">
            <Loader2 className={cn(iconSize.xs, 'mt-0.5 shrink-0 animate-spin text-content-secondary')} />
            <div className="min-w-0 flex-1">
              <p className={cn('text-xs', typography.weight.medium)}>{msg.tool_call_delta.name}</p>
              <pre className="mt-1 overflow-x-auto whitespace-pre-wrap rounded bg-surface-sunken p-2 text-[10px]">
                {msg.tool_call_delta.arguments}
              </pre>
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }

  if (msg.thinking) {
    return (
      <Card variant="default" padding="sm" className="border-brand/20 bg-brand/5">
        <CardContent>
          <div className="flex items-start gap-2">
            <Loader2
              className={cn(iconSize.xs, !msg.thinking.done && 'animate-spin', 'mt-0.5 shrink-0 text-brand-text')}
            />
            <div className="min-w-0 flex-1">
              <p
                className={cn(
                  'text-[10px] text-content-secondary uppercase tracking-[0.14em]',
                  typography.weight.medium,
                )}
              >
                {t('chat.thinking')}
              </p>
              <p className="mt-1 whitespace-pre-wrap break-words text-content-secondary text-xs">
                {msg.thinking.content}
                {!msg.thinking.done ? '\u258C' : ''}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }

  const isUser = msg.role === 'user'

  return (
    <div className={cn('flex gap-2', isUser ? 'justify-end' : 'justify-start')}>
      {!isUser && (
        <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-brand/10">
          <Bot className="h-3.5 w-3.5 text-brand-text" />
        </div>
      )}
      <div
        className={cn(
          'max-w-[75%] rounded-lg px-3 py-2 text-sm',
          isUser ? 'bg-brand text-content-inverse' : 'bg-surface-elevated text-content',
        )}
      >
        {isUser ? (
          <p className="whitespace-pre-wrap break-words">
            {highlight ? highlightText(msg.content, highlight) : msg.content}
          </p>
        ) : (
          <div className="prose-sm">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={mdComponents}>
              {msg.content + (msg.streaming ? '\u258C' : '')}
            </ReactMarkdown>
          </div>
        )}
        {msg.usage && (
          <p className={cn('mt-1 text-[10px]', isUser ? 'text-content-inverse/60' : 'text-content-secondary')}>
            {msg.usage.input_tokens} in / {msg.usage.output_tokens} out
          </p>
        )}
      </div>
      {isUser && (
        <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-surface-elevated">
          <User className="h-3.5 w-3.5 text-content-secondary" />
        </div>
      )}
    </div>
  )
}
