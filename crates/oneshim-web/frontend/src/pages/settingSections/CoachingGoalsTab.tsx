import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Card, CardContent, CardHeader, CardTitle, Input } from '../../components/ui'
import { useGoalProgress, useUpdateGoals } from '../../hooks/useCoaching'
import { typography } from '../../styles/tokens'

export default function CoachingGoalsTab() {
  const { t } = useTranslation()
  const { data: goals } = useGoalProgress()
  const updateGoalsMutation = useUpdateGoals()
  const [newLabel, setNewLabel] = useState('')
  const [newMinutes, setNewMinutes] = useState(60)

  const handleAdd = useCallback(() => {
    if (!newLabel.trim()) return
    const current: Record<string, number> = {}
    for (const g of goals ?? []) {
      current[g.regime_label] = g.target_minutes
    }
    current[newLabel.trim()] = newMinutes
    updateGoalsMutation.mutate(current)
    setNewLabel('')
    setNewMinutes(60)
  }, [newLabel, newMinutes, goals, updateGoalsMutation])

  const handleDelete = useCallback(
    (label: string) => {
      const current: Record<string, number> = {}
      for (const g of goals ?? []) {
        if (g.regime_label !== label) {
          current[g.regime_label] = g.target_minutes
        }
      }
      updateGoalsMutation.mutate(current)
    },
    [goals, updateGoalsMutation],
  )

  return (
    <div className="space-y-6">
      <Card variant="default" padding="md">
        <CardHeader>
          <CardTitle>{t('coaching.settingsTitle', 'Coaching Goals')}</CardTitle>
        </CardHeader>
        <CardContent>
          {/* Existing goals list */}
          {goals && goals.length > 0 ? (
            <div className="mb-4 space-y-2">
              {goals.map((g) => (
                <div key={g.regime_label} className="flex items-center gap-3">
                  <span className={`w-32 truncate text-sm ${typography.weight.medium}`}>{g.regime_label}</span>
                  <span className="text-sm text-content-secondary">
                    {g.target_minutes} {t('coaching.perDay', 'min/day')}
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => handleDelete(g.regime_label)}>
                    {t('coaching.remove', 'Remove')}
                  </Button>
                </div>
              ))}
            </div>
          ) : (
            <p className="mb-4 text-sm text-content-secondary">
              {t('coaching.noGoals', 'No goals set. Add a regime goal below.')}
            </p>
          )}

          {/* Add new goal form */}
          <div className="flex items-end gap-2">
            <div>
              <label htmlFor="coaching-regime-label" className="mb-1 block text-xs text-content-secondary">
                {t('coaching.regimeLabel', 'Regime Label')}
              </label>
              <Input
                id="coaching-regime-label"
                value={newLabel}
                onChange={(e) => setNewLabel(e.target.value)}
                placeholder="e.g. Deep Coding"
                className="w-40"
              />
            </div>
            <div>
              <label htmlFor="coaching-target-minutes" className="mb-1 block text-xs text-content-secondary">
                {t('coaching.targetMinutes', 'Target (min)')}
              </label>
              <Input
                id="coaching-target-minutes"
                type="number"
                value={newMinutes}
                onChange={(e) => setNewMinutes(Number(e.target.value))}
                min={1}
                max={1440}
                className="w-24"
              />
            </div>
            <Button variant="primary" size="sm" onClick={handleAdd}>
              {t('coaching.addGoal', 'Add Goal')}
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
