# 产品范围

## V2 目标

Read Desktop V2 提供“本地文库 → 原 PDF 阅读 → 用户主动 OCR → 可定位 Block 操作 → 全 PDF Discussion/解释/Lens → 可恢复成果与费用审计”的桌面闭环。

## 首发范围

- 单一可切换 Workspace；`Papers/`（论文）与 `Textbooks/`（教材）存放 PDF，`.read-desktop/` 存放应用数据。种类由顶层目录决定。
- 任意嵌套 Collection，与 Explorer 中目录/文件移动和改名对账。
- PDF 复制托管、SHA-256 去重、不可变 Document Revision、冲突而非静默覆盖。
- PDF.js worker Reader，可见页 ±2、缩放/旋转 render 取消、系统应用 fallback、阅读状态恢复；捏合只缩放 PDF。
- 用户主动 Mistral OCR；整篇 Blocks、0–1000 bbox、staging/checkpoint、重 OCR 保守 remap。
- 跨页非连续 Block 引用篮和不可变消息引用快照。
- Gemini Files + Interactions 或 OpenAI-compatible / Grok Chat Completions 论文根；多个 Discussion、完整分支历史、active head、流式 partial、取消保留、provider ID 恢复和审计压缩。
- Brief、Glossary、Symbol Table、Metadata 独立生成，共享 PDF 来源根。
- 文本翻译/解释与 Formula/Figure/Table Lens；Lens QA 与 Discussion 隔离。
- 论证／知识地图：OCR 后按整体构图、检查定稿与局部展开生成，当前为 outline-map-v4，保留旧 v3 兼容。合同见 [outline-generation.md](outline-generation.md)。
- Reading Guide V2：OCR 后的可选角色页边旁批，与 Discussion 独立；保留旧三人兼容。合同见 [reading-guide-generation.md](reading-guide-generation.md) 与 [guide-characters.md](guide-characters.md)。
- Artifact 版本/head、只读成果、术语/符号 override、Lens 重生成成功后删除旧 QA。
- durable Job、provider 并发限制、同论文 OCR 串行、合并重复任务、暂停/恢复/取消/优先级。
- Usage、空间统计、30 天回收站、remote tombstone、脱敏诊断预览/导出。
- Settings 工作台中的 Workspace、三家论文 LLM（`gemini`、`openai_compatible`、`grok`）各自的论文/翻译模型，以及固定 Mistral OCR 凭据。

## 当前首发仍明确不做

- 非 PDF 文档、文库级/文档级全文搜索和向量检索。Hub 的标题、作者、年份、标签和生命周期筛选不属于全文搜索。
- Semantic View、正文重排、PDF Text Layer 或 OCR 自动替换原版面。
- 多论文上下文、跨论文 Chat。
- V0.1 Workspace 数据迁移。
- 自动 OCR、后台静默上传或未获用户动作的付费调用。

## 下一里程碑：部分落地（2026-08-31）

D-063 PR 0–6 代码已落地。前端：文库选择从单篇升级为 Selection Set（复选框、Ctrl/Shift、Ctrl+A、Space / Esc），拖拽改为把手启动并带插入线、无效原因与 `aria-live` 提示，`Alt+↑/↓`、移到最前 / 最后与右键菜单共享同一份完整精确置换，目录树成为真实可折叠树并按 Workspace 记忆展开状态，新增「移动到……」跨根预览与首次交互导览。Hub 卡片列表走 `hub_page` 分页虚拟窗口，整层手排走 `collection_layer`，`chapter` 与后端 `chapter_sort_key` 同一份键。本地批量（标签 add/remove、移动、回收站、导出、生命周期、多文件导入与 Explorer 拖入 overlay）走 `library_act` 持久批次，任务中心增加批次分组。Reading Lifecycle（未读 / 阅读中 / 已读、收藏、稍后阅读、复习日期）与 Engagement（同 revision 单调 furthest page）已与 Reader 视口状态分开；末页才询问是否标为已读。侧栏智能集合是版本化查询，不是文件夹。后端：生产 schema 升到 8，全局 `library_change_seq` 与 8 个领域 revision 由写事务同笔推进，Hub 聚合投影消除逐 Paper 重开数据库的 N+1，`library_read` / `library_act` 合同与 Tauri、Memory 双 Adapter 已就位。批量 OCR / Brief 走 Prepared Job Handle 与费用预览，不另造 Provider 调用。

仍属发布机时门槛，不得写成已上线：实机 Windows Explorer 拖入、一次 500 文件、Windows P95 / 内存 / 装包体积。详细接口、状态机、迁移切片和发布 Gate 见 [文库批量管理、阅读生命周期与拖拽可发现性实施计划](library-workspace-plan-2026-08.md)，逐 PR 进度见该文档 §11.0。

## 发布前仍需实机验收

代码闭环不等于发布门槛已经通过。以下必须在有足够磁盘、Windows WebView2/Tauri 运行环境和真实 PDF 样本的机器上执行：Tauri release/NSIS/MSI、冷启动 P95、空闲/单 PDF 内存、Reader P95、500 页 PDF，以及 born-digital、扫描、公式/图/表、重复文件、Explorer 外部移动六类路径。
