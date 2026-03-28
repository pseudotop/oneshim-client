# Storybook & Design System Completion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract 4 missing UI primitives (Divider, Alert, Dialog, Checkbox), upgrade all Storybook stories to minimum quality bar with autodocs + mock data, and add MDX documentation pages.

**Architecture:** Additive-only changes — new primitives in `src/components/ui/`, new variants in `variants.ts`, new stories co-located with components. No existing component APIs change. No new npm dependencies. All patterns follow established forwardRef + cn() + variants conventions.

**Tech Stack:** React 18, TypeScript 5.6, Tailwind CSS 3.4, Storybook 10.2, Vite 5

**Spec:** `docs/STORYBOOK-DESIGN-SYSTEM-SPEC.md` (v3)

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `src/components/ui/Divider.tsx` | Semantic separator primitive (horizontal/vertical) |
| `src/components/ui/Divider.stories.tsx` | Divider stories (3 stories) |
| `src/components/ui/Alert.tsx` | Info/warning/error box primitive |
| `src/components/ui/Alert.stories.tsx` | Alert stories (6 stories) |
| `src/components/ui/Dialog.tsx` | Modal dialog primitive (overlay + focus trap) |
| `src/components/ui/Dialog.stories.tsx` | Dialog stories (4 stories) |
| `src/components/ui/Checkbox.tsx` | Checkbox with label primitive |
| `src/components/ui/Checkbox.stories.tsx` | Checkbox stories (5 stories) |
| `src/stories/mock-data.ts` | Factory functions for page story mock data |
| `src/stories/GettingStarted.mdx` | Storybook onboarding doc page |
| `src/stories/ComponentPatterns.mdx` | Component pattern guide doc page |

### Modified Files

| File | Change |
|------|--------|
| `src/styles/variants.ts` | Append `alertVariants` + `dialogVariants` |
| `src/components/ui/index.ts` | Add exports for 4 new primitives |
| All 76 existing `*.stories.tsx` files | Add `tags: ['autodocs']` to meta |
| ~20 thin story files | Add additional story variants |

---

## Phase A: UI Primitives

### Task 0: Update Storybook config for MDX support

**Files:**
- Modify: `.storybook/main.ts`

- [ ] **Step 1: Add MDX pattern to stories glob**

In `.storybook/main.ts`, update the `stories` array to include MDX files:

```ts
// BEFORE
stories: ['../src/**/*.stories.@(ts|tsx)'],

// AFTER
stories: ['../src/**/*.mdx', '../src/**/*.stories.@(ts|tsx)'],
```

This is required for the MDX documentation pages in Phase C (GettingStarted.mdx, ComponentPatterns.mdx).

- [ ] **Step 2: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 3: Commit**

```bash
git add .storybook/main.ts
git commit -m "build(frontend): add MDX support to Storybook stories config"
```

---

### Task 1: Divider Primitive

**Files:**
- Create: `src/components/ui/Divider.tsx`
- Create: `src/components/ui/Divider.stories.tsx`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Create Divider component**

```tsx
// src/components/ui/Divider.tsx
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'

export interface DividerProps extends React.HTMLAttributes<HTMLHRElement> {
  orientation?: 'horizontal' | 'vertical'
}

export const Divider = forwardRef<HTMLHRElement, DividerProps>(
  ({ className, orientation = 'horizontal', ...props }, ref) => {
    return (
      <hr
        ref={ref}
        role="separator"
        aria-orientation={orientation}
        className={cn(
          'border-DEFAULT border-0',
          orientation === 'horizontal' ? 'w-full border-t' : 'h-full border-l',
          className,
        )}
        {...props}
      />
    )
  },
)

Divider.displayName = 'Divider'
```

- [ ] **Step 2: Create Divider stories**

```tsx
// src/components/ui/Divider.stories.tsx
import type { Meta, StoryObj } from '@storybook/react'
import { Divider } from './Divider'

const meta = {
  title: 'UI Primitives/Divider',
  component: Divider,
  tags: ['autodocs'],
  argTypes: {
    orientation: {
      control: 'radio',
      options: ['horizontal', 'vertical'],
    },
  },
} satisfies Meta<typeof Divider>

export default meta
type Story = StoryObj<typeof meta>

export const Horizontal: Story = {
  args: { orientation: 'horizontal' },
  decorators: [
    (Story) => (
      <div className="w-64 space-y-3 p-4">
        <p className="text-content text-sm">Above</p>
        <Story />
        <p className="text-content text-sm">Below</p>
      </div>
    ),
  ],
}

export const Vertical: Story = {
  args: { orientation: 'vertical' },
  decorators: [
    (Story) => (
      <div className="flex h-12 items-center gap-3 p-4">
        <span className="text-content text-sm">Left</span>
        <Story />
        <span className="text-content text-sm">Right</span>
      </div>
    ),
  ],
}

export const InContext: Story = {
  render: () => (
    <div className="w-72 space-y-3 rounded-lg bg-surface-elevated p-4">
      <p className="font-medium text-content text-sm">Section 1</p>
      <Divider />
      <p className="font-medium text-content text-sm">Section 2</p>
      <Divider />
      <p className="font-medium text-content text-sm">Section 3</p>
    </div>
  ),
}
```

