import { useMemo } from 'react'
import { useLocation } from 'react-router-dom'
import { type RouteLeaf, type RouteNode, routeTree } from './route-tree'

/**
 * Resolve the current route from the location pathname against routeTree.
 *
 * Returns the matching top-level RouteNode and, if applicable, the active
 * child RouteLeaf. Used by SidePanel, ActivityBar, and TitleBar to derive
 * navigation state from the single source of truth (routeTree) instead of
 * maintaining parallel lookup tables.
 *
 * Matching rules (must stay in sync with RouteRenderer):
 *  - Root path "/" matches exactly OR any of its direct child sub-paths
 *    (e.g., "/overview", "/monitoring", "/insights").
 *  - Other paths match by exact match or prefix (e.g., "/focus/score" matches
 *    the "/focus" route).
 *  - Returns null if no route matches (fallback should be the root route).
 *
 * Assumes single-level nesting — children are direct sub-paths of their parent.
 */
export interface CurrentRoute {
  node: RouteNode
  child: RouteLeaf | null
}

export function useCurrentRoute(): CurrentRoute {
  const location = useLocation()

  return useMemo(() => {
    const pathname = location.pathname

    const node = routeTree.find((r) => {
      if (r.path === '/') {
        if (pathname === '/') return true
        return r.children?.some((c) => pathname === `/${c.path}` || pathname.startsWith(`/${c.path}/`))
      }
      return pathname === r.path || pathname.startsWith(`${r.path}/`)
    })

    // Fallback to root route if nothing matched
    const resolvedNode = node ?? routeTree.find((r) => r.path === '/')
    if (!resolvedNode) {
      // Defensive: routeTree should always have a root route
      throw new Error('[useCurrentRoute] routeTree is missing a root "/" route')
    }

    // Find the active child, if any
    let activeChild: RouteLeaf | null = null
    if (resolvedNode.children && resolvedNode.children.length > 0) {
      if (resolvedNode.path === '/') {
        // Root: match the first segment of the pathname
        const firstSegment = pathname === '/' ? null : pathname.split('/')[1]
        activeChild = resolvedNode.children.find((c) => c.path === firstSegment) ?? null
      } else {
        // Other: match the segment immediately after the parent path
        const remainder = pathname.slice(resolvedNode.path.length).replace(/^\//, '')
        const firstSegment = remainder.split('/')[0]
        activeChild = resolvedNode.children.find((c) => c.path === firstSegment) ?? null
      }
    }

    return { node: resolvedNode, child: activeChild }
  }, [location.pathname])
}
