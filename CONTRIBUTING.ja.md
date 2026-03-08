# ONESHIM Rustクライアント コントリビューションガイド

ONESHIMのRustクライアントに関心をお寄せいただきありがとうございます。本ドキュメントは、10クレイトで構成されるCargo workspaceへのコントリビューションに特化したRust向けガイドです。

## 開発環境のセットアップ

### 前提条件

- **Rust** 1.77.1以降（`rustup update stable`で最新に保ってください）
- **cargo** — Rustのビルドシステム兼パッケージマネージャー（Rustに同梱）
- **pnpm** — フロントエンドWebダッシュボード（`oneshim-web/frontend`）のビルドに必要

### セットアップ

```bash
# 1. リポジトリをクローン
git clone https://github.com/pseudotop/oneshim-client.git
cd oneshim-client

# 2. 依存関係の確認とビルド
cargo check --workspace

# 3. フロントエンドのビルド（Webダッシュボードを含む場合）
cd crates/oneshim-web/frontend
pnpm install
pnpm build
cd ../../..

# 4. フルビルド
cargo build --workspace
```

### オプション機能

一部の機能はfeatureフラグで制御されています。

```bash
# OCRの有効化（Tesseractが必要）
cargo build -p oneshim-vision --features ocr

# gRPCクライアントの有効化（tonic/prost）
cargo build -p oneshim-network --features grpc
```

## ビルド

### 開発ビルド

```bash
# ワークスペースの簡易検証
cargo check --workspace

# 開発ビルド
cargo build -p oneshim-app

# 開発モードで実行
cargo run -p oneshim-app
```

### フロントエンドを含むビルド

Webダッシュボードは、Reactのビルド出力をRustバイナリに埋め込みます。

```bash
# ステップ1: フロントエンドのビルド
cd crates/oneshim-web/frontend && pnpm install && pnpm build
# またはスクリプトを使用
./scripts/build-frontend.sh

# ステップ2: Rustバイナリのビルド（dist/を自動的に埋め込みます）
cargo build --release -p oneshim-app
```

### ワークスペース全体のビルド

```bash
# 全クレイトのリリースビルド
cargo build --release --workspace
```

### 特定クレイトのビルド

```bash
cargo build -p oneshim-core
cargo build -p oneshim-network
cargo build -p oneshim-vision
```

## コードスタイル

### フォーマット

すべてのコードは`cargo fmt`のデフォルト設定に従います。PR提出前に必ず実行してください。

```bash
# フォーマットを適用
cargo fmt --all

# フォーマットのチェック（CIと同一）
cargo fmt --check
```

### リント

`cargo clippy`は警告ゼロでなければなりません。警告を抑制する必要がある場合は、対象項目に`#[allow(...)]`を追加し、コメントで理由を説明してください。

```bash
# ワークスペース全体でclippyを実行
cargo clippy --workspace

# 全featureを有効にして実行
cargo clippy --workspace --all-features
```

### コメントとドキュメント

- **コードコメント/docstringはデフォルトで英語で記述してください。**
- **公開ドキュメントは英語が主であり、主要なガイドには韓国語の付属ドキュメントが用意されています。**
- すべての`pub`項目に`///`ドキュメントコメントを追加してください。
- 複雑なロジックにはインラインコメント（`//`）で意図を説明してください。
- ドキュメントのガバナンスについては[docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md)に従ってください。
- 韓国語のポリシードキュメント: [docs/DOCUMENTATION_POLICY.ko.md](./docs/DOCUMENTATION_POLICY.ko.md)
- 可変品質指標の更新は[docs/STATUS.md](./docs/STATUS.md)のみで行ってください。

```rust
/// Screen capture trigger — decides whether to capture based on event importance.
pub struct SmartCaptureTrigger {
    // Timestamp of last capture — used for throttling
    last_capture: Instant,
}
```

### エラーハンドリング

- ライブラリクレイト: `thiserror`を使用して具体的なエラー列挙型を定義します
- バイナリクレイト（`oneshim-app`）: `anyhow::Result`を使用します
- 外部クレイトのエラーは`#[from]`でラップします

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

### 非同期トレイト

すべてのPortトレイトに`#[async_trait]`を適用してください。`Arc<dyn PortTrait>`によるDIパターンに必要です。

```rust
use async_trait::async_trait;

#[async_trait]
pub trait ApiClient: Send + Sync {
    /// Uploads a context payload to the server.
    async fn upload_context(&self, context: &ContextPayload) -> Result<(), CoreError>;
}
```

## アーキテクチャルール

本プロジェクトは**Hexagonal Architecture（Ports & Adapters）**を厳密に遵守しています。コントリビューション前にこれらのルールを理解してください。

### コア原則

**`oneshim-core`がすべてのPortトレイトとドメインモデルを定義します。** 他の9つのクレイトはAdapterです。

```
oneshim-core  (port definitions, models)
    <- oneshim-monitor   (system monitoring adapter)
    <- oneshim-vision    (image processing adapter)
    <- oneshim-network   (HTTP/SSE/WebSocket adapter)
    <- oneshim-storage   (SQLite adapter)
    <- oneshim-suggestion <- oneshim-network
    <- src-tauri          <- oneshim-suggestion
    <- oneshim-automation
    <- oneshim-app        (full DI wiring)
```