- [ ] **Step 3: Add Divider export to index.ts**

Append to `src/components/ui/index.ts`:
```ts
export { Divider, type DividerProps } from './Divider'
```

- [ ] **Step 4: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds with no errors.

- [ ] **Step 5: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 6: Commit**

```bash
git add src/components/ui/Divider.tsx src/components/ui/Divider.stories.tsx src/components/ui/index.ts
git commit -m "feat(frontend): add Divider UI primitive with stories"
```

---

### Task 2: Alert Primitive

**Files:**
- Create: `src/components/ui/Alert.tsx`
- Create: `src/components/ui/Alert.stories.tsx`
- Modify: `src/styles/variants.ts`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Add alertVariants to variants.ts**

Append to `src/styles/variants.ts` after `selectVariants`:

```ts
export const alertVariants = {
  variant: {
    default: 'bg-surface-muted border border-DEFAULT text-content',
    info: 'bg-semantic-info/10 border border-semantic-info/30 text-content',
    success: 'bg-semantic-success/10 border border-semantic-success/30 text-content',
    warning: 'bg-semantic-warning/10 border border-semantic-warning/30 text-content',
    error: 'bg-semantic-error/10 border border-semantic-error/30 text-content',
  },
  iconColor: {
    default: 'text-content-secondary',
    info: 'text-semantic-info',
    success: 'text-semantic-success',
    warning: 'text-semantic-warning',
    error: 'text-semantic-error',
  },
} as const
```

- [ ] **Step 2: Create Alert component**

```tsx
// src/components/ui/Alert.tsx
import type { ReactNode } from 'react'
import { forwardRef } from 'react'
import { iconSize, radius, typography } from '../../styles/tokens'
import { alertVariants } from '../../styles/variants'
import { cn } from '../../utils/cn'

export interface AlertProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: keyof typeof alertVariants.variant
  title?: string
  icon?: ReactNode
}

export const Alert = forwardRef<HTMLDivElement, AlertProps>(
  ({ className, variant = 'default', title, icon, children, ...props }, ref) => {
    const semanticRole = variant === 'error' || variant === 'warning' ? 'alert' : 'status'

    return (
      <div
        ref={ref}
        role={semanticRole}
        className={cn(radius.md, 'p-4', alertVariants.variant[variant], className)}
        {...props}
      >
        <div className="flex gap-3">
          {icon && (
            <div className={cn(iconSize.md, 'mt-0.5 shrink-0', alertVariants.iconColor[variant])}>{icon}</div>
          )}
          <div className="min-w-0 flex-1">
            {title && <p className={cn(typography.label, 'mb-1 text-content')}>{title}</p>}
            <div className={cn(typography.body, 'text-content-secondary')}>{children}</div>
          </div>
        </div>
      </div>
    )
  },
)

Alert.displayName = 'Alert'
```

- [ ] **Step 3: Create Alert stories**

```tsx
// src/components/ui/Alert.stories.tsx
import type { Meta, StoryObj } from '@storybook/react'
import { AlertCircle, CheckCircle, Info, TriangleAlert } from 'lucide-react'
import { Alert } from './Alert'

const meta = {
  title: 'UI Primitives/Alert',
  component: Alert,
  tags: ['autodocs'],
  argTypes: {
    variant: {
      control: 'select',
      options: ['default', 'info', 'success', 'warning', 'error'],
    },
  },
} satisfies Meta<typeof Alert>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    title: 'Note',
    children: 'This is a default informational alert.',
  },
}

export const Info: Story = {
  args: {
    variant: 'info',
    icon: <Info className="h-5 w-5" />,
    title: 'Information',
    children: 'Your data is synced every 10 seconds.',
  },
}

export const Success: Story = {
  args: {
    variant: 'success',
    icon: <CheckCircle className="h-5 w-5" />,
    title: 'Success',
    children: 'Settings saved successfully.',
  },
}

export const Warning: Story = {
  args: {
    variant: 'warning',
    icon: <TriangleAlert className="h-5 w-5" />,
    title: 'Warning',
    children: 'Storage usage exceeds 80%.',
  },
}

export const Error: Story = {
  args: {
    variant: 'error',
    icon: <AlertCircle className="h-5 w-5" />,
    title: 'Error',
    children: 'Connection to server failed.',
  },
}

export const AllVariants: Story = {
  render: () => (
    <div className="max-w-md space-y-3">
      <Alert variant="default" title="Default">Neutral message.</Alert>
      <Alert variant="info" icon={<Info className="h-5 w-5" />} title="Info">Informational message.</Alert>
      <Alert variant="success" icon={<CheckCircle className="h-5 w-5" />} title="Success">Something worked.</Alert>
      <Alert variant="warning" icon={<TriangleAlert className="h-5 w-5" />} title="Warning">Be careful.</Alert>
      <Alert variant="error" icon={<AlertCircle className="h-5 w-5" />} title="Error">Something broke.</Alert>
    </div>
  ),
}
```

