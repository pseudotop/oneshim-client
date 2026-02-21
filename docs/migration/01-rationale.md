[English](./01-rationale.md) | [한국어](./01-rationale.ko.md)

# 1. Migration Rationale

[← README](./README.md) | [Project Structure →](./02-project-structure.md)

---

## Why Rust Native

| Aspect | Python (current) | Rust (target) |
|--------|-----------------|---------------|
| **Deploy size** | ~100MB+ (includes Python runtime) | ~15-20MB (single binary) |
| **Installation** | Python install → venv → pip install → run | .dmg / .exe double-click |
| **Startup time** | 2-5s (interpreter loading) | <100ms |
| **Memory usage** | ~80-150MB (GC overhead) | ~20-40MB |
| **System access** | psutil (wrapper) + pyobjc/pywin32 | Direct system calls |
| **Concurrency** | asyncio + threading (GIL) | tokio (true multithreading) |
| **Stability** | Runtime type errors possible | Compile-time guarantees |
| **Security** | Source exposed, memory vulnerable | Binary, memory safe |

## Why Full Rust Instead of Sidecar

- Sidecar: Python UI + Rust monitoring → burden of maintaining two runtimes
- Full Rust: single binary, single language, single build pipeline
- Since AI generates code, Rust UI development difficulty is not an issue
