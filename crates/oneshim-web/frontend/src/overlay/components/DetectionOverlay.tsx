import { forwardRef, useCallback, useEffect, useRef } from 'react'
import type { DetectionElementPayload, DetectionScenePayload } from '../types'

const ROLE_COLORS: Record<string, string> = {
  AXButton: '#3B82F6',
  button: '#3B82F6',
  Button: '#3B82F6',
  AXTextField: '#22C55E',
  AXTextArea: '#22C55E',
  edit: '#22C55E',
  Edit: '#22C55E',
  TextInput: '#22C55E',
  AXLink: '#A855F7',
  link: '#A855F7',
  Link: '#A855F7',
  AXMenuItem: '#F97316',
  menuitem: '#F97316',
  MenuItem: '#F97316',
  AXTabGroup: '#06B6D4',
  tab: '#06B6D4',
  TabLabel: '#06B6D4',
  AXOutlineRow: '#F59E0B',
  treeitem: '#F59E0B',
  TreeItem: '#F59E0B',
  AXImage: '#EC4899',
  image: '#EC4899',
  Image: '#EC4899',
}

const DEFAULT_COLOR = '#6B7280'

function getRoleColor(role: string | null): string {
  if (!role) return DEFAULT_COLOR
  return ROLE_COLORS[role] ?? DEFAULT_COLOR
}

function getRoleLabel(role: string | null): string {
  if (!role) return '?'
  return role.replace(/^AX/, '')
}

interface DetectionOverlayProps {
  scene: DetectionScenePayload
  selectedId: string | null
  onSelect: (id: string | null) => void
}

export default function DetectionOverlay({ scene, selectedId, onSelect }: DetectionOverlayProps) {
  const selected = selectedId ? scene.elements.find((el) => el.element_id === selectedId) : null

  const handleBoxClick = useCallback(
    (el: DetectionElementPayload) => {
      onSelect(selectedId === el.element_id ? null : el.element_id)
    },
    [selectedId, onSelect],
  )

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        if (selectedId) {
          onSelect(null)
        } else {
          import('@tauri-apps/api/core')
            .then(({ invoke }) => {
              invoke('toggle_detection_overlay', { active: false })
            })
            .catch((e) => console.warn('toggle_detection_overlay failed:', e))
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selectedId, onSelect])

  return (
    <>
      {scene.elements.map((el) => {
        const color = getRoleColor(el.role)
        const opacity = el.confidence * 0.6 + 0.4
        const isSelected = el.element_id === selectedId

        return (
          <button
            type="button"
            key={el.element_id}
            aria-label={`${getRoleLabel(el.role)}: ${el.label || 'unlabeled'}`}
            className="fixed cursor-pointer"
            style={{
              left: el.x,
              top: el.y,
              width: el.width,
              height: el.height,
              border: `${isSelected ? 2 : 1.5}px solid ${color}`,
              borderRadius: '2px',
              backgroundColor: `${color}${isSelected ? '20' : '14'}`,
              opacity,
              zIndex: isSelected ? 10001 : 10000,
              transition: 'border-width 0.1s, background-color 0.1s',
            }}
            onClick={(e) => {
              e.stopPropagation()
              handleBoxClick(el)
            }}
          >
            <span
              className="absolute top-0.5 left-0.5 rounded px-0.5 font-medium text-[8px] leading-3"
              style={{
                backgroundColor: `${color}CC`,
                color: '#fff',
                whiteSpace: 'nowrap',
              }}
            >
              {getRoleLabel(el.role)}
            </span>
          </button>
        )
      })}

      {selected && <Inspector element={selected} />}
    </>
  )
}

interface InspectorProps {
  element: DetectionElementPayload
}

const Inspector = forwardRef<HTMLDivElement, InspectorProps>(function Inspector({ element }, ref) {
  const color = getRoleColor(element.role)
  const innerRef = useRef<HTMLDivElement>(null)
  const resolvedRef = (ref as React.RefObject<HTMLDivElement>) ?? innerRef

  const left = Math.min(element.x + element.width + 8, window.innerWidth - 300)
  const top = Math.min(element.y, window.innerHeight - 200)

  return (
    <div
      ref={resolvedRef}
      className="fixed z-detection-inspector rounded-lg border border-white/20 bg-black/85 p-3 text-white text-xs shadow-2xl backdrop-blur-sm"
      style={{ left, top, width: 260 }}
    >
      <div className="mb-1.5 flex items-center justify-between">
        <span className="rounded px-1.5 py-0.5 font-semibold text-[10px]" style={{ backgroundColor: `${color}CC` }}>
          {getRoleLabel(element.role)}
        </span>
        <span className="text-white/60">{(element.confidence * 100).toFixed(0)}%</span>
      </div>
      <div className="space-y-1 text-[11px]">
        <Row label="label" value={element.label || '(empty)'} />
        <Row label="role" value={element.role ?? 'unknown'} />
        <Row label="bounds" value={`(${element.x}, ${element.y}, ${element.width}, ${element.height})`} />
        <Row label="source" value={element.source} />
        <Row label="id" value={element.element_id} mono />
      </div>
    </div>
  )
})

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-white/50">{label}</span>
      <span className={`text-right ${mono ? 'font-mono' : ''} truncate`} title={value}>
        {value}
      </span>
    </div>
  )
}
