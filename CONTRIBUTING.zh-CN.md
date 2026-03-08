# ONESHIM Rust 客户端贡献指南

感谢您对 ONESHIM Rust 客户端的关注。本文档是针对 10-crate Cargo workspace 的 Rust 专属贡献指南。

## 开发环境搭建

### 前置条件

- **Rust** 1.77.1 或更高版本（使用 `rustup update stable` 保持最新）
- **cargo** -- Rust 构建系统和包管理器（随 Rust 一同安装）
- **pnpm** -- 构建前端 Web 仪表盘（`oneshim-web/frontend`）时需要

### 初始化设置

```bash
# 1. 克隆仓库
git clone https://github.com/pseudotop/oneshim-client.git
cd oneshim-client

# 2. 检查依赖并构建
cargo check --workspace

# 3. 构建前端（如需包含 Web 仪表盘）
cd crates/oneshim-web/frontend
pnpm install
pnpm build
cd ../../..

# 4. 完整构建
cargo build --workspace
```

### 可选功能

部分功能通过 feature flag 控制。

```bash
# 启用 OCR（需要 Tesseract）
cargo build -p oneshim-vision --features ocr

# 启用 gRPC 客户端（tonic/prost）
cargo build -p oneshim-network --features grpc
```

## 构建

### 开发构建

```bash
# 快速验证整个 workspace
cargo check --workspace

# 开发构建
cargo build -p oneshim-app

# 以开发模式运行
cargo run -p oneshim-app
```

### 包含前端的构建

Web 仪表盘会将 React 构建产物嵌入 Rust 二进制文件中。

```bash
# 第 1 步：构建前端
cd crates/oneshim-web/frontend && pnpm install && pnpm build
# 或使用脚本
./scripts/build-frontend.sh

# 第 2 步：构建 Rust 二进制文件（自动嵌入 dist/）
cargo build --release -p oneshim-app
```

### 完整 Workspace 构建

```bash
# 所有 crate 的发布构建
cargo build --release --workspace
```

### 构建特定 Crate

```bash
cargo build -p oneshim-core
cargo build -p oneshim-network
cargo build -p oneshim-vision
```

## 代码风格

### 格式化

所有代码遵循 `cargo fmt` 默认设置。提交 PR 前请先运行格式化。

```bash
# 应用格式化
cargo fmt --all

# 检查格式（与 CI 一致）
cargo fmt --check
```

### Lint

`cargo clippy` 必须报告零警告。如需抑制某个警告，请在相应条目上添加 `#[allow(...)]` 并在注释中说明原因。

```bash
# 对整个 workspace 运行 clippy
cargo clippy --workspace

# 启用所有 feature 运行
cargo clippy --workspace --all-features
```

### 注释与文档

- **代码注释/文档注释默认使用英文。**
- **公开文档以英文为主，关键指南提供韩文配套文档。**
- 为所有 `pub` 项添加 `///` 文档注释。
- 在复杂逻辑中使用行内注释（`//`）说明意图。
- 文档治理规范请参阅 [docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md)。
- 韩文配套策略: [docs/DOCUMENTATION_POLICY.ko.md](./docs/DOCUMENTATION_POLICY.ko.md)
- 可变质量指标请仅更新 [docs/STATUS.md](./docs/STATUS.md)。

```rust
/// Screen capture trigger — decides whether to capture based on event importance.
pub struct SmartCaptureTrigger {
    // Timestamp of last capture — used for throttling
    last_capture: Instant,
}
```

### 错误处理

- 库 crate：使用 `thiserror` 定义具体的错误枚举
- 二进制 crate（`oneshim-app`）：使用 `anyhow::Result`
- 使用 `#[from]` 包装外部 crate 错误

```rust
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// No auth token available
    #[error("no auth token")]
    NoToken,
}
```

### 异步 Trait

