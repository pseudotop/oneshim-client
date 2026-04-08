import { Suspense } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Spinner } from '../components/ui'
import { RouteErrorBoundary } from './RouteErrorBoundary'
import { routeTree } from './route-tree'

// Validate routeTree once at module load (dev only).
// Catches misconfiguration before any render — fail fast.
if (import.meta.env.DEV) {
  for (const node of routeTree) {
    if (node.children && node.defaultChild) {
      const match = node.children.some((c) => c.path === node.defaultChild)
      if (!match) {
        throw new Error(
          `[RouteRenderer] ${node.path} defaultChild="${node.defaultChild}" ` +
            `not found in children: [${node.children.map((c) => c.path).join(', ')}]`,
        )
      }
    }
    if (node.children && !node.defaultChild) {
      throw new Error(`[RouteRenderer] ${node.path} has children but no defaultChild`)
    }
  }
}

// Sort once at module load: leaf routes first, parent routes after, root "/" last.
// Prevents "/" catch-all from consuming other routes.
const sortedRouteTree = [...routeTree].sort((a, b) => {
  if (a.path === '/') return 1
  if (b.path === '/') return -1
  if (a.children && !b.children) return 1
  if (!a.children && b.children) return -1
  return 0
})

export default function RouteRenderer() {
  return (
    <Suspense
      fallback={
        <div className="flex min-h-full items-center justify-center">
          <Spinner size="lg" />
        </div>
      }
    >
      <Routes>
        {sortedRouteTree.map((node) =>
          node.children ? (
            // Parent layouts get exactly ONE outer RouteErrorBoundary so a
            // crash anywhere in the layout (header, useQuery, Outlet child)
            // is isolated per-route. Earlier revisions also wrapped the
            // <Outlet> inside each Layout with a second boundary, but two
            // boundaries on the same route key produced duplicate
            // recoverySignals subscribers — halving the escalation threshold
            // from 3 to 2 (commit e5bab2e1). Do not re-introduce the inner
            // wrapping without also re-keying the recovery channel.
            <Route
              key={node.path}
              path={`${node.path}/*`}
              element={
                <RouteErrorBoundary route={node.path}>
                  <node.component />
                </RouteErrorBoundary>
              }
            >
              {node.defaultChild && <Route index element={<Navigate to={node.defaultChild} replace />} />}
              {node.children.map((child) => (
                <Route key={child.path} path={child.path} element={<child.component />} />
              ))}
            </Route>
          ) : (
            <Route
              key={node.path}
              path={node.path}
              element={
                <RouteErrorBoundary route={node.path}>
                  <node.component />
                </RouteErrorBoundary>
              }
            />
          ),
        )}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Suspense>
  )
}
