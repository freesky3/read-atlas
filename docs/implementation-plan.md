# V2 实施计划与状态

状态日期：2026-08-17。`[x]` 表示代码与自动测试已落地；`[~]` 表示主要代码完成但仍有实机/真实 provider 验收；`[ ]` 表示尚未实现。

## 架构

- `WorkspaceModule`：V2 初始化/重置、独占锁、路径布局。
- `PaperModule`：嵌套文库、导入/移动/对账、阅读状态、空间、Trash。
- `JobModule`：durable queue、provider/OCR 并发、checkpoint、恢复、优先级。
- `ArtifactModule` / `ReadingArtifactModule`：OCR、Artifact head、override、Lens QA、翻译/解释/Lens。
- `OutlineModule`（M7，已授权未落地）：论证地图 revision/head、Deep dive 缓存。
- `PaperModelPort` / `OcrPort`：Gemini Interactions 与 Mistral OCR seam。
- `DesktopClient`：前端唯一 open/command/watch 入口。

## M0 — 保护基线与测试面 `[x]`

- 保留既有 Settings 工作台基线，实施过程中没有 reset/覆盖。
- 已建立 Vitest/Testing Library/jsdom 和 Rust tempdir/wiremock fixture。
- 已加入 PDF.js、Markdown/KaTeX、notify/fs2/lopdf/tokio/async-trait/flate2。
- App 已拆出 Settings、PDF Reader、Artifact、Operations、DesktopClient、throttle 和 Block quote 模块；Rust 已拆出深模块/provider seam。
- 正式文档已合并 V2 工作稿，旧 V0.1 描述清除。

## M1 — Workspace、数据库与任务底座 `[x]`

- `Papers/` + `.read-desktop/`、锁、旧库确认重置、SQLite WAL。
- `reset_required` 不激活 Workspace；前端自动引导 Settings、禁用导入，并用两阶段确认接通 `reset_workspace_v2`。导入失败在文库区域显示可见警告。
- 不可变 revision/head、依赖快照、operation journal、watcher reconciliation。
- 750 ms debounce + 两次 size/mtime 稳定检查后 hash。
- durable Job、每 provider 两个付费请求、同论文 OCR 串行、dedupe、checkpoint、恢复/`interrupted_unknown`。
- 队列暂停/恢复/取消和 priority 调整。

## M2 — 文库与 PDF.js Reader `[~]`

- 导入复制/就地登记、任意嵌套 Collection、Explorer 双向对账、内容变化新 revision。
- PDF.js worker、可见页 ±2、绝对定位槽 + 实测画布高度（已替代 spacer）、仅外部翻页才写 `scrollTop`、render 取消、无 Text Layer、系统应用 fallback。
- OCR bbox overlay 和统一旋转坐标。
- 500 ms 阅读状态保存，含引用篮；引用回跳 page/bbox。
- 阅读器捏合/滚轮缩放（只缩放 PDF）和可拖分割条。

待验收：真实长 PDF 的 P95/内存、不同 DPI 和系统 fallback 兼容性；触控板捏合需重启 Tauri 后实机点一次。

## M3 — Gemini 论文根、Orientation 与 Chat `[~]`

- Settings 中 Gemini 论文/翻译角色、Mistral Credential Manager。
- Files + Interactions 论文根，system/schema 每轮重发。
- Orientation Pack 四 Artifact 同批发布，metadata 自动生成。
- 多 Discussion、完整消息分支、active head、无 promote、模型 epoch。
- Markdown 气泡、真实 parentId 树、编辑/重生成旁支。
- 失败 assistant 删除且不进 list/tree；`send_chat` 使用包装后的 `ChatRequest`。
- Gemini 扁平 `text`/`document`/`image` input；Mistral OCR 4 扁平 bbox。
- provider ID 失效恢复、审计压缩、流式 delta/250 ms partial/取消保留。
- page/Block citation 严格校验，usage/file reuse/session/cache 分开。

待验收：live Gemini 模型 capability、429/timeout 之外的真实账号/计费表现。

## M4 — Mistral OCR、Block 与基础成果 `[~]`

- 上传/signed URL/OCR/staging/规范化/原子发布/远端清理。
- include blocks、HTML table、无 image base64，拒绝无 bbox Blocks。
- staging 恢复和保守重 OCR remap。
- Block 操作矩阵、跨页非连续引用篮、后端 canonical 快照、翻译/解释原子发布。

待验收：live Mistral 对 born-digital、扫描、公式/图/表 PDF 的响应差异。

## M5 — Lens 与阅读成果 `[~]`

- Formula/Figure/Table Lens、display/model crop 分离、typed validator、一次 repair。
- Lens QA 从 Lens node 续接并与 Discussion 隔离。
- 成功重生成切 head、保留旧 revision、删除旧 QA；失败不变。
- 右侧“讨论 / 阅读成果”标签和统一成果详情。

待验收：三类 Lens 在真实模型上的 schema/crop 质量和费用。

## M6 — 发布加固 `[~]`

已完成：

- 全局任务中心、priority、空间分解、Trash/restore、remote tombstone/retry。
- 软删除停止 OCR/Artifact/Discussion，并尝试远端清理。
- 脱敏诊断范围预览和用户主动 JSON 导出。
- 主包/PDF.js/Markdown-KaTeX lazy chunk。
- 托盘“继续运行 / 暂停并退出 / 取消并退出”；暂停事务保留安全任务，已提交但结果未知的付费请求转为 interrupted_unknown。
- 本地 Trash 已完成后，即使 remote tombstone 排队失败，删除命令也返回本地成功并携带 cleanup warning。
- Tauri release build 通过，生成 MSI 8.09 MB、NSIS 6.11 MB；release exe 单次隐藏启动烟测通过。

未完成：

- 缓存 TTL 的 provider 实机确认。
- MSI/NSIS 实装、升级、卸载和签名回归。
- 冷启动/内存/Reader P95、500 页和六类 PDF 实测。

## M7 — Full Outline 论证地图 `[~]`

代码已落地，待 live Gemini 实机验收。合同：[full-outline-v1.md](full-outline-v1.md)。

- `OutlineModule` + 瘦 OCR 目录；Job kind `outline_overview` / `outline_deep_dive`。
- 顶岛「地图」默认 `PDF | 地图`；dagre TB + 可拖节点 + 可收起详情栏；详情里跳 Block。
- Deep dive：局部页图集 + 一次构图；旧 OCR 使总图 `stale`，不自动重跑。
- 不做通用窗口管理器；不迁插件 Dexie/MV3。

## 自动门槛

提交前运行：

```powershell
npm run test
npm run build
cd src-tauri
cargo test --locked
cargo check --locked
git diff --check
```

Live provider 测试默认禁用，只在显式开关和用户提供凭据时运行。
