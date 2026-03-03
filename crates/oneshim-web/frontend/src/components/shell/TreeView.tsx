import { useState } from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { layout } from '../../styles/tokens'
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

  const toggleExpand = (id: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className="text-sm">
      {nodes.map(node => {
        const hasChildren = node.children && node.children.length > 0
        const isExpanded = expanded.has(node.id)
        const isSelected = selectedId === node.id

        return (
          <div key={node.id}>
            <button
              onClick={() => {
                if (hasChildren) toggleExpand(node.id)
                onSelect?.(node.id)
              }}
              className={cn(
                'w-full flex items-center gap-1.5 py-1 px-2 rounded-sm transition-colors',
                isSelected ? layout.sidePanel.itemActive : layout.sidePanel.itemBg,
                layout.sidePanel.itemText,
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
