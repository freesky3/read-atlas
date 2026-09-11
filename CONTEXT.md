# 领域词汇表

## Paper（文档）

文库中一份受管理的 PDF 及其稳定身份。Paper 可以拥有多个不可变的 Document Revision。

## Physical Collection（物理集合）

与 Workspace 内真实目录一一对应的文库集合。Paper 的物理归属、同层手动顺序和拖放目标都以它为边界。

## Smart Collection（智能集合）

由版本化查询规则动态计算出的 Paper 视图，不复制成员，也不对应磁盘目录。它不能成为物理拖放目标或手动排序范围。

## Selection Set（选择集）

用户在当前文库视图中临时选中的对象，以及范围锚点、焦点和排除项。它只属于当前前端会话，不是持久业务数据。

## Selection Snapshot（选择快照）

提交动作时由 Selection Set 解析出的确定目标集合，包含 Paper、Document Revision、查询修订向量和动作前置条件。Batch 的成员由该快照固定，不随之后的筛选变化。

## Library Revision（文库修订号）

文库可观察状态每次成功变化时单调递增的全局事件序号。它用于事件排序和缺口恢复，不单独决定某个查询是否陈旧。

## Query Revision Vector（查询修订向量）

一个查询所依赖领域的修订号集合，例如目录、标签、生命周期或 OCR 状态。它用于避免无关领域的变化错误地使 Selection Snapshot 失效。

## Drop Evaluation（落点判定）

一次拖放、菜单动作或键盘改序是否合法及其原因的单一结论。鼠标拖拽、右键菜单与 `Alt+↑/↓` 共享同一份判定，并且都带原因码；拒绝必须可见，不能静默不响应。

## Batch（批次）

一次由用户明确发起、可跨多个目标并可在重启后继续审计的操作。Batch 的总体状态由其 Batch Item 的状态推导。

## Batch Item（批次项）

Batch 中一个独立目标及其冻结依赖、执行结果、错误和补偿记录。一个 Batch Item 的失败不会抹掉其他项的成功结果。

## Batch Plan（批次计划）

执行前对目标、资格、冲突、外部路线、潜在费用和不可逆影响的冻结预览。计划发生漂移后必须重新生成并确认。

## Compensation Batch（补偿批次）

为撤销另一个 Batch 的可逆成功项而创建的新 Batch。它可能部分成功，不代表数据库时间旅行或外部费用退款。

## Undo Token（撤销令牌）

允许用户单次请求补偿 Batch 的不透明凭证。它受有效期和并发前置条件约束，不能覆盖提交后发生的新修改。

## Reading Lifecycle（阅读生命周期）

Paper 级的未读、阅读中、已读、收藏、优先级、稍后阅读和复习安排。它与 Reader Session State 分离，并跨 Document Revision 保留。

## Reading Engagement（阅读参与记录）

某个 Document Revision 的首次打开、最近打开和最远阅读页记录。它用于计算进度，不等同于 Reader 当前停留页。

## Reader Session State（阅读器会话状态）

用于恢复阅读工作台的当前页、偏移、缩放、旋转、面板、草稿和布局状态。现有数据库 `reading_states` 承担这一职责。

## Provider Job（外部任务）

使用冻结 Provider 路线执行 OCR 或成果生成的持久任务。Batch 只能关联并编排 Provider Job，不能绕过它直接选择凭据或调用外部服务。

## Prepared Job Handle（预备任务句柄）

Job Module 为 Batch Plan 持久化冻结执行身份后返回的不透明引用。它不是可执行 Job，只有在确认时被原子消费后才会创建或复用 Provider Job。

## Cost Preview（费用预览）

批次开始前基于版本化价格与工作量假设给出的精确值、上界、估算区间或未知结果。最终费用以 Usage Receipt 为准。

---

## 实现入口（给下一个 agent）

词汇在本页。动手先读：

1. [docs/library-workspace.md](docs/library-workspace.md) — D-063 现状、不变量、文件入口、踩坑、**§11 变更日志**（可追加）
2. [docs/library-workspace-plan-2026-08.md](docs/library-workspace-plan-2026-08.md) §11.0 — 切片落地进度（§1 是实施前快照）
3. [docs/hub-sort.md](docs/hub-sort.md) — 仅当动排序下拉 / 手排
4. [docs/data-protocols.md](docs/data-protocols.md) — 改 IPC / 投影字段时

不要从过期 handoff 段落或计划 §1 推断「Batch 还没有 writer」或「Hub 仍只读 `list_documents`」。