- [ ] **Step 4: Add Alert export to index.ts**

Append to `src/components/ui/index.ts`:
```ts
export { Alert, type AlertProps } from './Alert'
```

- [ ] **Step 5: Verify build + Storybook**

Run: `cd crates/oneshim-web/frontend && pnpm build && pnpm build-storybook 2>&1 | tail -5`
Expected: Both succeed.

- [ ] **Step 6: Commit**

```bash
git add src/components/ui/Alert.tsx src/components/ui/Alert.stories.tsx src/styles/variants.ts src/components/ui/index.ts
git commit -m "feat(frontend): add Alert UI primitive with 5 variants"
```

---

### Task 3: Dialog Primitive

**Files:**
- Create: `src/components/ui/Dialog.tsx`
- Create: `src/components/ui/Dialog.stories.tsx`
- Modify: `src/styles/variants.ts`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Add dialogVariants to variants.ts**

Append to `src/styles/variants.ts`:

```ts
export const dialogVariants = {
  size: {
    sm: 'max-w-sm',
    md: 'max-w-lg',
    lg: 'max-w-xl',
  },
} as const
```

- [ ] **Step 2: Create Dialog component**

```tsx
// src/components/ui/Dialog.tsx
import { type ReactNode, useCallback, useEffect, useRef } from 'react'
import { elevation, layout, motion, radius } from '../../styles/tokens'
import { dialogVariants } from '../../styles/variants'
import { cn } from '../../utils/cn'

export interface DialogProps {
  open: boolean
  onClose: () => void
  children: ReactNode
}

export function Dialog({ open, onClose, children }: DialogProps) {
  const previousFocusRef = useRef<HTMLElement | null>(null)

  useEffect(() => {
    if (open) {
      previousFocusRef.current = document.activeElement as HTMLElement
      document.body.style.overflow = 'hidden'
    } else {
      document.body.style.overflow = ''
      previousFocusRef.current?.focus()
    }
    return () => {
      document.body.style.overflow = ''
    }
  }, [open])

  useEffect(() => {
    if (!open) return
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        onClose()
      }
    }
    document.addEventListener('keydown', handler)
    return () => document.removeEventListener('keydown', handler)
  }, [open, onClose])

  if (!open) return null

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: Escape handled via document keydown
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop click-outside close
    <div
      className={cn('fixed inset-0 z-overlay flex items-center justify-center', layout.commandPalette.overlay)}
      onClick={onClose}
    >
      {children}
    </div>
  )
}

export interface DialogContentProps extends React.HTMLAttributes<HTMLDivElement> {
  size?: keyof typeof dialogVariants.size
}

export function DialogContent({ className, size = 'md', children, ...props }: DialogContentProps) {
  const ref = useRef<HTMLDivElement>(null)

  // Focus trap
  useEffect(() => {
    const el = ref.current
    if (!el) return

    const handleTab = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return
      const focusable = el.querySelectorAll<HTMLElement>(
        'input, button, textarea, select, a[href], [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }

    // Auto-focus first focusable element
    const timer = setTimeout(() => {
      const firstFocusable = el.querySelector<HTMLElement>(
        'input, button, textarea, select, a[href], [tabindex]:not([tabindex="-1"])',
      )
      firstFocusable?.focus()
    }, 50)

    document.addEventListener('keydown', handleTab)
    return () => {
      clearTimeout(timer)
      document.removeEventListener('keydown', handleTab)
    }
  }, [])

  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: click events do not need key handlers here
    // biome-ignore lint/a11y/noStaticElementInteractions: stops click-outside from firing on content
    <div
      ref={ref}
      role="dialog"
      aria-modal="true"
      className={cn(
        'w-full',
        radius.lg,
        elevation.dialog,
        motion.opacity,
        layout.commandPalette.bg,
        layout.commandPalette.border,
        dialogVariants.size[size],
        className,
      )}
      onClick={(e) => e.stopPropagation()}
      {...props}
    >
      {children}
    </div>
  )
}

export function DialogTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn('p-4 pb-0 font-semibold text-content text-lg', className)} {...props} />
}

export function DialogBody({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('p-4 text-content-secondary text-sm', className)} {...props} />
}

export function DialogFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('flex justify-end gap-2 border-DEFAULT border-t p-4', className)} {...props} />
}
```

