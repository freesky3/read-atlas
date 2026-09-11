# 论文 Lens 生成与修复合同

2026-09-09。六份论文提示词已经采用完整中文稿；公式的首要目标是帮助读者理解数学表达，图与表的首要目标是解释基本意思、读法、重点位置、实际观察及其含义。教材和 Lens 追问提示词未在本轮改写。

## 1. 正文与入口

- [六份完整定稿](note/lens-paper-prompt.md)：依次为公式、图、表、公式修复、图修复、表修复，与运行时文件逐字一致（仅忽略行尾格式和末尾换行）。
- 运行时：[公式](../src-tauri/prompts/lens-formula.paper.md)、[图](../src-tauri/prompts/lens-figure.paper.md)、[表](../src-tauri/prompts/lens-table.paper.md)、[公式修复](../src-tauri/prompts/lens-repair-formula.paper.md)、[图修复](../src-tauri/prompts/lens-repair-figure.paper.md)、[表修复](../src-tauri/prompts/lens-repair-table.paper.md)。
- 字段与本地校验：[lens_contract.rs](../src-tauri/src/lens_contract.rs)。生成、修复、发布：[reading_artifact_module.rs](../src-tauri/src/reading_artifact_module.rs)。配置及迁移：[prompt_settings.rs](../src-tauri/src/prompt_settings.rs)。展示：[ArtifactPanel.tsx](../src/ArtifactPanel.tsx)。

每份生成稿九部分，每份修复稿七部分。内容完整保留，不以短摘要替代。六槽继续要求字面量 `{output_language}`，当前前端填入 zh-CN。论文生成稿已内置读者规则，不再追加英文 F2 段；教材原稿及其读者段保持原样。

## 2. 讲解目标

### 公式

先建立整体直觉，再按理解依赖进入结构、符号、必要背景、例子和推导。读者可能没有相关数学背景，应讲明每个必要概念在当前公式中的作用；不能将符号名称列表当作完整解释。复杂公式可以充分展开。例子与类比按需使用，映射回原式，明确自拟内容。推导服务于理解，不对定义、设计选择或启发式伪造证明。保留数学条件、作用域、近似及论文联系。

### 图

先讲图意和视觉语言，有坐标时明确横纵轴、单位、尺度、图例，没有坐标时解释模块、箭头或样例布局。给出有理由的观察顺序，并把每个重点写成“具体位置—实际现象—含义”。多面板按关系解释。区分图中可见内容、作者解释与模型推断，不把样例当总体、示意当验证、视觉差距当统计显著。

### 表

讲清行列、多级表头、分组、单位、指标方向和脚注，必要时带读者读一个真实单元格，再说明重点比较。按性能表、消融表、统计表或设置表的用途选择读法。计算只在有帮助时加入，交代操作数、基准、口径与单位；不从缺失单元格推造数值，不将局部优势扩大成全面领先。

## 3. 真实输入与分支

1. 新任务冻结生成正文、修复正文、来源根正文、目标语言、读者包装及 `prompts.lensProtocol`。生成／修复正文与 Lens 协议从同一份设置快照读取，协议以生成槽为准。
2. 从只含 PDF 来源的根创建独立 Lens 旁支，生成请求包含 anchor、kind、allowedEvidenceIds、outputLanguage 和 model crop。display crop 保存为本地展示资产。当前无需先生成 Brief 或辅助表，也不会自动传入它们。
3. Reader context 只影响起点和深度，不是论文证据。各 Lens 与 Discussion、其他成果的历史隔离；本次不改变 QA、转入讨论、版本归并、OCR 定位或 provider route 的现有机制。
4. 校验失败后最多一次修复，续接本次初始 Lens 节点，使用同一个 schema，输入 validationError、invalidOutput、anchor、allowedEvidenceIds、outputLanguage 与修复任务。修复不重新附截图和 Reader context；提示词只允许使用原分支实际仍可见的内容，不能承诺恢复了缺失视觉材料。
5. 两次结果均不通过时不发布 Lens，记录已有调用 receipt。有效的 partial／unavailable 是诚实的可展示成果，成功发布后不因这些状态自动触发修复。后续重试、缓存命中及费用沿用现有任务与 provider 机制，不能仅凭复用来源根宣称 token 缓存命中。

## 4. v2 字段

所有对象与子对象禁止额外字段，字符串／数组类型严格校验。

| 共同字段 | 分工 |
| --- | --- |
| status | complete／partial／unavailable，描述本次解释可完成程度，不是研究结论的置信度 |
| limitations | 具体材料缺口及影响；partial／unavailable 必须非空 |
| quickTakeaway | title 与 markdown 均非空；简短定向，不能替代完整讲解 |
| sections | sectionId、title、markdown、evidenceIds；本次 ID 唯一，正文按理解需要组织 |
| suggestedQuestions | 0–3 条具体非空问题，不能把必要解释留给追问 |
| formula／figure／table | 只包含当前对象的专有字段 |

### 公式专有字段

- `whatItDoesMarkdown`：完整的整体直觉，界面显示“先直观理解”。
- `startHereMarkdown`：具体组块及理解顺序，界面显示“怎样读这个公式”。
- `reconstructedLatex`：可靠确认的完整原式，裸 LaTeX；partial 可为空，不能用猜测项凑成完整原式。
- `symbols`：每项为 symbolLatex、meaningMarkdown、provenance、evidenceIds。来源枚举为 paper_defined／standard／inferred／unresolved，界面使用中文说明。无非平凡符号时可为空。

