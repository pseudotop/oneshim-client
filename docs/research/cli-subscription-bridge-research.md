# CLI Subscription Bridge Research (2026-02-23)

## Goal
Define a provider-neutral bridge for `AiAccessMode::ProviderSubscriptionCli` so ONESHIM context can be reused from external AI CLIs without backend dependency.

## Sources
- OpenAI Codex docs (custom commands): [developers.openai.com/codex/cli/commands](https://developers.openai.com/codex/cli/commands)
- Anthropic Claude Code docs (slash commands): [docs.anthropic.com/en/docs/claude-code/slash-commands](https://docs.anthropic.com/en/docs/claude-code/slash-commands)
- Google Gemini CLI docs (custom commands): [raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/custom-commands.md](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/custom-commands.md)
- Gemini CLI context file docs (`GEMINI.md`): [raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/gemini-md.md](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/gemini-md.md)

## Findings
1. Codex CLI supports project/user custom commands under `.codex/commands` and `~/.codex/commands`.
2. Claude Code supports project/user slash commands under `.claude/commands` and `~/.claude/commands`.
3. Gemini CLI supports project/user custom commands under `.gemini/commands` and `~/.gemini/commands`, plus context layering via `GEMINI.md`.

## ONESHIM Bridge Decision
Generate bridge artifacts using each CLI-native format:

| CLI | Project Scope | User Scope | Artifact |
| --- | --- | --- | --- |
| Codex | `.codex/commands/oneshim-context.md` | `~/.codex/commands/oneshim-context.md` | Markdown command |
| Claude Code | `.claude/commands/oneshim-context.md` | `~/.claude/commands/oneshim-context.md` | Markdown command |
| Gemini CLI | `.gemini/commands/oneshim-context.toml` | `~/.gemini/commands/oneshim-context.toml` | TOML command |

Shared ONESHIM context export reference:
- Default path: `<data_dir>/exports/oneshim-context.json`

## Runtime Policy
- Bridge sync runs only when `ai_provider.access_mode == ProviderSubscriptionCli`.
- Auto-install is opt-in via `ONESHIM_CLI_BRIDGE_AUTOINSTALL=1`.
- User-scope bridge generation is opt-in via `ONESHIM_CLI_BRIDGE_INCLUDE_USER_SCOPE=1`.

This keeps standalone behavior deterministic while enabling external CLI interoperability when explicitly requested.