- [ ] **Step 3: Create Dialog stories**

```tsx
// src/components/ui/Dialog.stories.tsx
import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'
import { Button } from './Button'
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogTitle } from './Dialog'

function DialogDemo({ size = 'md' }: { size?: 'sm' | 'md' | 'lg' }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button onClick={() => setOpen(true)}>Open Dialog</Button>
      <Dialog open={open} onClose={() => setOpen(false)}>
        <DialogContent size={size}>
          <DialogTitle>Confirm Action</DialogTitle>
          <DialogBody>Are you sure you want to proceed? This action cannot be undone.</DialogBody>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
            <Button variant="primary" onClick={() => setOpen(false)}>Confirm</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}

const meta = {
  title: 'UI Primitives/Dialog',
  component: Dialog,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
} satisfies Meta<typeof Dialog>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  render: () => <DialogDemo />,
}

export const Small: Story = {
  render: () => <DialogDemo size="sm" />,
}

export const Large: Story = {
  render: () => <DialogDemo size="lg" />,
}

export const DangerConfirm: Story = {
  render: () => {
    const [open, setOpen] = useState(false)
    return (
      <>
        <Button variant="danger" onClick={() => setOpen(true)}>Delete Data</Button>
        <Dialog open={open} onClose={() => setOpen(false)}>
          <DialogContent size="sm">
            <DialogTitle>Delete All Data?</DialogTitle>
            <DialogBody>This will permanently remove all stored activity data. This cannot be undone.</DialogBody>
            <DialogFooter>
              <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
              <Button variant="danger" onClick={() => setOpen(false)}>Delete</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </>
    )
  },
}
```

- [ ] **Step 4: Add Dialog exports to index.ts**

Append to `src/components/ui/index.ts`:
```ts
export { Dialog, DialogBody, DialogContent, type DialogContentProps, DialogFooter, type DialogProps, DialogTitle } from './Dialog'
```

- [ ] **Step 5: Verify build + Storybook**

Run: `cd crates/oneshim-web/frontend && pnpm build && pnpm build-storybook 2>&1 | tail -5`
Expected: Both succeed.

- [ ] **Step 6: Commit**

```bash
git add src/components/ui/Dialog.tsx src/components/ui/Dialog.stories.tsx src/styles/variants.ts src/components/ui/index.ts
git commit -m "feat(frontend): add Dialog UI primitive with focus trap + keyboard support"
```

---

### Task 4: Checkbox Primitive

**Files:**
- Create: `src/components/ui/Checkbox.tsx`
- Create: `src/components/ui/Checkbox.stories.tsx`
- Modify: `src/components/ui/index.ts`

- [ ] **Step 1: Create Checkbox component**

```tsx
// src/components/ui/Checkbox.tsx
import { forwardRef, useId } from 'react'
import { form, interaction, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export interface CheckboxProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string
  description?: string
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(
  ({ className, label, description, id: externalId, ...props }, ref) => {
    const autoId = useId()
    const id = externalId ?? autoId

    if (!label) {
      return (
        <input ref={ref} id={id} type="checkbox" className={cn(form.checkbox, className)} {...props} />
      )
    }

    return (
      <div className="flex items-start gap-3">
        <input
          ref={ref}
          id={id}
          type="checkbox"
          className={cn(form.checkbox, 'mt-0.5', className)}
          {...props}
        />
        <div>
          <label htmlFor={id} className={cn(typography.label, 'cursor-pointer text-content')}>
            {label}
          </label>
          {description && (
            <p className={cn(typography.caption, 'mt-0.5 text-content-secondary')}>{description}</p>
          )}
        </div>
      </div>
    )
  },
)

Checkbox.displayName = 'Checkbox'
```

- [ ] **Step 2: Create Checkbox stories**

```tsx
// src/components/ui/Checkbox.stories.tsx
import type { Meta, StoryObj } from '@storybook/react'
import { Checkbox } from './Checkbox'

const meta = {
  title: 'UI Primitives/Checkbox',
  component: Checkbox,
  tags: ['autodocs'],
  argTypes: {
    checked: { control: 'boolean' },
    disabled: { control: 'boolean' },
  },
} satisfies Meta<typeof Checkbox>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: { label: 'Enable notifications' },
}

export const WithDescription: Story = {
  args: {
    label: 'Auto-update',
    description: 'Automatically install updates when available.',
  },
}

export const Checked: Story = {
  args: { label: 'I agree to the terms', checked: true, readOnly: true },
}

export const Disabled: Story = {
  args: { label: 'Premium feature', disabled: true },
}

export const Bare: Story = {
  args: {},
  decorators: [
    (Story) => (
      <div className="flex items-center gap-2">
        <Story />
        <span className="text-content text-sm">Bare checkbox (no label prop)</span>
      </div>
    ),
  ],
}
```

