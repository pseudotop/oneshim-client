import { useCallback, useEffect, useRef, useState } from 'react'
import { AlertTriangle, Bot, Loader2, MessageSquarePlus, Plus, RefreshCw, Send, Trash2, User, Wrench } from 'lucide-react'
import { Button, Card, CardContent, Select } from '../components/ui'
import { colors, iconSize, interaction, radius, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

type Transport = 'subprocess' | 'http_api' | 'local_llm'
type SessionState = 'starting' | 'active' | 'idle' | 'recovering' | 'failed' | 'terminated'
interface SessionConfig { transport: Transport; surface_id?: string; model?: string; system_prompt?: string; tools_enabled: boolean }
interface SessionInfo { session_id: string; provider_name: string; model: string; state: SessionState; transport: Transport; created_at: string; last_active: string; turn_count: number }
type OutboundMessage =
  | { type: 'text'; content: string; done: boolean }
  | { type: 'result'; content: string; done: boolean; usage?: { input_tokens: number; output_tokens: number } }
  | { type: 'tool_use'; tool: string; status: 'started' | 'completed' | 'failed'; input?: unknown; result?: string }
  | { type: 'error'; code: string; message: string; retryable: boolean }
  | { type: 'control'; action: string }
interface ChatMessage {
  role: 'user' | 'assistant' | 'system'; content: string; timestamp: string; streaming?: boolean
  tool_use?: { tool: string; status: string }; usage?: { input_tokens: number; output_tokens: number }
  error?: { code: string; message: string; retryable: boolean }
}

const STATE_DOT: Record<string, string> = {
  active: 'bg-status-connected', idle: 'bg-status-connecting', starting: 'bg-status-connecting',
  recovering: 'bg-semantic-warning', failed: 'bg-status-error', terminated: 'bg-status-disconnected',
}

async function ipc<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core')
  return invoke<T>(cmd, args)
}

function now() { return new Date().toISOString() }

export default function Chat() {
  const [sessions, setSessions] = useState<SessionInfo[]>([])
  const [activeId, setActiveId] = useState<string | null>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [input, setInput] = useState('')
  const [sending, setSending] = useState(false)
  const [transport, setTransport] = useState<Transport>('subprocess')
  const [creating, setCreating] = useState(false)
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' })
  }, [messages])

  useEffect(() => { ipc<SessionInfo[]>('list_ai_sessions').then(setSessions).catch(() => {}) }, [])

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
            return [...prev, { role: 'system', content: `Tool: ${p.tool} [${p.status}]`, timestamp: now(), tool_use: { tool: p.tool, status: p.status } }]
          if (p.type === 'error')
            return [...prev, { role: 'system', content: p.message, timestamp: now(), error: { code: p.code, message: p.message, retryable: p.retryable } }]
          if (p.type === 'control' && p.action === 'done') setSending(false)
          return prev
        })
      })
    })()
    return () => { unlisten?.() }
  }, [activeId])

  const refresh = useCallback(() => { ipc<SessionInfo[]>('list_ai_sessions').then(setSessions).catch(() => {}) }, [])

  const handleCreate = useCallback(async () => {
    setCreating(true)
    try {
      const info = await ipc<SessionInfo>('create_ai_session', { config: { transport, tools_enabled: true } satisfies SessionConfig })
      setSessions((p) => [info, ...p]); setActiveId(info.session_id); setMessages([])
    } catch { /* noop */ }
    setCreating(false)
  }, [transport])

  const handleDelete = useCallback(async (id: string) => {
    try {
      await ipc('kill_ai_session', { sessionId: id })
      setSessions((p) => p.filter((s) => s.session_id !== id))
      if (activeId === id) { setActiveId(null); setMessages([]) }
    } catch { /* noop */ }
  }, [activeId])

  const handleSend = useCallback(async () => {
    if (!input.trim() || !activeId || sending) return
    const text = input.trim(); setInput('')
    setMessages((p) => [...p, { role: 'user', content: text, timestamp: now() }]); setSending(true)
    try { await ipc('send_session_message', { sessionId: activeId, message: text }) } catch { setSending(false) }
  }, [input, activeId, sending])

  const handleRetry = useCallback(async () => {
    if (!activeId) return
    try {
      const info = await ipc<SessionInfo>('retry_ai_session', { sessionId: activeId })
      setSessions((p) => p.map((s) => (s.session_id === info.session_id ? info : s)))
    } catch { /* noop */ }
  }, [activeId])

  const active = sessions.find((s) => s.session_id === activeId)

  return (
    <div className="flex h-full min-h-0">
      {/* Sidebar */}
      <div className="flex w-64 shrink-0 flex-col border-r border-muted bg-surface-base">
        <div className="flex items-center justify-between border-b border-muted px-3 py-2">
          <span className={cn(typography.label, colors.text.primary)}>Sessions</span>
          <Button variant="ghost" size="sm" onClick={refresh}><RefreshCw className={iconSize.sm} /></Button>
        </div>
        <div className="flex items-center gap-1 border-b border-muted px-2 py-2">
          <Select selectSize="sm" value={transport} onChange={(e) => setTransport(e.target.value as Transport)} className="flex-1 text-xs">
            <option value="subprocess">Subprocess</option>
            <option value="http_api">HTTP API</option>
            <option value="local_llm">Local LLM</option>
          </Select>
          <Button variant="primary" size="sm" onClick={handleCreate} isLoading={creating} disabled={creating}>
            <Plus className={iconSize.sm} />
          </Button>
        </div>
        <div className="flex-1 overflow-y-auto">
          {sessions.length === 0 ? (
            <p className={cn('px-3 py-4 text-center text-xs', colors.text.secondary)}>No sessions</p>
          ) : sessions.map((s) => (
            <button key={s.session_id} type="button"
              onClick={() => { setActiveId(s.session_id); setMessages([]) }}
              className={cn('group flex w-full items-center gap-2 px-3 py-2 text-left', interaction.interactive,
                activeId === s.session_id ? 'bg-surface-elevated' : 'hover:bg-hover')}>
              <span className={cn('h-2 w-2 shrink-0 rounded-full', STATE_DOT[s.state] ?? 'bg-status-disconnected')} />
              <div className="min-w-0 flex-1">
                <p className={cn('truncate text-xs', typography.weight.medium, colors.text.primary)}>{s.model || s.provider_name}</p>
                <p className={cn('truncate text-[10px]', colors.text.secondary)}>{s.transport} -- {s.turn_count} turns</p>
              </div>
              <button type="button" onClick={(e) => { e.stopPropagation(); handleDelete(s.session_id) }}
                className="hidden text-content-muted hover:text-semantic-error group-hover:block">
                <Trash2 className={iconSize.xs} />
              </button>
            </button>
          ))}
        </div>
      </div>

      {/* Main area */}
      <div className="flex min-w-0 flex-1 flex-col bg-surface-sunken">
        {!activeId ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-surface-elevated">
              <MessageSquarePlus className="h-6 w-6 text-content-muted" />
            </div>
            <p className={cn('text-sm', typography.weight.medium, colors.text.primary)}>Create a session to start chatting</p>
            <p className={cn('text-xs', colors.text.secondary)}>Select a transport and press + in the sidebar.</p>
          </div>
        ) : (<>
          <div className="flex items-center gap-2 border-b border-muted bg-surface-base px-4 py-2">
            <span className={cn('h-2 w-2 rounded-full', STATE_DOT[active?.state ?? 'terminated'])} />
            <span className={cn('text-xs', typography.weight.medium, colors.text.primary)}>{active?.model || active?.provider_name || 'Session'}</span>
            <span className={cn('text-[10px]', colors.text.secondary)}>({active?.transport})</span>
            {active?.state === 'failed' && (
              <Button variant="ghost" size="sm" onClick={handleRetry} className="ml-auto text-xs"><RefreshCw className={iconSize.xs} /> Retry</Button>
            )}
          </div>
          <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
            {messages.map((msg, i) => <Bubble key={i} msg={msg} onRetry={handleRetry} />)}
            {sending && messages[messages.length - 1]?.role !== 'assistant' && (
              <div className="flex items-center gap-2 text-content-secondary text-xs">
                <Loader2 className={cn(iconSize.sm, 'animate-spin')} /> Thinking...
              </div>
            )}
          </div>
          <form onSubmit={(e) => { e.preventDefault(); handleSend() }}
            className="flex items-end gap-2 border-t border-muted bg-surface-base px-4 py-3">
            <textarea value={input} onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSend() } }}
              placeholder="Type a message..." rows={1}
              className={cn('flex-1 resize-none border bg-surface-base px-3 py-2 text-sm placeholder-content-tertiary',
                radius.md, interaction.focusRing, interaction.interactive, colors.text.primary,
                'border-DEFAULT focus:border-brand-signal max-h-32')} />
            <Button variant="primary" size="sm" type="submit" disabled={!input.trim() || sending}>
              <Send className={iconSize.sm} />
            </Button>
          </form>
        </>)}
      </div>
    </div>
  )
}

