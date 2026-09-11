# 上下文管理

## 事实来源与隔离边界

SQLite 保存本地事实：Document Revision、OCR/Artifact revision、Discussion 消息树、active head、provider node、context root、Job/checkpoint 和 Usage。Gemini provider ID 只是续接与缓存索引，失效后必须能从本地事实恢复。

来源根按 `document revision + paper model + exact provider route + pdf-source-v2 + 实际冻结根提示词 SHA-256` 隔离。模型切换建立新来源根，不重写已有成果，不把成果正文放入共享根。

## Gemini 论文根

论文根使用 Gemini Files + Interactions，完整 PDF 是权威事实。每次 Interactions 调用都重复 canonical system instructions/schema，因为服务端续接不保证继承这些配置。

Receipt 分开记录：

- `fileReuse`：是否复用远端 PDF file。
- `sessionResume`：是否从 provider interaction node 续接。
- `paperRootBranch`：是否创建论文根/从根重建。
- `cachedInputTokens` 与 `uncachedInputTokens`：token cache，不与前两者混淆。

## Discussion

每篇论文可有多个 Discussion。消息是 parent-child 树，`discussion_heads` 只保存当前 active head；“设为当前分支”移动 head，不复制、不删除、不 promote 内容。

新问题有两条执行路径：

1. provider parent 有效：发送本轮问题、Reader page 和本轮 canonical Block 引用，从 parent 续接。
2. provider parent 无效或需要重建：发送 PDF、当前本地消息路径、每条消息的生成完成状态、不可变 Block 快照和当前问题，创建新论文根分支。

assistant 先以 `streaming` 落库；delta 经事件发送，partial 每 250 ms checkpoint。取消保留 partial 并标记 `cancelled`；异常保留 partial 并标记 `failed`；只有 `interaction.completed` 后才发布 provider node、usage、citation 和 active head。

回答只接受合法 `[p. N]` 和本轮允许的 `[block: BLOCK_ID]`。页码必须在 revision 页数范围内；Block ID 必须来自本轮后端重新加载的当前 OCR。不存在“当前页 fallback citation”。

## 审计压缩

至少 8 条消息且估算上下文达到模型窗口 80% 才触发 `context_compaction`。估算包含 Discussion 正文、引用 Block 文本、约 300 token/page 的 PDF 预留和输出预留。

压缩是独立不可变 Artifact，保存源 Message ID、消息正文 SHA-256、Block 快照 SHA-256、revision/model/context epoch，并单独记录 provider node 和 usage。后续问题从压缩 node 续接；当轮 node 失效时使用完整压缩三字段（`summaryMarkdown`、`retainedClaims`、`unresolvedQuestions`）及当前请求重建。原消息永不被摘要替换。

讨论与压缩采用四份完整中文稿，源消息角色、完成状态、修正及未决事项均需保留；完成状态、来源哈希和压缩本身不等于学术验证。历史块不自动进入本轮引用白名单。现有 provider 续接机制不等于远端祖先历史已被删除，实际 token 效果需 receipt 验证。完整合同见 [discussion-generation.md](discussion-generation.md)。

## Brief 与辅助成果

Brief、Glossary、Symbol Table、Metadata 分别生成、校验和发布，各有独立版本与重试。来源根只包含原生 PDF 和最小确认，每项从根独立分支。术语表可显式参考冻结的 Brief 主题线索；符号表和 metadata 不传 Brief。旧成果根不命中新版来源根，旧任务保留原协议。详见 [brief-generation.md](brief-generation.md)。

## 局部能力

- 翻译：使用翻译模型；输入当前 Block、同页前后各两个 Block、Brief、命中术语/符号（术语同时匹配主名称与别名，共享别名保留多个含义；只传判断所需的定义、用途或范围，不传整份来源目录）；不发送 PDF，不续接论文根，也不提供追问。
- 普通解释：只接受一个文本 Block，从论文根创建一次性旁支；不进入 Discussion，不提供追问。
- Lens：Formula/Figure/Table 从论文根旁支，输入完整 PDF 上下文、当前 OCR 对象和成本优化 crop；初始结果保存 provider node。
- Lens QA：从当前 Lens node 多轮续接，保存在 `lens_qa`，不进入 Discussion。带到讨论中只复制 Lens 摘要、证据和版本快照。
- Outline：从论文根旁支；输入为原生 PDF + 冻结瘦 OCR 目录；Brief/术语只读。不读不写 Discussion。Deep dive 续接该 Overview revision，不进入 Chat。合同见 [full-outline-v1.md](full-outline-v1.md)。

这些分支共享对论文的全局理解，但 Chat、Explanation、各 Lens、Lens QA 及 Outline 的历史彼此隔离。

## 2026-09-09：论文 Lens 完整中文稿与 v2

论文 Lens 新生成仍从 PDF 来源根创建独立旁支；生成／修复正文和 v2 协议在入队冻结。初始生成附 model crop 与读者背景；一次修复续接初始节点，不重新附这两项。当前无 Brief 或辅助表前置条件，也不自动输入其他成果。旧任务按 v1，详见 [Lens 合同](lens-generation.md)。

## 2026-09-09：Lens 追问完整中文稿

Lens QA 每轮读取追问设置和读者背景，传入问题与选定 Lens 版本原内容，按 parentId 续接同一 Lens 的 assistant 节点；不重新附截图，也未新增本地历史重放。旧 Lens 内容可在追问中被纠正，提示词要求已成立的纠正继续有效，但原成果不自动改写。实际边界见 [追问合同](lens-qa-generation.md)。