- [ ] **Step 3: Add Checkbox export to index.ts**

Append to `src/components/ui/index.ts`:
```ts
export { Checkbox, type CheckboxProps } from './Checkbox'
```

- [ ] **Step 4: Verify build + Storybook + lint**

Run: `cd crates/oneshim-web/frontend && pnpm build && pnpm lint && pnpm build-storybook 2>&1 | tail -5`
Expected: All pass.

- [ ] **Step 5: Run existing tests**

Run: `cd crates/oneshim-web/frontend && pnpm test`
Expected: All existing tests pass (no regressions).

- [ ] **Step 6: Commit**

```bash
git add src/components/ui/Checkbox.tsx src/components/ui/Checkbox.stories.tsx src/components/ui/index.ts
git commit -m "feat(frontend): add Checkbox UI primitive with label + description"
```

---

## Phase B: Story Quality Upgrade

### Task 5: Add autodocs to all existing stories

**Files:**
- Modify: All 76 existing `*.stories.tsx` files

- [ ] **Step 1: Add `tags: ['autodocs']` to every story meta**

For each of the 76 story files, add `tags: ['autodocs']` to the meta object. The pattern is:

```tsx
// BEFORE
const meta = {
  title: 'Category/Component',
  component: Component,
} satisfies Meta<typeof Component>

// AFTER
const meta = {
  title: 'Category/Component',
  component: Component,
  tags: ['autodocs'],
} satisfies Meta<typeof Component>
```

Apply to all files matching `src/**/*.stories.tsx` (excluding the 4 new stories from Phase A which already have it).

Use a search-and-replace approach: in each file, find the `satisfies Meta` line and insert `tags: ['autodocs'],` before the closing brace if not already present.

- [ ] **Step 2: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 3: Commit**

```bash
git add -A src/**/*.stories.tsx
git commit -m "feat(frontend): add autodocs tag to all 76 existing stories"
```

---

### Task 6: Create mock data factory

**Files:**
- Create: `src/stories/mock-data.ts`

- [ ] **Step 1: Create mock data factory**

```ts
// src/stories/mock-data.ts
import type { AppUsage, DailySummary, HourlyMetrics, ProcessEntry } from '../api/contracts'

export function createMockSummary(overrides?: Partial<DailySummary>): DailySummary {
  return {
    date: '2026-03-28',
    total_active_secs: 25200,
    total_idle_secs: 7200,
    top_apps: [
      { name: 'VS Code', duration_secs: 14400, event_count: 120, frame_count: 45 },
      { name: 'Chrome', duration_secs: 7200, event_count: 85, frame_count: 30 },
      { name: 'Terminal', duration_secs: 3600, event_count: 40, frame_count: 15 },
    ],
    cpu_avg: 32.5,
    memory_avg_percent: 68.2,
    frames_captured: 90,
    events_logged: 245,
    ...overrides,
  }
}

export function createMockHourlyMetrics(count = 24): HourlyMetrics[] {
  return Array.from({ length: count }, (_, i) => ({
    hour: `${String(i).padStart(2, '0')}:00`,
    cpu_avg: 20 + Math.random() * 40,
    cpu_max: 40 + Math.random() * 50,
    memory_avg: 50 + Math.random() * 30,
    memory_max: 60 + Math.random() * 35,
    sample_count: 60,
  }))
}

export function createMockProcesses(count = 5): ProcessEntry[] {
  const apps = ['VS Code', 'Chrome', 'Slack', 'Terminal', 'Figma', 'Spotify', 'Docker']
  return Array.from({ length: count }, (_, i) => ({
    pid: 1000 + i,
    name: apps[i % apps.length],
    cpu_usage: Math.random() * 15,
    memory_bytes: (100 + Math.random() * 500) * 1024 * 1024,
  }))
}

export function createMockAppUsage(count = 5): AppUsage[] {
  const apps = ['VS Code', 'Chrome', 'Terminal', 'Slack', 'Figma']
  return Array.from({ length: count }, (_, i) => ({
    name: apps[i % apps.length],
    duration_secs: Math.floor(Math.random() * 14400) + 1800,
    event_count: Math.floor(Math.random() * 100) + 10,
    frame_count: Math.floor(Math.random() * 50) + 5,
  }))
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds (mock-data.ts compiles without errors).

- [ ] **Step 3: Commit**

```bash
git add src/stories/mock-data.ts
git commit -m "feat(frontend): add mock data factory for page stories"
```

---

### Task 7: Enhance thin page stories

**Files:**
- Modify: `src/pages/Dashboard.stories.tsx`
- Modify: `src/pages/DashboardDay.stories.tsx`
- Modify: `src/pages/Timeline.stories.tsx`
- Modify: `src/pages/Reports.stories.tsx`
- Modify: `src/pages/Chat.stories.tsx`
- Modify: `src/pages/Focus.stories.tsx`
- Modify: `src/pages/Coaching.stories.tsx`
- Modify: `src/pages/Settings.stories.tsx`
- Modify: `src/pages/SessionReplay.stories.tsx`

- [ ] **Step 1: Enhance Dashboard story**

Replace `src/pages/Dashboard.stories.tsx` with:

```tsx
import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createMockHourlyMetrics, createMockProcesses, createMockSummary } from '../stories/mock-data'
import Dashboard from './Dashboard'

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/Dashboard',
  component: Dashboard,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <MemoryRouter>
        <Story />
      </MemoryRouter>
    ),
  ],
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof Dashboard>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      const date = new Date().toISOString().split('T')[0]
      qc.setQueryData(['summary', date], createMockSummary())
      qc.setQueryData(['hourlyMetrics'], createMockHourlyMetrics())
      qc.setQueryData(['processes'], createMockProcesses())
      return (
        <QueryClientProvider client={qc}>
          <Story />
        </QueryClientProvider>
      )
    },
  ],
}

