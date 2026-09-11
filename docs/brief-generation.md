# Brief 与辅助成果独立生成

2026-09-08：提示词讨论已确认并授权实施。本文替代旧 Orientation Pack 一次生成四类成果的合同；旧任务仍按其冻结协议执行。

- Brief、术语表、符号表、metadata 分别调用、校验、发布版本和重试。打开文档不自动生成，生成 Brief 不自动生成其余三项。
- 四类任务共享原生 PDF 来源根，根只返回最小确认，不包含已生成成果。来源根采用 `pdf-source-v2` 语义版本、实际冻结根提示词的 SHA-256 指纹和精确 Provider route / revision / model 隔离，旧成果根不可命中。缓存失效可重建；命中和费用以实际 receipt 为准。
- 每项任务从来源根独立分支；一次生成的输出不会变成其他成果的隐式历史。兼容 Provider 按其无状态能力携带完整 PDF。
- 术语表默认直接读 PDF。用户可选择参考当前 Brief 的主题线索；入队时冻结 Brief artifact ID、版本和所选字段。只提供 takeaway、keywords、classification、backgroundAndProblem，不提供 evaluation / futureWork，不用 Brief 限制术语覆盖。PDF 优先于 Brief。
- 符号表和 metadata 直接读 PDF，不传入 Brief。已有钉住的人工条目、metadata 字段继续保留。
- Brief 只输出 `{"brief": {...}}`，恰含 takeaway、keywords、classification、context、backgroundAndProblem、coreMethod、findings、evaluation、futureWork 九个字段。正文为中文，不注入 readerContext；原文专名和协议字段保持原样。
- 内部槽位 `orientation_pack` 保留作为 Brief 的兼容 ID；设置页名称改为 Brief，新增 glossary / symbol_table / metadata，总数 22。旧四合一稿在拆分迁移前备份；本次辅助稿升级只替换已知旧默认值，旧自定义稿及其兼容协议保留。新任务冻结实际使用稿及对应契约。旧队列无新协议标记时继续执行旧四合一处理器。
- 论文 Brief 最终稿位于 `src-tauri/prompts/brief.paper.md`；教材 Brief 保留教学语义的中文基础稿。文档根、术语表、符号表和元数据现已同步完整中文定稿，详见 [辅助成果生成](auxiliary-generation.md)。后续中英双语以中文稿为基准翻译。
- 单篇与 Hub 批量 Brief 均按文档种类冻结提示词。

验收：用离线 mock 验证来源根不含成果、旧根不命中、独立分支、严格单成果协议、冻结 Brief 依赖、钉住内容保留及种类选择；不把离线测试等同于真实模型质量评测。

## 原稿保真修订

论文 Brief 以用户提供的 [讨论原稿](note/prompt.md) 为内容依据。首次同步时，九字段及默认阅读深度仅调整标题、列表与软换行，不删减、不概括原文。开场四处适配和输出协议来源详见 [提示词手册 §6.19](prompts.md)。后续明确确认的正文修订同步更新讨论稿中的对应字段，其余内容保留。

验收增加原稿逐字段核对：忽略新增的 Markdown 排版标记及空白后，九个字段正文必须与原稿完全一致。该核对用于检查内容是否保留，不代表已验证模型输出质量。

## 2026-09-09：Brief 字段职责修订

用户确认不添加 sources，保留现有九个字段和字段名。其余六项正文及 JSON 输出协议保持不变；下列三项已同步到讨论稿、运行时稿和界面标题：

- context：展示为“学术脉络”，集中说明本文建立在什么已有认识之上，以及概念、理论、方法与既有研究之间的继承、结合或分歧关系。
- backgroundAndProblem：展示为“问题与动机”，集中说明本文要解决什么、困难为何出现、研究为什么值得开展，避免重复学术脉络。
- futureWork：展示为“未解问题与后续方向”，区分本文留下的未解问题、作者明确提出的方向和基于正文的延伸分析。可以只保留有依据的具体问题，不强求解决建议，不要求三类内容齐全；各类均缺乏依据时简要如实说明。

内部字段继续使用 futureWork，保持旧成果与任务结构兼容。修订前的完整默认稿保存在 `src-tauri/prompts/history/brief.paper.before-field-clarification.md`。设置使用 briefFieldsGeneration = 1，仅自动更新精确匹配该旧默认稿的论文当前正文；更新前备份配置，保留自定义正文、上一稿和教材稿。已入队任务仍执行其冻结正文。

本次验证：20 项提示词设置测试和 15 项 ArtifactPanel 界面测试通过。九字段保真检查通过，确认仅三个字段正文改变，其余六项、JSON 协议及四份辅助定稿保持不变。

## 2026-09-11：教材 Brief 专用结构

新版教材使用 briefProtocol=textbook-v2，九字段为 takeaway、keywords、learningScope、motivation、prerequisites、knowledgeStructure、coreKnowledge、masteryGoals、connections；不添加 sources。论文九字段及旧教材成果保留。单篇、Hub批量、读取、关键词编辑、展示、导出和术语主题线索均按版本分派。详见 [教材适配合同](textbook-generation.md) 与 [完整定稿](note/textbook-prompts.md)。
