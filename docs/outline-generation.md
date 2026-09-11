# 大纲与脉络图谱生成合同（地图 V2 / outline-map-v4）

状态：implemented，代码与自动化复审通过（2026-09-11）。产品方向见 [outline-map-v2-plan.md](outline-map-v2-plan.md)。v3 历史合同见 [full-outline-v1.md](full-outline-v1.md)，本文覆盖新生成路径。

## 1. 产品身份

地图回答：问题、设计、证据、结论和边界如何联系（论文），或概念、前提、推导、方法和应用如何依赖（教材）。节点是可独立表述的认识，不是目录，不是 Brief。

恰好两层：总图 + 按需局部图。局部图节点只跳原文／已有 Lens，不生成第三层。

## 2. 协议

| 对象 | 标识 |
| --- | --- |
| 持久化图语义 | `outline-map-v4` |
| 总图候选 | `outline-draft-v4` |
| 总图检查定稿 | `outline-review-v4` |
| 局部展开 | `outline-deep-dive-v4` |

任务 payload 必须显式写 `mapProtocol`。缺少新标识且符合旧 `{prompts.extract, prompts.compose}` 形状时走 v3。无法识别或未来版本在付费前报错。

不 bump 全局数据库 schema，不递增 `OUTLINE_EPOCH`。

## 3. 图字段

节点：`nodeId`、`title`、`takeaway`、可选 `roleLabel`、`references`（至少一项有效定位）、可选 `uncertainty`。

关系：`edgeId`、端点、`direction`（`directed`｜`undirected`）、自由文本 `label`、`rationale`、`references`。允许自环、回路、平行边、空边、多分量。

引用：`blockId` 和／或 `pageNumber`，加上 `purpose`。block 与页码同时给出时必须一致，应用不改写错误页码。无 block 的页级定位在界面标为页级。

`groups`、`gaps` 可选。未连通或缺少某种角色不是缺口。

模型不输出 protocolVersion、planId、bbox。

## 4. 生成流程

计划阶段不调用模型，冻结 PDF／OCR／目录／提示词／route／model／期望 head，写入 `outline_plans`。

总图：PDF-only 来源根 → 整体构图 → 结构校验（共享一次修复）→ 保存候选（不移动正式 head）→ 独立复核（显式传递 candidateGraph）→ 结构校验（若额度仍在可修一次）→ 事务发布（`expectedHeadId` + `publishId`）。

局部图：冻结父修订与节点；全文目录作定位白名单；优先关注页来自目标与相邻引用；一次调用，最多一次修复。worker 不读取 `current_graph`。

默认不注入 orientation／Discussion／旁批／读者上下文。

## 5. 提示词

物理槽仍为 `outline_extract`／`outline_compose`／`outline_deep_dive`。v4 显示为整体构图／检查与定稿／局部展开。六份中文稿见 [note/outline-prompts.md](note/outline-prompts.md)。

论文与教材 bundle 独立。已知旧出厂稿升级；自定义旧稿保留 v3。混合协议在生成前拦住。备份 `before-outline-map-v4.json`。


## 6. 复审后的执行与恢复约束

1. 总图和局部图都先展示冻结计划。开始提交 planId／planDigest，并核对文档、节点、完整定位目录、PDF／OCR、模型、精确 route、窗口和提示词。同一计划复用已存在任务；失败任务从任务中心恢复，重新生成使用新计划。
2. 系统稿、原生 PDF 估计、定位目录、候选／修复输出、schema 与输出预留共同参与发送前检查。超限保留候选及已付费结果，不截断材料。
3. rootCalls 单列来源根调用额度，局部确认显示冻结额度。实际 Root 发送处也核对额度并写入 rootRequestIssued，来源缓存失效不会绕过预算或自动重建再调用。
4. 原始响应与收据先于解析保存。解析和引用／结构错误都可以进入一次共享修复，修复输入保留原文。恢复重放已收到结果，未知结果不自动重发。
5. outline_runtime.rs 的适配器保留取消期间收到的已付费响应；收据按任务／逻辑槽／route 事务去重。调用前与发布前检查任务是否仍可执行。
6. 候选按 publishId 幂等保存，latestAttempt 不覆盖正式 head；早于后来正式图的失败候选不再显示为最新尝试。状态区分 unchecked、reviewed、reviewed_with_gaps 和局部 self_checked。
7. 总图／局部图发布事务检查来源及期望版本；局部版本也在付费前检查。发布完成标记保存在计划中，旧 revision 被替换或删除后仍能阻止恢复覆盖。
8. 删除与旧计划失效在同一事务中执行，捕获一次 WorkspaceRuntime 并与开始操作互斥。历史计划和收据保留，已删除的未完成候选不能被旧计划重新发布。
9. v4 任意自由角色／关系和合法拓扑均保留；结构错误、无效引用／分组／缺口定位及未知模型字段明确报错，不能静默丢弃后称作成功。
10. epoch 历史升级只更新兼容标记，不删除旧图、任务及恢复材料。提示词 current／previous 各自携带协议来源，整套新版默认一次写入，重复恢复默认不覆盖历史。

代码入口：outline_plan.rs、outline_generation.rs、outline_runtime.rs、outline_module.rs；前端计划、关系详情、列表降级及作用域检查位于 src/outline/ 和 App.tsx。地图 Markdown 导出保留可读关系、方向、理由、引用用途和不确定性。

## 7. 验证与待验收事项

2026-09-11 复审：完整 Rust 库 544 项通过，地图前端 52 项、设置相关前端 21 项通过，生产构建成功。包含 15 项模拟提供方集成测试，覆盖实际计划、任务、阶段编排、来源根、收据和发布模块。完整发现与限制见 [复审报告](outline-map-v2-review.md)。

- 真实桌面视觉、真实模型付费生成及人工论文内容核对尚未执行。
- 自动化测试不能证明模型生成的所有学术关系都正确。
- 窗口预算为保守估计，未知费用仍按未知展示。
- 早期缺少冻结材料的 v4 任务在继续付费前要求重新计划；旧 v3 路径保留。

修改 Rust 命令后必须重启 tauri dev。
