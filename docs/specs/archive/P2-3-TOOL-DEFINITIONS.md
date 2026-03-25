# P2-3: Pull API Tool Definitions

## Problem

`SessionContextAssembler.build_system_message()` sets `tools: None`. CLI sessions cannot discover or query oneshim-web endpoints for local data (metrics, sessions, events, focus, suggestions).

## Solution

Add a `build_tool_definitions()` method that returns `Vec<ToolDefinition>` for key oneshim-web endpoints, using the configured web port. Populate `tools: Some(defs)` in `build_system_message()`.

## Implementation

**File**: `src-tauri/src/session_context.rs`

Add method:
```rust
fn build_tool_definitions(&self) -> Vec<ToolDefinition> {
    let base = format!("http://localhost:{}/api", self.config.web.port);
    vec![
        ToolDefinition { name: "get_metrics", description: "Query raw activity metrics", endpoint: format!("{base}/metrics") },
        ToolDefinition { name: "get_stats_summary", description: "Get summary statistics (app usage, session counts)", endpoint: format!("{base}/stats/summary") },
        ToolDefinition { name: "get_sessions", description: "List work sessions", endpoint: format!("{base}/sessions") },
        ToolDefinition { name: "get_events", description: "Query recent events", endpoint: format!("{base}/events") },
        ToolDefinition { name: "get_suggestions", description: "List pending suggestions", endpoint: format!("{base}/suggestions") },
        ToolDefinition { name: "get_focus_metrics", description: "Get focus/productivity metrics", endpoint: format!("{base}/focus/metrics") },
        ToolDefinition { name: "search", description: "Full-text search across events", endpoint: format!("{base}/search") },
    ]
}
```

Modify `build_system_message()`:
```rust
tools: Some(self.build_tool_definitions()),
```

Also remove `#[allow(dead_code)]` from the `config` field since it's now used.

## Files Changed

| File | Change |
|------|--------|
| `src-tauri/src/session_context.rs` | Add `build_tool_definitions()`, populate `tools` field, remove dead_code allow |

## Testing

Existing test `build_system_message_serializes_context` verifies the message structure. Add assertion for `tools.is_some()`.