function Bubble({ msg, onRetry }: { msg: ChatMessage; onRetry: () => void }) {
  if (msg.error) {
    return (
      <Card variant="default" padding="sm" className="border-semantic-error/30 bg-semantic-error/5">
        <CardContent>
          <div className="flex items-start gap-2">
            <AlertTriangle className={cn(iconSize.base, 'mt-0.5 shrink-0 text-semantic-error')} />
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium text-semantic-error">{msg.error.code}</p>
              <p className="mt-0.5 text-xs text-content-secondary">{msg.error.message}</p>
              {msg.error.retryable && (
                <Button variant="ghost" size="sm" onClick={onRetry} className="mt-1 text-xs"><RefreshCw className={iconSize.xs} /> Retry</Button>
              )}
            </div>
          </div>
        </CardContent>
      </Card>
    )
  }
  if (msg.tool_use) {
    const statusCls = msg.tool_use.status === 'completed' ? 'bg-semantic-success/20 text-semantic-success'
      : msg.tool_use.status === 'failed' ? 'bg-semantic-error/20 text-semantic-error' : 'bg-surface-elevated text-content-secondary'
    return (
      <div className="flex items-center gap-2 px-2 text-xs text-content-secondary">
        <Wrench className={iconSize.xs} /><span>{msg.tool_use.tool}</span>
        <span className={cn('rounded px-1.5 py-0.5 text-[10px]', statusCls)}>{msg.tool_use.status}</span>
      </div>
    )
  }
  const isUser = msg.role === 'user'
  return (
    <div className={cn('flex gap-2', isUser ? 'justify-end' : 'justify-start')}>
      {!isUser && <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-brand/10"><Bot className="h-3.5 w-3.5 text-brand-text" /></div>}
      <div className={cn('max-w-[75%] rounded-lg px-3 py-2 text-sm', isUser ? 'bg-brand text-content-inverse' : 'bg-surface-elevated text-content')}>
        <p className="whitespace-pre-wrap break-words">{msg.content}{msg.streaming ? '\u258C' : ''}</p>
        {msg.usage && <p className={cn('mt-1 text-[10px]', isUser ? 'text-content-inverse/60' : 'text-content-secondary')}>{msg.usage.input_tokens} in / {msg.usage.output_tokens} out</p>}
      </div>
      {isUser && <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-surface-elevated"><User className="h-3.5 w-3.5 text-content-secondary" /></div>}
    </div>
  )
}
