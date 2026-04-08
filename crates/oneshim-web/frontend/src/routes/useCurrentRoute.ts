import { useMemo } from 'react'
import { useLocation } from 'react-router-dom'
import { type RouteLeaf, type RouteNode, routeTree } from './route-tree'

/**
 * Determine whether a given pathname maps to the specified route node.
 *
 * Single source of truth for pathname-to-route matching. Used by
 * `useCurrentRoute` (which iterates routeTree to find ONE matching node)
 * and `ActivityBar` (which tests EACH top-level node for active state).
 *
 * Matching rules (must stay in sync with RouteRenderer):
 *  - Root path "/" matches exactly OR any of its direct child sub-paths
 *    (e.g., "/overview", "/monitoring", "/insights").
 *  - Other paths match by exact match or prefix (e.g., "/focus/score"
 *    matches the "/focus" route).
 *
 * Assumes single-level nesting — children are direct sub-paths.
 */
export function matchesRoute(node: RouteNode, pathname: string): boolean {
  if (node.path === '/') {
    if (pathname === '/') return true
    return node.children?.some((c) => pathname === `/${c.path}` || pathname.startsWith(`/${c.path}/`)) ?? false
  }
  return pathname === node.path || pathname.startsWith(`${node.path}/`)
}

/**
 * Resolve the current route from the location pathname against routeTree.
 *
 * Returns the matching top-level RouteNode and, if applicable, the active
 * child RouteLeaf. Used by SidePanel and TitleBar to derive navigation
 * state from the single source of truth (routeTree).
 */
export interface CurrentRoute {
  node: RouteNode
  child: RouteLeaf | null
}

export function useCurrentRoute(): CurrentRoute {
  const location = useLocation()

  return useMemo(() => {
    const pathname = location.pathname

    const node = routeTree.find((r) => matchesRoute(r, pathname))

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
