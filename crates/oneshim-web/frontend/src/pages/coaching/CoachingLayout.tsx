import { useTranslation } from 'react-i18next'
import { Outlet } from 'react-router-dom'
import type { CoachingEvent, GoalProgress } from '../../api/coaching'
import HabitTrackerWidget from '../../components/HabitTrackerWidget'
import { useCoachingHistory, useGoalProgress } from '../../hooks/useCoaching'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export interface CoachingOutletContext {
  history: CoachingEvent[] | undefined
  histLoading: boolean
  goals: GoalProgress[] | undefined
  goalsLoading: boolean
}

export default function CoachingLayout() {
  const { t } = useTranslation()
  const { data: history, isLoading: histLoading } = useCoachingHistory()
  const { data: goals, isLoading: goalsLoading } = useGoalProgress()

  const ctx: CoachingOutletContext = { history, histLoading, goals, goalsLoading }

  return (
    <div className="min-h-full p-6">
      <h1 className={cn(typography.h1, colors.text.pageTitle, 'mb-6')}>{t('coaching.title', 'Coaching History')}</h1>

      <Outlet context={ctx} />

      {/* Habit streak tracker */}
      <section className="mb-6">
        <HabitTrackerWidget />
      </section>
    </div>
  )
}
