import {
  AlertTriangle,
  Bot,
  Check,
  ChevronDown,
  Copy,
  Loader2,
  MessageSquarePlus,
  Plus,
  RefreshCw,
  Send,
  Trash2,
  User,
  Wrench,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import ReactMarkdown from 'react-markdown'
import bash from 'react-syntax-highlighter/dist/esm/languages/prism/bash'
import cssLang from 'react-syntax-highlighter/dist/esm/languages/prism/css'
import javascript from 'react-syntax-highlighter/dist/esm/languages/prism/javascript'
import jsonLang from 'react-syntax-highlighter/dist/esm/languages/prism/json'
import markdownLang from 'react-syntax-highlighter/dist/esm/languages/prism/markdown'
import python from 'react-syntax-highlighter/dist/esm/languages/prism/python'
import rust from 'react-syntax-highlighter/dist/esm/languages/prism/rust'
import sql from 'react-syntax-highlighter/dist/esm/languages/prism/sql'
import typescript from 'react-syntax-highlighter/dist/esm/languages/prism/typescript'
import yaml from 'react-syntax-highlighter/dist/esm/languages/prism/yaml'
import SyntaxHighlighter from 'react-syntax-highlighter/dist/esm/prism-async-light'
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import remarkGfm from 'remark-gfm'
import { Button, Card, CardContent, Select } from '../components/ui'
import { colors, iconSize, interaction, radius, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

// Register languages with PrismAsyncLight (ships with zero languages by default)
SyntaxHighlighter.registerLanguage('javascript', javascript)
SyntaxHighlighter.registerLanguage('js', javascript)
SyntaxHighlighter.registerLanguage('jsx', javascript)
SyntaxHighlighter.registerLanguage('typescript', typescript)
SyntaxHighlighter.registerLanguage('ts', typescript)
SyntaxHighlighter.registerLanguage('tsx', typescript)
SyntaxHighlighter.registerLanguage('python', python)
SyntaxHighlighter.registerLanguage('py', python)
SyntaxHighlighter.registerLanguage('bash', bash)
SyntaxHighlighter.registerLanguage('sh', bash)
SyntaxHighlighter.registerLanguage('shell', bash)
SyntaxHighlighter.registerLanguage('json', jsonLang)
SyntaxHighlighter.registerLanguage('css', cssLang)
SyntaxHighlighter.registerLanguage('rust', rust)
SyntaxHighlighter.registerLanguage('rs', rust)
SyntaxHighlighter.registerLanguage('sql', sql)
SyntaxHighlighter.registerLanguage('yaml', yaml)
SyntaxHighlighter.registerLanguage('yml', yaml)
SyntaxHighlighter.registerLanguage('markdown', markdownLang)
SyntaxHighlighter.registerLanguage('md', markdownLang)

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
  | { type: 'error'; code: string; message: string; retryable: boolean }
  | { type: 'control'; action: string }
interface ChatMessage {
  role: 'user' | 'assistant' | 'system'
  content: string
  timestamp: string
  streaming?: boolean
  tool_use?: { tool: string; status: string }
  usage?: { input_tokens: number; output_tokens: number }
  error?: { code: string; message: string; retryable: boolean }
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
    } catch {
      // Clipboard API may be unavailable in some Tauri WebView contexts
    }
  }, [text])
  return (
    <button
      type="button"
      onClick={handleCopy}
      className="absolute top-2 right-2 rounded bg-white/10 p-1 text-white/60 opacity-0 transition-opacity hover:bg-white/20 group-hover:opacity-100"
      title="Copy"
    >
      {copied ? <Check size={14} /> : <Copy size={14} />}
    </button>
  )
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
  const scrollRef = useRef<HTMLDivElement>(null)
  const isNearBottom = useRef(true)
  const messagesCache = useRef<Map<string, ChatMessage[]>>(new Map())

  // Smart auto-scroll: only scroll when user is near bottom
  const handleScroll = useCallback(() => {
    const el = scrollRef.current
    if (!el) return
    isNearBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100
  }, [])

  useEffect(() => {
    if (isNearBottom.current) {
      scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' })
    }
  }, [])

  useEffect(() => {
    ipc<SessionInfo[]>('list_ai_sessions')
      .then(setSessions)
      .catch(() => {})
  }, [])

  useEffect(() => {
    if (!activeId) return
    let unlisten: (() => void) | null = null
    ;(async () => {
      const { listen } = await import('@tauri-apps/api/event')
      unlisten = await listen<OutboundMessage>(`ai-session:${activeId}`, ({ payload: p }) => {
        setMessages((prev) => {
          const last = prev[prev.length - 1]
          const appendStream = (c: string, done: boolean, extra?: Partial<ChatMessage>) => {
            if (last?.role === 'assistant' && last.streaming)
              return [...prev.slice(0, -1), { ...last, content: last.content + c, streaming: !done, ...extra }]
            return [...prev, { role: 'assistant' as const, content: c, timestamp: now(), streaming: !done, ...extra }]
          }
          if (p.type === 'text') return appendStream(p.content, p.done)
          if (p.type === 'result') return appendStream(p.content, true, { usage: p.usage, streaming: false })
          if (p.type === 'tool_use')
            return [
              ...prev,
              {
                role: 'system',
                content: `Tool: ${p.tool} [${p.status}]`,
                timestamp: now(),
                tool_use: { tool: p.tool, status: p.status },
              },
            ]
          if (p.type === 'error')
            return [
              ...prev,
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
      .catch(() => {})
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
          system_prompt: systemPrompt || undefined,
          tools_enabled: true,
        } satisfies SessionConfig,
      })
      setSessions((p) => [info, ...p])
      setActiveId(info.session_id)
      setMessages([])
    } catch {
      /* noop */
    }
    setCreating(false)
  }, [transport, systemPrompt])

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
      } catch {
        /* noop */
      }
    },
    [activeId],
  )

  const handleSend = useCallback(async () => {
    if (!input.trim() || !activeId || sending) return
    const text = input.trim()
    setInput('')
    // Reset textarea height after clearing input
    const ta = document.querySelector<HTMLTextAreaElement>('form textarea')
    if (ta) ta.style.height = 'auto'
    setMessages((p) => [...p, { role: 'user', content: text, timestamp: now() }])
    setSending(true)
    try {
      await ipc('send_session_message', { sessionId: activeId, message: text })
    } catch {
      setSending(false)
    }
  }, [input, activeId, sending])

  const handleRetry = useCallback(async () => {
    if (!activeId) return
    try {
      const info = await ipc<SessionInfo>('retry_ai_session', { sessionId: activeId })
      setSessions((p) => p.map((s) => (s.session_id === info.session_id ? info : s)))
    } catch {
      /* noop */
    }
  }, [activeId])

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value)
    const el = e.target
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 128)}px`
  }, [])

  const active = sessions.find((s) => s.session_id === activeId)

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
          <Button variant="primary" size="sm" onClick={handleCreate} isLoading={creating} disabled={creating}>
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
          <ChevronDown className={cn('h-3 w-3 transition-transform', showAdvanced && 'rotate-180')} />
          {t('chat.advanced')}
        </button>
        {showAdvanced && (
          <div className="border-muted border-b px-2 py-2">
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
              {active?.state === 'failed' && (
                <Button variant="ghost" size="sm" onClick={handleRetry} className="ml-auto text-xs">
                  <RefreshCw className={iconSize.xs} /> {t('chat.retry')}
                </Button>
              )}
            </div>
            <div ref={scrollRef} onScroll={handleScroll} className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
              {messages.map((msg) => (
                <Bubble key={`${msg.timestamp}-${msg.role}`} msg={msg} onRetry={handleRetry} />
              ))}
              {sending && messages[messages.length - 1]?.role !== 'assistant' && (
                <div className="flex items-center gap-2 text-content-secondary text-xs">
                  <Loader2 className={cn(iconSize.sm, 'animate-spin')} /> {t('chat.thinking')}
                </div>
              )}
            </div>
            <form
              onSubmit={(e) => {
                e.preventDefault()
                handleSend()
              }}
              className="flex items-end gap-2 border-muted border-t bg-surface-base px-4 py-3"
            >
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

function Bubble({ msg, onRetry }: { msg: ChatMessage; onRetry: () => void }) {
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
            return <pre className="my-2 overflow-x-auto rounded bg-surface-sunken p-3 font-mono text-xs">{code}</pre>
          }
          // After done: full syntax highlighting
          return (
            <div className="group relative my-2">
              <CopyButton text={code} />
              <SyntaxHighlighter
                style={oneDark}
                language={match[1]}
                PreTag="div"
                customStyle={{ margin: 0, borderRadius: '0.375rem', fontSize: '0.8rem' }}
              >
                {code}
              </SyntaxHighlighter>
            </div>
          )
        }

        // Inline code
        return (
          <code className="rounded bg-surface-sunken px-1 py-0.5 font-mono text-xs" {...props}>
            {children}
          </code>
        )
      },
      p: ({ children }: { children?: React.ReactNode }) => <p className="mb-2 last:mb-0">{children}</p>,
      ul: ({ children }: { children?: React.ReactNode }) => <ul className="mb-2 ml-4 list-disc">{children}</ul>,
      ol: ({ children }: { children?: React.ReactNode }) => <ol className="mb-2 ml-4 list-decimal">{children}</ol>,
      li: ({ children }: { children?: React.ReactNode }) => <li className="mb-0.5">{children}</li>,
      h3: ({ children }: { children?: React.ReactNode }) => <h3 className="mt-2 mb-1 font-semibold">{children}</h3>,
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
        <th className="border border-muted px-2 py-1 font-medium">{children}</th>
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
              <p className="font-medium text-semantic-error text-xs">{msg.error.code}</p>
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
      <div className="flex items-center gap-2 px-2 text-content-secondary text-xs">
        <Wrench className={iconSize.xs} />
        <span>{msg.tool_use.tool}</span>
        <span className={cn('rounded px-1.5 py-0.5 text-[10px]', statusCls)}>{msg.tool_use.status}</span>
      </div>
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
          <p className="whitespace-pre-wrap break-words">{msg.content}</p>
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
