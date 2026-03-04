import { useState, useCallback, useRef } from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { layout, interaction } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export interface TreeNode {
  id: string
  label: string
  icon?: React.ReactNode
  count?: number
  children?: TreeNode[]
}

interface TreeViewProps {
  nodes: TreeNode[]
  selectedId?: string
  onSelect?: (id: string) => void
  depth?: number
}

export default function TreeView({ nodes, selectedId, onSelect, depth = 0 }: TreeViewProps) {
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    return new Set(nodes.filter(n => n.children?.length).map(n => n.id))
  })
  const treeRef = useRef<HTMLDivElement>(null)

  const toggleExpand = (id: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  // Arrow-key navigation for ARIA tree pattern (root level only)
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!treeRef.current) return
    const items = Array.from(treeRef.current.querySelectorAll<HTMLElement>('[role="treeitem"]'))
    const currentIndex = items.indexOf(e.target as HTMLElement)
    if (currentIndex === -1) return

    switch (e.key) {
      case 'ArrowDown': {
        e.preventDefault()
        const next = items[currentIndex + 1]
        next?.focus()
        break
      }
      case 'ArrowUp': {
        e.preventDefault()
        const prev = items[currentIndex - 1]
        prev?.focus()
        break
      }
      case 'ArrowRight': {
        e.preventDefault()
        const btn = items[currentIndex]
        const nodeId = btn.dataset.nodeId
        if (nodeId && !expanded.has(nodeId)) {
          toggleExpand(nodeId)
        } else {
          // Move into first child
          const next = items[currentIndex + 1]
          next?.focus()
        }
        break
      }
      case 'ArrowLeft': {
        e.preventDefault()
        const btn = items[currentIndex]
        const nodeId = btn.dataset.nodeId
        if (nodeId && expanded.has(nodeId)) {
          toggleExpand(nodeId)
        } else {
          // Move to parent (previous item at lower depth)
          const prev = items[currentIndex - 1]
          prev?.focus()
        }
        break
      }
      case 'Home': {
        e.preventDefault()
        items[0]?.focus()
        break
      }
      case 'End': {
        e.preventDefault()
        items[items.length - 1]?.focus()
        break
      }
    }
  }, [expanded])

  return (
    <div
      ref={depth === 0 ? treeRef : undefined}
      className="text-sm"
      role={depth === 0 ? 'tree' : 'group'}
      onKeyDown={depth === 0 ? handleKeyDown : undefined}
    >
      {nodes.map((node, index) => {
        const hasChildren = node.children && node.children.length > 0
        const isExpanded = expanded.has(node.id)
        const isSelected = selectedId === node.id

        return (
          <div key={node.id} role="none">
            <button
              role="treeitem"
              aria-expanded={hasChildren ? isExpanded : undefined}
              aria-selected={isSelected}
              aria-level={depth + 1}
              data-node-id={hasChildren ? node.id : undefined}
              tabIndex={depth === 0 && index === 0 ? 0 : -1}
              onClick={() => {
                if (hasChildren) toggleExpand(node.id)
                onSelect?.(node.id)
              }}
              className={cn(
                'w-full flex items-center gap-1.5 py-1 px-2 rounded-sm transition-colors',
                isSelected ? layout.sidePanel.itemActive : layout.sidePanel.itemBg,
                layout.sidePanel.itemText,
                interaction.focusRing,
              )}
              style={{ paddingLeft: `${depth * 12 + 8}px` }}
            >
              {hasChildren ? (
                isExpanded ? <ChevronDown className="w-3.5 h-3.5 flex-shrink-0 text-slate-400" /> : <ChevronRight className="w-3.5 h-3.5 flex-shrink-0 text-slate-400" />
              ) : (
                <span className="w-3.5 flex-shrink-0" />
              )}
              {node.icon && <span className="flex-shrink-0">{node.icon}</span>}
              <span className="truncate flex-1 text-left">{node.label}</span>
              {node.count !== undefined && (
                <span className="text-[10px] text-slate-400 dark:text-slate-600 tabular-nums">{node.count}</span>
              )}
            </button>
            {hasChildren && isExpanded && (
              <TreeView
                nodes={node.children!}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={depth + 1}
              />
            )}
          </div>
        )
      })}
    </div>
  )
}

TreeView.displayName = 'TreeView'
