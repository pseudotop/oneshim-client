# CLI 구독 브리지 리서치 (2026-02-23)

## 목표
백엔드 없이도 `AiAccessMode::ProviderSubscriptionCli`에서 ONESHIM 컨텍스트를 외부 AI CLI가 재사용할 수 있도록, 제공자 중립 브리지 규격을 정의한다.

## 참고 자료
- OpenAI Codex 문서 (custom commands): [developers.openai.com/codex/cli/commands](https://developers.openai.com/codex/cli/commands)
- Anthropic Claude Code 문서 (slash commands): [docs.anthropic.com/en/docs/claude-code/slash-commands](https://docs.anthropic.com/en/docs/claude-code/slash-commands)
- Google Gemini CLI 문서 (custom commands): [raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/custom-commands.md](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/custom-commands.md)
- Gemini CLI 컨텍스트 파일 문서 (`GEMINI.md`): [raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/gemini-md.md](https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/gemini-md.md)

## 리서치 결과
1. Codex CLI는 프로젝트/사용자 커맨드를 `.codex/commands`, `~/.codex/commands`에서 로드한다.
2. Claude Code는 프로젝트/사용자 slash command를 `.claude/commands`, `~/.claude/commands`에서 로드한다.
3. Gemini CLI는 프로젝트/사용자 커맨드를 `.gemini/commands`, `~/.gemini/commands`에서 로드하며 `GEMINI.md` 계층도 지원한다.

## ONESHIM 브리지 결정
CLI별 네이티브 포맷으로 브리지 아티팩트를 생성한다.

| CLI | 프로젝트 스코프 | 사용자 스코프 | 아티팩트 |
| --- | --- | --- | --- |
| Codex | `.codex/commands/oneshim-context.md` | `~/.codex/commands/oneshim-context.md` | Markdown command |
| Claude Code | `.claude/commands/oneshim-context.md` | `~/.claude/commands/oneshim-context.md` | Markdown command |
| Gemini CLI | `.gemini/commands/oneshim-context.toml` | `~/.gemini/commands/oneshim-context.toml` | TOML command |

공통 ONESHIM 컨텍스트 경로:
- 기본값: `<data_dir>/exports/oneshim-context.json`

## 런타임 정책
- 브리지 동기화는 `ai_provider.access_mode == ProviderSubscriptionCli`일 때만 수행한다.
- 자동 설치는 `ONESHIM_CLI_BRIDGE_AUTOINSTALL=1`일 때만 활성화한다.
- 사용자 스코프 생성은 `ONESHIM_CLI_BRIDGE_INCLUDE_USER_SCOPE=1`일 때만 활성화한다.

이 정책으로 standalone 동작의 예측 가능성을 유지하면서, 명시적으로 원할 때 외부 CLI 연계를 활성화할 수 있다.
