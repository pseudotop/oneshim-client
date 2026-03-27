# Chat Page Improvements

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `crates/oneshim-web/frontend/src/pages/Chat.tsx`

## Problem

The Chat page is functional with markdown rendering, syntax highlighting, and streaming. Three capabilities are missing:

1. **Message search**: No way to find messages in long conversations
2. **File attachments**: Backend `Attachment` model supports Image/File/Directory/Skill/AppReference, but frontend has no upload UI
3. **Tool use rendering**: Tool invocations show only `tool_name + status badge` — no expandable details (input params, result output)

## Design

All three features are **frontend-only** changes to `Chat.tsx`. No Rust backend changes needed.

### Feature 1: Message Search

A search bar that filters and highlights messages in the current conversation.

**UI:**
- Search icon button in the message area header (next to session state indicator)
- Click → expands inline search input with close button
- As user types, messages not matching are dimmed (opacity 0.3), matching messages stay full opacity
- Matched text within messages gets a yellow highlight `<mark>` wrapper
- Match count shown: "3 of 12 matches"
- Escape or close button → clear search, restore all messages

**Implementation:**
- New state: `searchQuery: string`, `searchOpen: boolean`
- Filter logic: case-insensitive `content.includes(query)` on each message
- Highlight: wrap matched substrings in `<mark className="bg-yellow-300/40 rounded">` within the Bubble component
- Performance: debounce search input at 150ms for large conversations

### Feature 2: File Attachments

Allow users to attach files/images to messages.

**UI:**
- Paperclip button (📎) left of the textarea input
- Click → native file dialog via Tauri `open()` API
- Selected files shown as chips below the input: `[📄 file.txt ✕] [🖼 screenshot.png ✕]`
- Images show thumbnail preview (48x48)
- Send button includes attachments in the message payload

**Implementation:**
- New state: `attachments: { name: string; type: string; data: string }[]`
- File selection: Hidden `<input type="file" multiple>` triggered by paperclip button (no Tauri plugin dependency)
- Read file content: `FileReader.readAsDataURL()` → extract base64
- Attachment chip component: filename + type icon + remove button
- Image attachments: read as base64, show `<img>` thumbnail
- On send: include `attachments` array alongside `message` text

**Backend consideration:** The `send_session_message` IPC command currently takes `(session_id, message)`. Attachments would need to be serialized into the message payload or sent separately. Since the backend `Attachment` enum already exists with `data: Option<String>` (base64), we encode file content as base64 and prepend attachment metadata to the message as a structured header that the session manager can parse.

**Simplified approach for v1:** Encode attachments as markdown in the message text:
- Image: `![filename](data:image/png;base64,{data})`
- File: `[📎 filename.ext]\n\`\`\`\n{file_content}\n\`\`\``

This works immediately with no backend changes. A proper attachment API can be added later.

### Feature 3: Interactive Tool Use Rendering

Transform the flat tool use status line into expandable cards.

**Current:**
```
🔧 search_files  [completed]
```

**Proposed:**
```
┌─────────────────────────────────────┐
│ 🔧 search_files         [completed]│
│ ─────────────────────────────────── │
│ ▸ Input                             │  ← click to expand
│ ▾ Result                            │  ← expanded
│   Found 3 matches in src/lib.rs     │
└─────────────────────────────────────┘
```

**Implementation:**
- Replace the flat `<div>` in Bubble with a `ToolUseCard` component
- Collapsible sections for `input` (JSON, syntax-highlighted) and `result` (plain text or markdown)
- Status badge with color coding (green/red/gray)
- Default: input collapsed, result expanded (users care about results)
- During `started` status: show a subtle spinner, no input/result yet

**Data flow:** The `ChatMessage.tool_use` interface needs extension:

```typescript
interface ChatMessage {
  // ... existing fields
  tool_use?: {
    tool: string
    status: string
    input?: Record<string, unknown>  // NEW — tool input parameters
    result?: string                   // NEW — tool output
  }
}
```

The backend `OutboundMessage::ToolUse` already sends `input: Option<Value>` and `result: Option<String>`. The frontend event handler just needs to capture these fields.

### Files Changed

| File | Change Type | Description |
|------|------------|-------------|
| `crates/oneshim-web/frontend/src/pages/Chat.tsx` | MODIFY | Add search, attachments, tool use cards |

Single file change — all three features are self-contained in Chat.tsx.

### Testing Strategy

- Manual: type in search box → verify dimming + highlighting
- Manual: attach image file → verify chip display + send
- Manual: send message that triggers tool use → verify expandable card
- Biome lint: `pnpm lint` must pass
- TypeScript: `pnpm tsc --noEmit` must pass
