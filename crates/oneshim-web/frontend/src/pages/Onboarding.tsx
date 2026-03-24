/**
 * First-run onboarding page — 4-step guide shown before the main shell.
 */

import { Camera, ChevronLeft, ChevronRight, Cpu, Lightbulb, Monitor, Rocket, Shield } from 'lucide-react'
import { type ReactNode, useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '../components/ui'
import { colors, iconSize, motion, radius, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

interface OnboardingProps {
  onComplete: () => void
}

const TOTAL_STEPS = 4

async function invokeCommand(cmd: string) {
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke(cmd)
  } catch {
    // Standalone / dev mode — no Tauri runtime
  }
}

/* ── Step content components ── */

function StepIntro() {
  const { t } = useTranslation()
  return (
    <div className="flex flex-col items-center text-center">
      <div className={cn('mb-6 flex items-center justify-center rounded-full bg-brand-signal/15 p-4', motion.opacity)}>
        <Rocket className={cn(iconSize.hero, 'text-brand-text')} />
      </div>
      <h2 className={cn(typography.h1, colors.text.primary, 'mb-3')}>{t('onboarding.step1Title')}</h2>
      <p className={cn(typography.body, colors.text.secondary, 'max-w-sm')}>{t('onboarding.step1Desc')}</p>
    </div>
  )
}

function PermissionRow({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className={cn('flex items-center gap-3 rounded-lg bg-surface-muted px-4 py-3', motion.colors)}>
      <div className="flex items-center justify-center rounded-md bg-brand-signal/15 p-2">{icon}</div>
      <span className={cn(typography.body, typography.weight.medium, colors.text.primary)}>{label}</span>
    </div>
  )
}

function StepPermissions() {
  const { t } = useTranslation()
  const iconCls = cn(iconSize.base, 'text-brand-text')
  return (
    <div className="flex flex-col items-center text-center">
      <div className={cn('mb-6 flex items-center justify-center rounded-full bg-brand-signal/15 p-4', motion.opacity)}>
        <Shield className={cn(iconSize.hero, 'text-brand-text')} />
      </div>
      <h2 className={cn(typography.h1, colors.text.primary, 'mb-3')}>{t('onboarding.step2Title')}</h2>
      <p className={cn(typography.body, colors.text.secondary, 'mb-6 max-w-sm')}>{t('onboarding.step2Desc')}</p>
      <div className="flex w-full max-w-xs flex-col gap-3">
        <PermissionRow icon={<Monitor className={iconCls} />} label={t('onboarding.step2Accessibility')} />
        <PermissionRow icon={<Camera className={iconCls} />} label={t('onboarding.step2ScreenCapture')} />
        <PermissionRow icon={<Lightbulb className={iconCls} />} label={t('onboarding.step2Notifications')} />
      </div>
    </div>
  )
}

function FeatureItem({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className={cn('flex items-center gap-3 rounded-lg bg-surface-muted px-4 py-3', motion.colors)}>
      <div className="flex items-center justify-center rounded-md bg-brand-signal/15 p-2">{icon}</div>
      <span className={cn(typography.body, typography.weight.medium, colors.text.primary)}>{label}</span>
    </div>
  )
}

function StepFeatures() {
  const { t } = useTranslation()
  const iconCls = cn(iconSize.base, 'text-brand-text')
  return (
    <div className="flex flex-col items-center text-center">
      <div className={cn('mb-6 flex items-center justify-center rounded-full bg-brand-signal/15 p-4', motion.opacity)}>
        <Cpu className={cn(iconSize.hero, 'text-brand-text')} />
      </div>
      <h2 className={cn(typography.h1, colors.text.primary, 'mb-3')}>{t('onboarding.step3Title')}</h2>
      <p className={cn(typography.body, colors.text.secondary, 'mb-6 max-w-sm')}>{t('onboarding.step3Desc')}</p>
      <div className="flex w-full max-w-xs flex-col gap-3">
        <FeatureItem icon={<Camera className={iconCls} />} label={t('onboarding.step3Capture')} />
        <FeatureItem icon={<Cpu className={iconCls} />} label={t('onboarding.step3Analysis')} />
        <FeatureItem icon={<Lightbulb className={iconCls} />} label={t('onboarding.step3Suggestions')} />
      </div>
    </div>
  )
}

function StepReady() {
  const { t } = useTranslation()
  return (
    <div className="flex flex-col items-center text-center">
      <div className={cn('mb-6 flex items-center justify-center rounded-full bg-brand-signal/15 p-4', motion.opacity)}>
        <Rocket className={cn(iconSize.hero, 'text-brand-text')} />
      </div>
      <h2 className={cn(typography.h1, colors.text.primary, 'mb-3')}>{t('onboarding.step4Title')}</h2>
      <p className={cn(typography.body, colors.text.secondary, 'max-w-sm')}>{t('onboarding.step4Desc')}</p>
    </div>
  )
}

const STEPS = [StepIntro, StepPermissions, StepFeatures, StepReady]

/* ── Step indicator dots ── */

function StepDots({ current, total }: { current: number; total: number }) {
  const steps = Array.from({ length: total }, (_, i) => ({ id: `step-${i + 1}`, index: i }))
  return (
    <fieldset className="m-0 flex items-center gap-2 border-none p-0" aria-label="Step indicator">
      {steps.map((step) => (
        <div
          key={step.id}
          className={cn(
            'h-2 rounded-full transition-all duration-300',
            step.index === current ? 'w-6 bg-brand-signal' : 'w-2 bg-surface-muted',
          )}
          aria-current={step.index === current ? 'step' : undefined}
        />
      ))}
    </fieldset>
  )
}

/* ── Main onboarding component ── */

export default function Onboarding({ onComplete }: OnboardingProps) {
  const { t } = useTranslation()
  const [step, setStep] = useState(0)

  const handleComplete = useCallback(async () => {
    await invokeCommand('complete_onboarding')
    onComplete()
  }, [onComplete])

  const handleSkip = useCallback(() => {
    onComplete()
  }, [onComplete])

  const StepContent = STEPS[step]
  const isFirst = step === 0
  const isLast = step === TOTAL_STEPS - 1

  return (
    <div className="flex min-h-screen items-center justify-center bg-surface-sunken p-4">
      <div className={cn('flex w-full max-w-lg flex-col items-center', radius.lg, 'bg-surface-elevated p-8 shadow-xl')}>
        {/* Step dots */}
        <div className="mb-8">
          <StepDots current={step} total={TOTAL_STEPS} />
        </div>

        {/* Step content */}
        <div className="mb-8 w-full">
          <StepContent />
        </div>

        {/* Navigation buttons */}
        <div className="flex w-full items-center justify-between">
          <div>
            {!isFirst && (
              <Button variant="ghost" size="md" onClick={() => setStep((s) => s - 1)}>
                <ChevronLeft className={cn(iconSize.base, 'mr-1')} />
                {t('onboarding.back')}
              </Button>
            )}
          </div>

          <div className="flex items-center gap-3">
            <Button variant="ghost" size="md" onClick={handleSkip}>
              {t('onboarding.skip')}
            </Button>

            {isLast ? (
              <Button variant="primary" size="md" onClick={handleComplete}>
                {t('onboarding.complete')}
              </Button>
            ) : (
              <Button variant="primary" size="md" onClick={() => setStep((s) => s + 1)}>
                {t('onboarding.next')}
                <ChevronRight className={cn(iconSize.base, 'ml-1')} />
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
