/**
 * TrackingScheduleSettings — A.20 stub.
 *
 * A.20 will replace this file with the real implementation that:
 * - Renders a list of TrackingWindow entries with HH:MM start/end inputs
 * - Shows an "Add window" button to append a default window
 * - Submits changes via PUT /api/tracking-schedule (React Query mutation)
 * - Displays an "Active now — ends HH:MM" pill when /status returns active_now=true
 * - Validates HH:MM format and shows inline errors
 * - Provides a timezone dropdown with IANA names + "Local" default
 * - Uses trackingSchedule.title ("추적 일정" in ko / "Tracking Schedule" in en)
 */
export function TrackingScheduleSettings() {
  return <div data-testid="tracking-schedule-settings-stub">TrackingScheduleSettings A.20 stub</div>
}
