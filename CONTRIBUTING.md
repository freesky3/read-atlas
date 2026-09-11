# Contributing to Read Atlas

欢迎提交问题、文档改进和代码贡献。当前以 Windows Alpha 的可靠性、数据安全和清晰的阅读体验为重点。

## 开始之前

阅读 [开发指南](docs/development.md)、[产品范围](docs/product-scope.md) 和相关功能文档。涉及 Rust 时先看 [后端约定](docs/backend-hardening.md)；涉及数据结构时同时更新 [数据协议](docs/data-protocols.md)。

- 使用独立测试工作区和自制／允许再分发的 PDF，避免操作日常研究资料。
- 不提交 API Key、用户数据库、私人论文、诊断原文或签名文件。
- 真实模型调用可能计费；默认测试使用模拟服务，不读取个人凭据。
- 保留用户自定义提示词、历史成果、阅读状态和任务恢复材料。

## 提交变更

先为较大功能开 issue 说明问题与范围。分支可用 `codex/` 或清楚描述主题的名称。提交信息推荐 `feat(scope): ...`、`fix(scope): ...`、`docs: ...`。

每个 PR 说明解决的问题、最终行为、验证结果和仍未验证的部分。UI 改动附截图；同时更新 `zh-CN` 与 `en` 语言资源。截图使用演示数据，不包含私人文献。

```powershell
npm ci
npm run check:repo
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

修改依赖后重新运行 `npm run notices`、`npm audit` 和 `cargo audit --file src-tauri/Cargo.lock`。提交前执行 `git diff --check`。现有 Rust 格式和 clippy 警告应与新变更分开处理，避免无关全库重排。

## 报告问题

普通 bug 请使用 issue 模板，提供版本、Windows 版本、最小复现步骤和脱敏信息。安全漏洞使用 [安全报告流程](SECURITY.md)。

参与讨论时尊重他人，围绕可复现的事实和设计取舍。不要公布其他人的个人信息。

## License of contributions

By submitting an original contribution for inclusion in this project, you agree to license it under the project's MIT License. Only contribute material you have the right to share, and retain applicable third-party notices.

See [development setup](docs/development.md) for the pinned toolchain and Windows prerequisites. Run the checks above before opening a PR. Live provider calls are not part of the default tests.