对所有 Port trait 应用 `#[async_trait]`。这是 `Arc<dyn PortTrait>` 依赖注入模式所必需的。

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ApiClient: Send + Sync {
    /// Uploads a context payload to the server.
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

## 架构规则

本项目严格遵循 **Hexagonal Architecture（Ports & Adapters）**。请在贡献代码前了解以下规则。

### 核心原则

**`oneshim-core` 定义所有 Port trait 和领域模型。** 其余 9 个 crate 均为 adapter。

```
oneshim-core  (Port 定义、模型)
    <- oneshim-monitor   (系统监控 adapter)
    <- oneshim-vision    (图像处理 adapter)
    <- oneshim-network   (HTTP/SSE/WebSocket adapter)
    <- oneshim-storage   (SQLite adapter)
    <- oneshim-suggestion <- oneshim-network
    <- src-tauri          <- oneshim-suggestion
    <- oneshim-automation
    <- oneshim-app        (完整依赖注入布线)
```

### 禁止的模式

不允许 adapter crate 之间直接依赖。例如，`oneshim-monitor` 不得直接依赖 `oneshim-storage`。所有跨 crate 通信必须通过 `oneshim-core` 中定义的 trait 进行。

允许的例外：
- `oneshim-suggestion` -> `oneshim-network`（SSE 接收）
- `src-tauri` -> `oneshim-suggestion`（建议展示）

### 依赖注入模式

使用 `Arc<dyn T>` 进行构造函数注入。不使用任何 DI 框架；所有布线在 `oneshim-app/src/main.rs` 中手动完成。

```rust
pub struct Scheduler {
    // Dependencies injected via Arc<dyn T> pattern
    monitor: Arc<dyn SystemMonitor>,
    storage: Arc<dyn StorageService>,
    api_client: Arc<dyn ApiClient>,
}

impl Scheduler {
    pub fn new(
        monitor: Arc<dyn SystemMonitor>,
        storage: Arc<dyn StorageService>,
        api_client: Arc<dyn ApiClient>,
    ) -> Self {
        Self { monitor, storage, api_client }
    }
}
```

## 添加新功能

添加新功能时请按以下顺序进行。

### 第 1 步：在 core 中定义 Port

在 `crates/oneshim-core/src/ports/` 下添加新的 trait。

```rust
// crates/oneshim-core/src/ports/my_service.rs

use async_trait::async_trait;
use crate::error::CoreError;

/// Port interface for the new feature
#[async_trait]
pub trait MyService: Send + Sync {
    /// Performs the operation.
    async fn do_something(&self, input: &str) -> Result<String, CoreError>;
}
```

### 第 2 步：实现 Adapter

在对应的 adapter crate 中实现 trait。

```rust
// crates/oneshim-xxx/src/my_impl.rs

use async_trait::async_trait;
use oneshim_core::{ports::MyService, error::CoreError};

pub struct MyServiceImpl {
    // Fields needed for the implementation
}

#[async_trait]
impl MyService for MyServiceImpl {
    async fn do_something(&self, input: &str) -> Result<String, CoreError> {
        // Actual implementation
        todo!()
    }
}
```

### 第 3 步：在 app 中完成依赖注入布线

在 `crates/oneshim-app/src/main.rs` 中将实现连接到对应的 Port。

```rust
// crates/oneshim-app/src/main.rs

let my_service: Arc<dyn MyService> = Arc::new(MyServiceImpl::new());
let scheduler = Scheduler::new(my_service, /* other dependencies */);
```

### 第 4 步：编写测试

编写单元测试和集成测试。

```rust
// Unit tests: place at the bottom of the relevant module
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_do_something() {
        let svc = MyServiceImpl::new();
        let result = svc.do_something("input").await;
        assert!(result.is_ok());
    }
}
```

## 编写测试

### 原则

- **不使用 mockall。** 手动编写 mock。
- 在每个模块底部的 `#[cfg(test)] mod tests` 块中编写测试。
- 直接实现 Port trait 来创建测试 mock。

### 手动 Mock 模式

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::ports::ApiClient;

    // Test mock — only defined inside the #[cfg(test)] block
    struct MockApiClient {
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl ApiClient for MockApiClient {
        async fn upload_context(
            &self,
            _context: &ContextPayload,
        ) -> Result<(), CoreError> {
            if self.should_fail {
                Err(CoreError::Network("test failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn upload_success_saves_event() {
        let client = Arc::new(MockApiClient { should_fail: false });
        // ... test logic
    }

    #[tokio::test]
    async fn upload_failure_triggers_retry() {
        let client = Arc::new(MockApiClient { should_fail: true });
        // ... test logic
    }
}
```

### 运行测试

```bash
# 完整测试套件
cargo test --workspace

# 特定 crate
cargo test -p oneshim-core
cargo test -p oneshim-vision
cargo test -p oneshim-network

# 单个测试
cargo test -p oneshim-storage -- sqlite::tests::migration_v7

# 集成测试
cargo test -p oneshim-app
```

### E2E 测试（Web 仪表盘）

```bash
cd crates/oneshim-web/frontend
pnpm test:e2e          # 完整 E2E 测试套件
pnpm test:e2e:headed   # 显示浏览器窗口
pnpm test:e2e:ui       # Playwright UI 模式
```

## PR 流程

### 分支策略

```bash
# 新功能分支
git checkout -b feat/vision-pii-filter-improvement

# Bug 修复分支
git checkout -b fix/network-sse-reconnect

# 文档分支
git checkout -b docs/scheduler-architecture
```

### PR 提交前检查清单

提交 PR 前请确认以下所有项目。

```bash
# 1. 格式检查
cargo fmt --check

# 2. Clippy 警告数：0
cargo clippy --workspace

# 3. 所有测试通过
cargo test --workspace

# 4. 构建成功
cargo build --workspace
```

### 编写 PR 描述

PR 描述中请包含以下内容：

- 变更的动机和背景
- 实现方案概述
- 如何测试该变更
- 确认遵守架构规则（特别是跨 crate 依赖关系）

### 代码审查

审查者重点关注：

- Hexagonal Architecture 合规性（Port/Adapter 分离）
- adapter crate 之间无直接依赖
- `cargo clippy` 警告数：0
- 仅使用手动 mock（不使用 mockall）
- 英文注释

## 提交信息规范

遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范。

### 格式

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### 类型

| 类型 | 说明 |
|------|------|
| `feat` | 新功能 |
| `fix` | Bug 修复 |
| `perf` | 性能优化 |
| `refactor` | 重构（不改变行为） |
| `test` | 添加或更新测试 |
| `docs` | 文档变更 |
| `chore` | 构建、CI 或依赖变更 |

### 作用域

使用 crate 名称或功能领域作为作用域。

`core`, `network`, `suggestion`, `storage`, `monitor`, `vision`, `tauri`, `web`, `automation`, `app`

### 示例

```
feat(vision): add credit card number masking to PII filter

Masks 16-digit number patterns at Standard level and above.
Integrated with the existing CWE-359 compliance logic.
```

```
fix(network): cap SSE reconnect exponential backoff at 30 seconds

Prevents the retry delay from growing unbounded on repeated failures.
```

```
perf(storage): eliminate N+1 query in end_work_session with RETURNING

Merges the SELECT + UPDATE into a single RETURNING clause query.
Benchmark: 50% throughput improvement confirmed.
```

## 报告问题

### Bug 报告

请使用 GitHub Issues 中的 **Bug Report** 模板，并包含以下信息：

1. **Bug 描述**: 清楚说明出了什么问题
2. **复现步骤**: 逐步的复现流程
3. **预期行为**: 应该发生什么
4. **实际行为**: 实际发生了什么
5. **环境**: 操作系统、Rust 版本（`rustc --version`）、相关依赖版本
6. **日志**: `RUST_LOG=debug cargo run -p oneshim-app` 的相关输出

### 功能请求

提出功能建议时，请从 Hexagonal Architecture 的角度进行说明：

- 是否需要新的 Port，还是可以扩展现有 Port
- Adapter 应放在哪个 crate 中
- 对现有跨 crate 依赖关系的影响

## 许可证

参与本项目即表示您同意您的贡献按 [Apache License 2.0](LICENSE) 进行授权。

---

如有疑问，请使用 GitHub Issues 或 Discussions。
