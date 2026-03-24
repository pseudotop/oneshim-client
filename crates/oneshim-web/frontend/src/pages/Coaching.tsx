import { MessageCircle, Target, TrendingUp } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent, Skeleton } from '../components/ui'
import { useCoachingHistory, useGoalProgress } from '../hooks/useCoaching'
import { colors, iconSize, motion, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

export default function Coaching() {
  const { t } = useTranslation()
  const { data: history, isLoading: histLoading } = useCoachingHistory()
  const { data: goals, isLoading: goalsLoading } = useGoalProgress()

  return (
    <div className="min-h-full p-6">
      <h1 className={cn(typography.h1, colors.text.pageTitle, 'mb-6')}>{t('coaching.title', 'Coaching History')}</h1>

      {/* Goal progress summary */}
      <section className="mb-6">
        <h2 className={cn(typography.h3, 'mb-3')}>{t('coaching.goalsTitle', "Today's Goals")}</h2>
        {goalsLoading ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {[1, 2, 3, 4].map((i) => (
              <Skeleton key={i} className="h-20 w-full" />
            ))}
          </div>
        ) : goals && goals.length > 0 ? (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            {goals.map((g) => (
              <Card key={g.regime_label} variant="default" padding="sm">
                <CardContent>
                  <div className="flex items-center gap-2">
                    <Target className={`${iconSize.base}`} style={{ color: g.display_color }} />
                    <span className={`text-sm ${typography.weight.medium}`}>{g.regime_label}</span>
                  </div>
                  <div className="mt-2 flex items-baseline gap-1">
                    <span className={`text-2xl ${typography.weight.bold}`}>{g.percentage}%</span>
                    <span className="text-content-secondary text-xs">
                      {g.current_minutes}/{g.target_minutes}m
                    </span>
                  </div>
                  <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-muted">
                    <div
                      className={`h-full rounded-full ${motion.all} `.trim()}
                      style={{
                        width: `${Math.min(g.percentage, 100)}%`,
                        backgroundColor: g.display_color,
                      }}
                    />
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          <Card variant="default" padding="md">
            <div className="flex flex-col items-center justify-center py-6 text-center">
              <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-surface-elevated">
                <Target className="h-6 w-6 text-content-muted" />
              </div>
              <p className={cn(`text-sm ${typography.weight.medium}`, colors.text.primary)}>
                {t('coaching.noGoals', 'No goals configured.')}
              </p>
              <p className={cn('mt-1 text-xs', colors.text.secondary)}>
                {t('coaching.noGoalsHint', 'Goals will appear here once regime tracking is active.')}
              </p>
            </div>
          </Card>
        )}
      </section>

      {/* Coaching event timeline */}
      <section>
        <h2 className={cn(typography.h3, 'mb-3')}>{t('coaching.recentEvents', 'Recent Events')}</h2>
        {histLoading ? (
          <div className="space-y-3">
            {[1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-16 w-full" />
            ))}
          </div>
        ) : history && history.length > 0 ? (
          <div className="space-y-3">
            {history.map((evt) => (
              <Card key={evt.event_id} variant="default" padding="sm">
                <CardContent>
                  <div className="flex items-start justify-between">
                    <div className="flex items-center gap-2">
                      <MessageCircle className={`${iconSize.base} text-brand-text`} />
                      <span
                        className={`rounded-md bg-surface-elevated px-2 py-0.5 text-xs ${typography.weight.medium}`}
                      >
                        {evt.profile_name}
                      </span>
                      <span className="text-content-secondary text-xs">{evt.trigger_type}</span>
                    </div>
                    <span className="text-content-secondary text-xs">
                      {new Date(evt.shown_at).toLocaleTimeString()}
                    </span>
                  </div>
                  <p className="mt-1 text-content text-sm">{evt.personalized_message || evt.message_template}</p>
                  {evt.feedback_type && (
                    <div className="mt-1 flex items-center gap-1">
                      <TrendingUp className={`${iconSize.xs}`} />
                      <span className="text-content-secondary text-xs">{evt.feedback_type}</span>
                    </div>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          <Card variant="default" padding="md">
            <div className="flex flex-col items-center justify-center py-6 text-center">
              <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-surface-elevated">
                <MessageCircle className="h-6 w-6 text-content-muted" />
              </div>
              <p className={cn(`text-sm ${typography.weight.medium}`, colors.text.primary)}>
                {t('coaching.noEvents', 'No coaching events yet.')}
              </p>
              <p className={cn('mt-1 text-xs', colors.text.secondary)}>
                {t('coaching.noEventsHint', 'Coaching nudges will appear here as you work.')}
              </p>
            </div>
          </Card>
        )}
      </section>
    </div>
  )
}
