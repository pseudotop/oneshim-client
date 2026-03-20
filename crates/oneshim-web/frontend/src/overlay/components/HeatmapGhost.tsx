/**
 * HeatmapGhost — semi-transparent attention heatmap overlay showing
 * click/interaction density as colored regions on screen.
 *
 * Status: Phase 3 placeholder (renders null).
 *
 * Prerequisites for implementation:
 * 1. Monitor loop: aggregate mouse position into a per-pixel heat counter
 *    (ring buffer, 5-minute sliding window, 50x50 grid buckets).
 *    Data source: InputActivityCollector already tracks mouse position;
 *    aggregation into grid buckets is the missing piece.
 * 2. Tauri event: `coaching://heatmap-update` with grid data (JSON array).
 * 3. Canvas renderer: draw semi-transparent colored rectangles per bucket,
 *    mapping interaction count to a warm-to-hot color gradient.
 * 4. Performance: must stay under 5ms render time per update to avoid
 *    blocking the overlay's animation frame budget.
 *
 * The GUI heatmap data pipeline already exists (monitor crate collects
 * mouse/keyboard input patterns). Canvas rendering will be added in
 * Phase 3 once real-time position data aggregation is wired through
 * the Tauri event bridge.
 */
export default function HeatmapGhost() {
  return null
}
