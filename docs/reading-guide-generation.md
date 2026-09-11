# Reading Guide V2 生成合同

状态：authoritative（2026-09-09，D-068）
范围：中文读后备忘、动态人物墨迹、密度与发布。V1 历史合同仍见 [reading-guide-v1.md](reading-guide-v1.md)。

## 协议

- Job kind 仍为 `reading_guide`。
- 新成果：`reading-guide-desktop-v2`。备忘：`reading-guide-memo-v1`。语言：提交时的 `zh-CN` 或 `en`，备忘、落笔、修复、补充与发布一致使用任务快照。详见 [语言合同](bilingual-ui.md)。
- 缺少协议信息的已排队任务按 V1 执行。未知未来协议拒绝，不降级猜测。

## 流水线

选择文档与本次人物 → 本地计划冻结依赖与阵容 → 入队使用冻结头像副本 → 复用或生成读后备忘 → 按完整正文分批落笔 → 结构／人物／定位校验 → 有上限的修复或补充 → 事务发布快照。

角色试写独立，不进入成果流程。

## 备忘

论文与教材同一结构：`documentFocus / spans / observations / connections / pageHints / limitations`。`basis` 为 `explicit / inference / reader_reaction`。V2 不再把 Outline 单元当作合格备忘。

## 墨迹

模型返回 `{ "inks": [...] }`，字段固定为 `id, kind, speakerId, blockId, weight, body, parentId`。页码与 bbox 由应用从 OCR 复制。speakerId 必须属于冻结阵容。一块最多一条主 note。

## 密度

`note` 计文字密度；`trace` 不补足文字目标。普通正文每页 3–5 条为校准目标，不是 schema 失败条件。只有 trace 的正文不能标为成功旁批。

## 提示词

运行时：`src-tauri/prompts/guide-context.*.md`、`guide-annotate.*.md`。定稿：`docs/note/reading-guide-prompt.md`。已知旧出厂成对升级；自定义保留 V1。


## 已接入的执行约束

1. **计划冻结**：`guide_plan::prepare` 从同一提示词快照解析配对协议，冻结 Reader、OCR/PDF 摘要、完整人物快照、语言及完整分片批次。`planId / planDigest` 存入 `reading_guide_plans`；开始时校验当前输入、精确路由和模型，变化则要求重新计划。缓存可用性只影响调用估计，不改变输入摘要。
2. **阵容优先级**：本次明确选择 → 文档偏好 → 工作区默认。空选择报错；只有成功入队才保存实际阵容。不同路由也不能同时启动同一文档的旁批。
3. **调用估计**：`rootCalls` 单列首次建立 PDF 来源根；`understandCalls` 为 memo 初次调用；每批一次初稿。memo 最多一次结构修复；每批最多一次结构修复及一次覆盖补充。展示估计包含根调用，费用未知时不伪造金额。
4. **独立备忘**：从 PDF-only 来源根分支调用；根不混入 Reader、人设、Brief 或其他派生产物。memo 输入可带 Reader。缓存键包含文档/PDF、OCR、目录、文档种类、memo 协议、读懂/根提示词、Reader、route 和 model，不含角色。
5. **材料与预算**：保留完整正文与片段元数据，邻接块只能作 context。按系统稿、人设、Reader、备忘、已有锚点摘要和输出预留预算；发送前再检查实际请求字符估计，超预算跳过该调用并记录原因，不偷偷截短原文。分片共享父块，合并后最多一个主 note。
6. **恢复**：`guide_generation::call_once` 为每个逻辑调用写入 in-flight 状态，响应原文与 receipt 在解析前保存。收到的响应可重放，未知结果不自动重发。`guide_runtime::record_receipts` 按 job、逻辑槽及路由去重。结构无效不会丢失已收到的付费结果。
7. **覆盖与发布**：逐页统计 note、trace、reply；记录稀疏页、未处理块与材料缺口。只有 trace 不能成功发布；存在缺口时保留可用旁批并标记 partial。发布事务同时记录稳定 publishId；旧任务恢复不能覆盖更新的 head。
8. **存储校验**：模型必须返回平坦 `blockId` 契约，禁止模型提供 `anchor/bbox`。持久化结果的 `anchor` 由独立恢复器校验，保留已有 ID 与回复关系，避免被误当模型响应全部丢弃。

历史 V1 任务仍使用旧提示词与三人校验器。早期不完整的 V2 任务若没有冻结 PDF、批次或根提示词，会在继续调用模型前明确要求重新计划。真实模型口吻、密度观感和桌面视觉验收仍属 P7，自动化测试不能替代。

## 开源准备：生成确认提示

确认页在提交前显示中英文的时间与 token 消耗提醒，不新增金额或耗时估算，不改变原有计划冻结与显式确认流程。实际界面见 [中文预览](assets/margin-note-confirm-zh-CN.png) 和 [英文预览](assets/margin-note-confirm-en.png)。