### 禁止パターン

Adapterクレイト間の直接依存は許可されていません。例えば、`oneshim-monitor`が`oneshim-storage`に直接依存することはできません。すべてのクレイト間通信は`oneshim-core`で定義されたトレイトを通じて行います。

許可される例外:
- `oneshim-suggestion` -> `oneshim-network`（SSE受信）
- `src-tauri` -> `oneshim-suggestion`（提案の表示）

### DIパターン

`Arc<dyn T>`を使用したコンストラクタインジェクションを採用しています。DIフレームワークは使用せず、すべての配線は`oneshim-app/src/main.rs`で手動で行います。

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

## 新機能の追加

新しい機能を追加する際は、以下の順序に従ってください。

### ステップ1: coreでPortを定義

`crates/oneshim-core/src/ports/`に新しいトレイトを追加します。

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

### ステップ2: Adapterを実装

適切なAdapterクレイトでトレイトを実装します。

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

### ステップ3: appでDIを配線

`crates/oneshim-app/src/main.rs`で実装をPortに接続します。

```rust
// crates/oneshim-app/src/main.rs

let my_service: Arc<dyn MyService> = Arc::new(MyServiceImpl::new());
let scheduler = Scheduler::new(my_service, /* other dependencies */);
```

### ステップ4: テストを作成

ユニットテストとインテグレーションテストの両方を作成します。

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

## テストの作成

### 原則

- **mockallは使用しないでください。** モックは手動で作成します。
- テストは各モジュールの末尾にある`#[cfg(test)] mod tests`ブロックに記述します。
- テスト用モックの作成にはPortトレイトを直接実装します。

### 手動モックパターン

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

### テストの実行

```bash
# フルテストスイート
cargo test --workspace

# 特定のクレイト
cargo test -p oneshim-core
cargo test -p oneshim-vision
cargo test -p oneshim-network

# 単一テスト
cargo test -p oneshim-storage -- sqlite::tests::migration_v7

# インテグレーションテスト
cargo test -p oneshim-app
```

### E2Eテスト（Webダッシュボード）

```bash
cd crates/oneshim-web/frontend
pnpm test:e2e          # フルE2Eテストスイート
pnpm test:e2e:headed   # ブラウザ表示あり
pnpm test:e2e:ui       # Playwright UIモード
```

## PRプロセス

### ブランチ戦略

```bash
# 新機能ブランチ
git checkout -b feat/vision-pii-filter-improvement

# バグ修正ブランチ
git checkout -b fix/network-sse-reconnect

# ドキュメントブランチ
git checkout -b docs/scheduler-architecture
```

### PR提出前チェックリスト

PRを作成する前に、以下のすべてを確認してください。

```bash
# 1. フォーマットチェック
cargo fmt --check

# 2. clippy警告: ゼロ
cargo clippy --workspace

# 3. すべてのテストが通ること
cargo test --workspace

# 4. ビルドが成功すること
cargo build --workspace
```

### PR説明の書き方

PRの説明には以下の内容を含めてください:

- 変更の動機と背景
- 実装アプローチの概要
- 変更のテスト方法
- アーキテクチャルール（特にクレイト間依存関係）の遵守確認

### コードレビュー

レビュアーは以下の点に注目します:

- Hexagonal Architectureの遵守（Port/Adapterの分離）
- Adapterクレイト間の直接依存がないこと
- `cargo clippy`の警告: ゼロ
- 手動モックのみ（mockall不使用）
- 英語でのコメント

## コミットメッセージの規約

[Conventional Commits](https://www.conventionalcommits.org/)に従います。

### フォーマット

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### タイプ

| タイプ | 説明 |
|------|------|
| `feat` | 新機能 |
| `fix` | バグ修正 |
| `perf` | パフォーマンス改善 |
| `refactor` | リファクタリング（動作変更なし） |
| `test` | テストの追加・更新 |
| `docs` | ドキュメントの変更 |
| `chore` | ビルド、CI、依存関係の変更 |

### スコープ

スコープにはクレイト名または機能領域を使用します。

`core`, `network`, `suggestion`, `storage`, `monitor`, `vision`, `tauri`, `web`, `automation`, `app`

### 例

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

## 問題の報告

### バグ報告

GitHub Issuesの**Bug Report**テンプレートを使用し、以下を含めてください:

1. **バグの説明**: 何が問題なのかの明確な説明
2. **再現手順**: ステップごとの再現手順
3. **期待される動作**: 本来どうなるべきか
4. **実際の動作**: 実際に何が起こるか
5. **環境**: OS、Rustバージョン（`rustc --version`）、関連する依存関係のバージョン
6. **ログ**: `RUST_LOG=debug cargo run -p oneshim-app`の関連出力

### 機能リクエスト

機能を提案する際は、Hexagonal Architectureの観点から説明してください:

- 新しいPortが必要か、既存のPortを拡張可能か
- Adapterをどのクレイトに配置すべきか
- 既存のクレイト間依存関係への影響

## ライセンス

本プロジェクトへのコントリビューションは[Apache License 2.0](LICENSE)の下でライセンスされることに同意したものとみなされます。

---

ご質問はGitHub IssuesまたはDiscussionsをご利用ください。
