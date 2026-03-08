# 安全策略

我们高度重视 ONESHIM Rust 客户端的安全性。如果您发现了安全漏洞，请按照本文档中的流程进行报告。

## 报告安全漏洞

**请勿将安全漏洞作为公开 Issue 提交。** 请使用以下私密渠道。

### 报告方式

1. **电子邮件**: 发送邮件至 `security@oneshim.dev`。如有可能，请使用 PGP 加密。
2. **GitHub Security Advisory**: 您可以在仓库的「Security」标签页下选择「Report a vulnerability」进行私密报告。

### 报告中应包含的信息

为确保我们能有效响应，请尽可能提供以下信息。

- **漏洞类型**: 如适用，请提供 CWE 标识符（例如 CWE-79 XSS、CWE-89 SQL 注入、CWE-200 信息泄露）
- **受影响的 Crate**: 包含漏洞的 crate 名称和源文件路径（例如 `crates/oneshim-vision/src/privacy.rs`）
- **复现步骤**: 逐步的漏洞复现说明
- **影响**: 漏洞被利用后的预期影响（本地数据泄露、远程代码执行等）
- **概念验证 (PoC)**: 如有，请提供演示漏洞的代码或截图
- **修复建议**: 如有修复思路，请一并提供（可选）
- **环境**: 操作系统、Rust 版本（`rustc --version`）、cargo 版本及相关 crate 版本

### 特别关注的安全领域

以下是 ONESHIM Rust 客户端中安全性尤为重要的领域。

- **截图捕获与 PII 过滤**（`oneshim-vision`）：绕过屏幕上个人可识别信息的遮罩处理
- **本地 SQLite 存储**（`oneshim-storage`）：未加密数据的未授权访问
- **JWT 认证令牌**（`oneshim-network`）：令牌窃取或验证绕过
- **自动化控制**（`oneshim-automation`）：通过绕过策略验证实现任意命令执行
- **自动更新**（`oneshim-app`）：绕过更新二进制文件的完整性验证
- **本地 Web 仪表盘**（`oneshim-web`）：本地 API 的未授权访问（与 `allow_external` 配置相关）

## 支持的版本

以下版本将获得安全更新支持。

| 版本 | 支持状态 |
|---------|---------------|
| 最新 `main` 分支 | 支持 |
| 最新 release 标签 | 支持 |
| 旧版本 | 不支持 |

由于尚未发布正式版本，请针对 **最新 `main` 分支** 报告安全漏洞。

## 响应时间 SLA

收到安全报告后，我们将按以下时间线进行响应。

| 阶段 | 目标时间 |
|-------|----------------|
| 确认收到报告 | 3 个工作日内 |
| 漏洞评估及响应方案 | 14 天内 |
| 发布补丁 | 90 天内 |
| 通知报告者并确定披露时间表 | 补丁发布后立即进行 |

对于紧急安全问题（如远程代码执行或完全认证绕过等高危漏洞），请在邮件主题中加入 `[URGENT]` 标记以获得优先处理。

## 负责任的披露策略

ONESHIM Rust 客户端遵循 **负责任的披露（Responsible Disclosure）** 策略。

### 我们的承诺

- 我们将保护报告者的隐私。
- 修复完成后我们将通知报告者，并在其同意下协调披露时间表。
- 如报告者愿意，我们将在 Security Advisory 中致谢其贡献。
- 我们不会对善意的安全研究活动采取法律行动。

### 对报告者的请求

- 在漏洞修复前，请勿公开披露。
- 请确保您的漏洞验证不会影响其他用户的数据或服务。
- 未经事先授权，请勿销毁、修改或窃取数据。

## 安全联系方式

| 渠道 | 联系方式 |
|---------|---------|
| 安全邮箱 | `security@oneshim.dev` |
| GitHub Security Advisory | 仓库 Security 标签页 |

## 安全更新通知

安全更新将通过以下渠道发布。

- GitHub Security Advisories
- 发布说明 (CHANGELOG.md)
- GitHub Releases 页面

## 完整性参考

- 独立模式完整性基线: `docs/security/standalone-integrity-baseline.md`
- 完整性操作手册: `docs/security/integrity-runbook.md`
- 本地完整性验证脚本: `scripts/verify-integrity.sh`

---

感谢每一位为提升我们安全性做出贡献的人。