推导、相关公式、例子与背景放在 sections，删除 v2 的 relatedFormulas 和 derivation 空对象数组。complete 要求两个讲解字符串、原式及至少一个 section 非空；partial 要求两个讲解字符串及至少一个 section 非空。

### 图与表专有字段

- `overallMarkdown`：整体表达什么。
- `readingGuideMarkdown`：图的坐标、图例或视觉编码；表的行列、指标与脚注；以及有理由的阅读顺序。
- `focusPoints`：按建议观察顺序排列，每项固定为 location、observationMarkdown、meaningMarkdown、evidenceIds。位置用真实标签、区域或行列文字描述；不提供交互坐标。

图的 panels／hotspots／roleTags、表的 cellLinks／calculations 不进入 v2。面板关系和必要计算使用阅读说明或 sections；三个阅读层次与补充 sections 均真实展示。complete／partial 要求两个讲解字符串非空，focusPoints 与 sections 按实际材料可为空。

### 无法解释

unavailable 通过 quickTakeaway 与 limitations 说明具体原因；专有讲解字符串及原式为空，symbols／focusPoints、sections、suggestedQuestions 为空数组。界面显示“暂无法解释”，不渲染空公式或空讲解栏目。不能为通过校验把可读对象随意降级；这由提示词约束并需质量评估，本地程序只能检查字段一致性。

## 5. 依据与校验的实际边界

- 所有结构化 evidenceIds 严格限制在本次选区白名单，数组允许为空但不允许重复值。不能把当前块冒充全文其他位置的证明；其他位置只在正文以已核实的编号、标题或物理页码自然说明。
- 本地检查嵌套字段、额外键、类型、枚举、空内容、问题数量、唯一 sectionId、引用白名单和状态一致性。校验不会判断数学证明是否正确、图像是否看对、计算是否真实或引用是否在语义上支持某句话。
- v2 不对生成正文执行旧版写入归一化，避免修改 LaTeX 与解码后的换行；现有显示层 Markdown 处理继续生效。v1 保留原写入归一化与验证合同。
- 没有新增第三次调用或独立学术审核；一次修复是错误恢复步骤，不保证发现所有学术问题。

## 6. 旧配置与历史兼容

1. 配置文件 schema 仍为 2，新增 `lensPaperGeneration=1` 和旧论文文本记录。首次读取旧配置前保存原始文件至 `prompt-settings.before-lens-paper-v2.json`，不覆盖已有备份。本轮开发未直接访问或改写用户的真实配置文件。
2. 仅将六槽论文侧已知的旧默认正文（含可选旧 F2 英文段、CRLF 差异）升级到新正文；旧自定义当前稿及 previousText 保留，教材和 Lens QA 保留。六份原始论文稿归档在 prompts/history。
3. 旧自定义论文稿及其后续编辑沿用 v1；恢复新默认后使用 v2，在新默认上编辑继续 v2；恢复已记录旧稿回到 v1。新任务按生成槽冻结协议；修复槽不改变本次输出协议，新修复稿明确按请求实际 schema 工作，允许修复 v1。自定义修复稿仍原样使用，需要遵守本次实际 schema。
4. 无 `prompts.lensProtocol` 的旧入队任务按 v1 执行；未知版本在调用前拒绝。v2 任务冻结后不受后续设置编辑影响。旧成果不重写、不强制重生成；只有带 schemaVersion=2 和 lensProtocol=v2 的成果展示新状态与图表字段。
5. v2 成果的 schemaVersion、lensProtocol、kind、sourceBlock、localEvidence、截图路径等由应用添加；它们不是模型输出字段。数据库和 Workspace schema 不升级。

## 7. 验证

- `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-fail-fast`：458 项通过，0 失败。新增覆盖完整正文一致性、论文默认迁移与旧自定义／上一版／原始备份保留、冻结版本、三个对象的状态及嵌套字段校验，以及三类对象的直接成功、一次修复成功、二次失败不发布、无法解释正常发布。来源根、初始结果与修复的费用记录分别核对。
- `npm test -- src/ArtifactPanel.test.tsx src/components/BlockCardStream.test.tsx --maxWorkers=1`：28 项通过，包含图表阅读说明、重点位置顺序、观察及含义、材料局限、无法解释时不显示空公式与旧成果兼容。
- `npm run build`：TypeScript 与生产构建通过。仍有现有的大于 500 kB chunk 提示。
- 六份定稿与运行时逐字一致，生成各九部分、修复各七部分；本轮合同的相对链接存在，涉及的已跟踪文件通过 `git diff --check`。

自动测试使用临时工作区与假模型；没有执行真实 provider 调用、付费质量评估或用户文库操作。这些结果证明字段、兼容与调用路径可工作，不证明真实模型已稳定达到直观讲解、准确读图和数值解释的质量目标。

## 2026-09-09：Lens 追问完整中文稿

此前六稿优化没有改写 Lens QA；现在追问已独立完成论文／教材两份中文稿，仍使用两字段 v1 协议，见 [Lens 追问合同](lens-qa-generation.md) 与 [完整定稿](note/lens-qa-prompt.md)。本页所述初始生成和修复合同保持不变。

## 2026-09-11：教材 Lens v2

教材六份生成／修复默认稿现已改为完整中文并接入相同 v2 schema；默认选择同时依据文档种类和冻结协议。教材旧自定义稿继续 v1，论文正文与原有fallback保留。教材原文定义的内部枚举兼容值不变。详见 [教材适配合同](textbook-generation.md) 与 [完整教材定稿](note/textbook-prompts.md)。