export const EmptyState: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      const date = new Date().toISOString().split('T')[0]
      qc.setQueryData(['summary', date], createMockSummary({
        total_active_secs: 0,
        total_idle_secs: 0,
        top_apps: [],
        frames_captured: 0,
        events_logged: 0,
      }))
      qc.setQueryData(['hourlyMetrics'], [])
      qc.setQueryData(['processes'], [])
      return (
        <QueryClientProvider client={qc}>
          <Story />
        </QueryClientProvider>
      )
    },
  ],
}
```

- [ ] **Step 2: Enhance remaining page stories**

Apply the same pattern to each remaining page story file. For each page:
1. Add `tags: ['autodocs']` (if not done in Task 5)
2. Add `parameters: { layout: 'fullscreen' }`
3. Keep existing `Default` story
4. Add `WithMockData` story using `createStoryQueryClient()` + `setQueryData()`
5. Add `EmptyState` story with empty/zero-value mock data

Each page uses different query keys — read each page component's `useQuery` calls to determine the correct keys. For pages that don't use `useQuery` (like Settings tabs), provide inline mock props instead.

**Per-page query keys** (from reading page source):
- `Dashboard`: `['summary', date]`, `['hourlyMetrics']`, `['processes']`
- `DashboardDay`: similar to Dashboard
- `Timeline`: read Timeline.tsx for keys
- `Reports`: read Reports.tsx for keys
- `Chat`: likely uses custom hooks — check Chat.tsx
- `Focus`: check Focus.tsx for useQuery
- `Coaching`: check Coaching.tsx for useQuery
- `SessionReplay`: check SessionReplay.tsx for useQuery
- `Settings`: no useQuery — just wraps tabs

For pages using hooks like `useSSE()` that don't go through QueryClient, the Default story (with no mock data) is acceptable — these pages will show their loading/empty states naturally.

- [ ] **Step 3: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 4: Commit**

```bash
git add src/pages/*.stories.tsx
git commit -m "feat(frontend): enhance page stories with mock data + empty states"
```

---

### Task 8: Enhance thin feature component stories

**Files:**
- Modify: `src/components/StatCard.stories.tsx`
- Modify: `src/components/InsightCard.stories.tsx`
- Modify: `src/components/EventLog.stories.tsx`
- Modify: `src/components/ProcessList.stories.tsx`
- Modify: `src/components/GuiInteractionTrack.stories.tsx`

- [ ] **Step 1: Read each component to understand its props**

Read each of the 5 component files to determine their prop interfaces and possible states. Then write 2-3 stories for each showing:
- Default (with representative data)
- Empty (no data)
- Variant states (if the component has variants)

Each story should pass realistic mock data inline as props — no QueryClient needed since these are presentational components.

- [ ] **Step 2: Add stories to each file**

For each component, add stories following the existing pattern (co-located `.stories.tsx`, same story structure as Button.stories.tsx). Ensure each story has `tags: ['autodocs']`.

- [ ] **Step 3: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 4: Commit**

```bash
git add src/components/*.stories.tsx
git commit -m "feat(frontend): enhance feature component stories with data + empty states"
```

---

### Task 9: Enhance setting tab stories

**Files:**
- Modify: All 10 `src/pages/setting-tabs/*.stories.tsx` files

- [ ] **Step 1: Read setting tab components to understand their props**

Read each tab component to determine what props/context they need. Most receive config data as props or via context.

- [ ] **Step 2: Add WithDefaults story to each tab**

For each setting tab, add a story that passes representative default config data. Use inline mock data objects rather than QueryClient.

- [ ] **Step 3: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 4: Commit**

```bash
git add src/pages/setting-tabs/*.stories.tsx
git commit -m "feat(frontend): enhance setting tab stories with mock config data"
```

---

## Phase C: Storybook Documentation

### Task 10: Getting Started MDX page

**Files:**
- Create: `src/stories/GettingStarted.mdx`

- [ ] **Step 1: Create GettingStarted.mdx**

```mdx
{/* src/stories/GettingStarted.mdx */}
import { Meta } from '@storybook/addon-docs/blocks'

