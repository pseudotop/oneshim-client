// i18n: This overlay runs in a separate Tauri window without i18n initialization.
// Hardcoded labels ("OK", "Later") are intentional and acceptable here.
import { useCallback, useEffect, useRef, useState } from 'react'
import { motion, typography } from '../../styles/tokens'
import { useAutoDismiss } from '../hooks/useAutoDismiss'
import type { CoachingPayload, DismissAction } from '../types'

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke(cmd, args)
}

interface CoachingPopupProps {
  message: CoachingPayload
  autoDismissSecs: number
}

export default function CoachingPopup({ message, autoDismissSecs }: CoachingPopupProps) {
  const [text, setText] = useState(message.text)
  const [transitioning, setTransitioning] = useState(false)
  const prevTextRef = useRef(message.text)

  // Detect text upgrade (LLM personalization)
  useEffect(() => {
    if (message.text !== prevTextRef.current) {
      setTransitioning(true)
      const timer = setTimeout(() => {
        setText(message.text)
        setTransitioning(false)
      }, 300) // fade duration
      prevTextRef.current = message.text
      return () => clearTimeout(timer)
    }
    setText(message.text)
  }, [message.text])

  const dismiss = useCallback(
    async (action: DismissAction) => {
      await tauriInvoke('dismiss_coaching_message', {
        messageId: message.message_id,
        action,
        profile: message.profile,
      })
    },
    [message.message_id, message.profile],
  )

  const { reset } = useAutoDismiss(true, autoDismissSecs, () => void dismiss('timeout'))

  // Reset auto-dismiss when LLM upgrade arrives
  useEffect(() => {
    if (message.text !== prevTextRef.current) {
      reset()
    }
  }, [message.text, reset])

  const feedback = useCallback(
    async (positive: boolean) => {
      await tauriInvoke('submit_coaching_feedback', {
        messageId: message.message_id,
        positive,
      })
    },
    [message.message_id],
  )

  // Note: No per-element mouseenter/mouseleave cursor passthrough management.
  // The overlay is click-through by default. The user presses Cmd+Shift+O to
  // make it interactive. After dismissal, the Rust backend returns it to
  // click-through mode automatically.

  return (
    <div className="fixed top-4 right-4 z-50">
      <div className="w-80 rounded-xl border border-content-inverse/10 bg-surface-sunken/90 p-4 shadow-2xl backdrop-blur-md">
        {/* Message text with transition */}
        <p
          className={`mb-3 text-content text-sm leading-relaxed ${motion.opacity}${
            transitioning ? 'opacity-0' : 'opacity-100'
          }`}
        >
          {text}
        </p>

        {/* Actions row */}
        <div className="flex items-center justify-between">
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void dismiss('ok')}
              aria-label="Dismiss coaching message"
              className={`rounded-md bg-content-inverse/10 px-3 py-1 text-xs ${typography.weight.medium} text-content-secondary ${motion.colors} hover:bg-content-inverse/20`}
            >
              OK
            </button>
            <button
              type="button"
              onClick={() => void dismiss('later')}
              aria-label="Remind me later"
              className={`rounded-md bg-content-inverse/5 px-3 py-1 text-xs ${typography.weight.medium} text-content-tertiary ${motion.colors} hover:bg-content-inverse/10`}
            >
              Later
            </button>
          </div>

          {/* Thumbs feedback — subtle by default */}
          <div className="flex gap-1">
            <button
              type="button"
              onClick={() => void feedback(true)}
              className={`rounded p-1 text-content-muted opacity-30 ${motion.opacity} hover:text-semantic-success hover:opacity-100`.trim()}
              aria-label="Helpful"
            >
              <ThumbsUpIcon />
            </button>
            <button
              type="button"
              onClick={() => void feedback(false)}
              className={`rounded p-1 text-content-muted opacity-30 ${motion.opacity} hover:text-semantic-error hover:opacity-100`.trim()}
              aria-label="Not helpful"
            >
              <ThumbsDownIcon />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function ThumbsUpIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <title>Thumbs up</title>
      <path d="M7 10v12" />
      <path d="M15 5.88 14 10h5.83a2 2 0 0 1 1.92 2.56l-2.33 8A2 2 0 0 1 17.5 22H4a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2h2.76a2 2 0 0 0 1.79-1.11L12 2h0a3.13 3.13 0 0 1 3 3.88Z" />
    </svg>
  )
}

function ThumbsDownIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <title>Thumbs down</title>
      <path d="M17 14V2" />
      <path d="M9 18.12 10 14H4.17a2 2 0 0 1-1.92-2.56l2.33-8A2 2 0 0 1 6.5 2H20a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-2.76a2 2 0 0 0-1.79 1.11L12 22h0a3.13 3.13 0 0 1-3-3.88Z" />
    </svg>
  )
}
