import { AlertTriangle } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '../components/ui'
import { iconSize, radius, typography } from '../styles/tokens'

interface RouteErrorFallbackProps {
  error: Error
  route: string
  componentStack?: string
  onRetry: () => void
  onGoHome: () => void
}

export function RouteErrorFallback({ error, route, componentStack, onRetry, onGoHome }: RouteErrorFallbackProps) {
  const { t } = useTranslation()
  const [detailsOpen, setDetailsOpen] = useState(false)

  return (
    <div className={`flex flex-1 items-center justify-center p-8 ${radius.md}`} role="alert">
      <div className="w-full max-w-md text-center">
        {/* Error icon */}
        <div className="mb-4 flex justify-center">
          <div className="inline-flex items-center justify-center rounded-full bg-semantic-error/10 p-3">
            <AlertTriangle className={`${iconSize.lg} text-semantic-error`} />
          </div>
        </div>

        {/* Title and description */}
        <h2 className={`mb-2 ${typography.h3} text-content`}>{t('errors.route.title')}</h2>
        <p className={`mb-6 ${typography.body} text-content-secondary`}>{t('errors.route.description')}</p>

        {/* Action buttons */}
        <div className="flex items-center justify-center gap-3">
          <Button variant="primary" size="md" onClick={onRetry}>
            {t('errors.route.tryAgain')}
          </Button>
          <Button variant="secondary" size="md" onClick={onGoHome}>
            {t('errors.route.goHome')}
          </Button>
        </div>

        {/* Dev-only debug details */}
        {import.meta.env.DEV && (
          <details
            className="mt-6 text-left"
            open={detailsOpen}
            onToggle={(e) => setDetailsOpen((e.target as HTMLDetailsElement).open)}
          >
            <summary className={`cursor-pointer select-none ${typography.caption} text-content-tertiary`}>
              Debug: {route} — {error.name}
            </summary>
            <div className="mt-2 space-y-2">
              <pre className="overflow-auto rounded bg-surface-muted p-3 text-content-secondary text-xs">
                {error.stack ?? error.message}
              </pre>
              {componentStack && (
                <pre className="overflow-auto rounded bg-surface-muted p-3 text-content-tertiary text-xs">
                  {componentStack}
                </pre>
              )}
            </div>
          </details>
        )}
      </div>
    </div>
  )
}