<Meta title="Docs/Getting Started" />

# ONESHIM Design System

Welcome to the ONESHIM component library. This Storybook documents all UI components used in the desktop dashboard application.

## Quick Start

```bash
# Install dependencies
pnpm install

# Run Storybook
pnpm storybook
# → http://localhost:6006

# Build static Storybook
pnpm build-storybook
```

## Component Categories

| Category | Path | Description |
|----------|------|-------------|
| **UI Primitives** | `src/components/ui/` | Base components (Button, Input, Card, etc.) — reusable, no business logic |
| **Shell** | `src/components/shell/` | App layout infrastructure (TitleBar, ActivityBar, SidePanel, StatusBar) |
| **Domain Components** | `src/components/` | Feature-specific shared components (charts, widgets, panels) |
| **Pages** | `src/pages/` | Route-level page components |
| **Overlay** | `src/overlay/components/` | Detection overlay window components |

## Adding a New Component

1. Create `src/components/ui/MyComponent.tsx` with `forwardRef` + `cn()` pattern
2. Add variants to `src/styles/variants.ts` (if the component has visual variants)
3. Export from `src/components/ui/index.ts`
4. Create co-located `MyComponent.stories.tsx` with `tags: ['autodocs']`
5. Run `pnpm lint` and `pnpm build-storybook` to verify

## Key Files

| File | Purpose |
|------|---------|
| `src/styles/tokens.ts` | All design tokens (colors, typography, spacing, motion) |
| `src/styles/variants.ts` | Component variant class strings |
| `src/index.css` | CSS custom properties (light/dark theme values) |
| `src/utils/cn.ts` | Class merging utility (clsx + tailwind-merge) |
| `tailwind.config.js` | Tailwind theme extensions |
| `scripts/lint-design-tokens.sh` | CI lint gate — blocks hardcoded colors, spacing, fonts |

## Documentation

