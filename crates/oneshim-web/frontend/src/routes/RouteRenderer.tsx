import { Suspense } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Spinner } from '../components/ui'
import { RouteErrorBoundary } from './RouteErrorBoundary'
import { routeTree } from './route-tree'

function validateRouteTree() {
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
}

export default function RouteRenderer() {
  validateRouteTree()

  // Sort: leaf routes first, parent routes after, root "/" last.
  // Prevents "/" catch-all from consuming other routes.
  const sorted = [...routeTree].sort((a, b) => {
    if (a.path === '/') return 1
    if (b.path === '/') return -1
    if (a.children && !b.children) return 1
    if (!a.children && b.children) return -1
    return 0
  })

  return (
    <Suspense
      fallback={
        <div className="flex min-h-full items-center justify-center">
          <Spinner size="lg" />
        </div>
      }
    >
      <Routes>
        {sorted.map((node) =>
          node.children ? (
            <Route key={node.path} path={`${node.path}/*`} element={<node.component />}>
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