- [DESIGN.md](https://github.com/pseudotop/oneshim-client/blob/main/crates/oneshim-web/frontend/DESIGN.md) — Design principles and contribution rules
- [TOKENS.md](https://github.com/pseudotop/oneshim-client/blob/main/crates/oneshim-web/frontend/TOKENS.md) — Visual token reference (all light/dark values)
```

- [ ] **Step 2: Verify MDX renders in Storybook**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully" — MDX page appears under "Docs/Getting Started".

- [ ] **Step 3: Commit**

```bash
git add src/stories/GettingStarted.mdx
git commit -m "docs(frontend): add Getting Started MDX page to Storybook"
```

---

### Task 11: Component Patterns MDX page

**Files:**
- Create: `src/stories/ComponentPatterns.mdx`

- [ ] **Step 1: Create ComponentPatterns.mdx**

```mdx
{/* src/stories/ComponentPatterns.mdx */}
import { Meta } from '@storybook/addon-docs/blocks'

<Meta title="Docs/Component Patterns" />

# Component Patterns

Architecture patterns used by all components in this design system.

## Primitive Pattern

Every UI primitive follows this structure:

```tsx
import { forwardRef } from 'react'
import { interaction, radius } from '../../styles/tokens'
import { myVariants } from '../../styles/variants'
import { cn } from '../../utils/cn'

export interface MyComponentProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: keyof typeof myVariants.variant
  size?: keyof typeof myVariants.size
}

export const MyComponent = forwardRef<HTMLDivElement, MyComponentProps>(
  ({ className, variant = 'default', size = 'md', ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        radius.md,
        interaction.interactive,
        myVariants.variant[variant],
        myVariants.size[size],
        className,   // caller's className always last
      )}
      {...props}
    />
  ),
)
MyComponent.displayName = 'MyComponent'
```

**Rules:**
- `forwardRef` on all primitives — enables ref forwarding for composition
- Props extend native HTML attributes — full DOM compatibility
- `cn()` composes all classes — tokens, variants, then caller's `className` last (override wins)
- `displayName` always set — required for React DevTools and Storybook

## Class Merging (cn)

`cn()` wraps `clsx` + `tailwind-merge`:

```tsx
import { cn } from '../../utils/cn'

cn('p-4', 'p-6')          // → 'p-6' (tailwind-merge resolves conflict)
cn('p-4', isActive && 'bg-hover')  // → 'p-4 bg-hover' (conditional)
cn('p-4', className)       // → caller's className overrides p-4 if conflicting
```

## Variant Pattern

Variants live in `src/styles/variants.ts`, not inline in components:

```ts
export const buttonVariants = {
  variant: {
    primary: 'bg-brand hover:bg-brand-hover text-content-inverse ...',
    secondary: 'bg-surface-muted hover:bg-active text-content ...',
    ghost: 'hover:bg-hover text-content-secondary',
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
    lg: 'px-6 py-3 text-base',
  },
} as const
```

**Rules:**
- Flat objects only — `variants.variant[key]`, never `variants.a.b.c`
- Use `as const` for strict TypeScript inference
- Import `colors` from `tokens.ts` for values, not hardcoded classes

## Dark Mode

Theme switching is CSS-only. The `.dark` class on `<html>` swaps CSS custom property values.

```tsx
// WRONG — never use dark: prefix
<div className="bg-white dark:bg-slate-900">

// RIGHT — automatically adapts via CSS vars
<div className="bg-surface-base">
```

## Architecture Rules

| Rule | Reason |
|------|--------|
| **No React.createContext** | Use hooks + callback props instead |
| **No React.createPortal** | Use fixed positioning + z-index |
| **No new npm dependencies** | All UI built with native browser APIs |
| **No `dark:` prefix** | Theming via CSS custom properties |
| **Token-only styling** | `lint-design-tokens.sh` blocks hardcoded values in CI |

## Composition Example

Combining primitives into a feature component:

```tsx
import { Badge, Button, Card, CardContent, CardHeader, CardTitle } from './ui'
import { Divider } from './ui'

function FeatureCard({ title, status, onAction }) {
  return (
    <Card variant="default" padding="md">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>{title}</CardTitle>
          <Badge color={status === 'active' ? 'success' : 'default'}>{status}</Badge>
        </div>
      </CardHeader>
      <Divider />
      <CardContent>
        <Button variant="primary" onClick={onAction}>Take Action</Button>
      </CardContent>
    </Card>
  )
}
```
```

- [ ] **Step 2: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 3: Commit**

```bash
git add src/stories/ComponentPatterns.mdx
git commit -m "docs(frontend): add Component Patterns MDX page to Storybook"
```

---

### Task 12: Enhance DesignTokens story

**Files:**
- Modify: `src/stories/DesignTokens.stories.tsx`

- [ ] **Step 1: Add light/dark comparison and icon catalog**

Enhance the existing `DesignTokens.stories.tsx` by adding:

1. **Light/Dark comparison section** — render each color swatch twice (once forced light, once forced dark) using wrapper divs with `className="light"` and `className="dark"`.

2. **Icon catalog** — import all lucide icons used across the project and render them in a grid with their names and size tokens.

Read the current file first, then add these sections while preserving all existing content. Add the light/dark comparison at the top of the `DesignTokensPage` function, and the icon catalog at the bottom.

- [ ] **Step 2: Verify Storybook build**

Run: `cd crates/oneshim-web/frontend && pnpm build-storybook 2>&1 | tail -5`
Expected: "Storybook build completed successfully"

- [ ] **Step 3: Commit**

```bash
git add src/stories/DesignTokens.stories.tsx
git commit -m "feat(frontend): enhance DesignTokens story with light/dark comparison + icon catalog"
```

---

## Final Verification

### Task 13: Full validation

- [ ] **Step 1: Run all checks**

```bash
cd crates/oneshim-web/frontend
pnpm build
pnpm lint
pnpm test
pnpm build-storybook
```

All 4 commands must pass.

- [ ] **Step 2: Verify component count**

```bash
# Count component files
find src/components -name '*.tsx' -not -name '*.stories.tsx' -not -name '*.test.tsx' | wc -l
# Count story files
find src -name '*.stories.tsx' | wc -l
```

Every component should have a matching story file.

- [ ] **Step 3: Verify new primitives in index.ts**

```bash
grep -c 'export' src/components/ui/index.ts
```

Expected: 18 (was 14, plus 4 new primitives: Divider, Alert, Dialog, Checkbox — Dialog has multiple exports).

- [ ] **Step 4: Commit any remaining fixes**

If any validation failed, fix and commit.

- [ ] **Step 5: Final commit (if all clean)**

```bash
git status
# Should show clean working tree or only docs/plans/ files
```
