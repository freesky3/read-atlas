# 教材提示词生成与兼容合同

状态：implemented（2026-09-11）。本轮覆盖现有 22 个教材槽位，不增加答题、评分、答案隐藏、学习表现追踪或章节选择器。
完整正文：[教材提示词定稿](note/textbook-prompts.md)。
现有论文稿与共用文档根、翻译、元数据稿保持不变；应用仍以当前 PDF 为范围，区分整书、多章、单章和节选。

## 1. 功能分工与22槽覆盖

| 功能 | 教材适配 | 输出契约 |
| --- | --- | --- |
| 文档根 | 保留 PDF-only 来源缓存，不预先生成教学成果 | 原有确认对象 |
| Brief | 学习范围、知识结构、核心知识及能力目标 | 新 textbook-v2 九字段 |
| 术语表 | 独立教材稿，强调定义、直观含义、概念区别及当前用法 | 原有辅助成果 v2 |
| 符号表 | 独立教材稿，强调对象、维度／单位、下标和跨章作用域 | 原有辅助成果 v2 |
| 元数据 | 共用稿已区分整书／章节／节选及出版角色 | 原有辅助成果 v2 |
| 翻译 | 共用忠实翻译框架，保留题设、量词、条件及编号 | 原有翻译协议 |
| 解释 | 聚焦当前选段，连接直觉、形式与关键推导 | 原有五字段 |
| 精读路线 | 建立方向、连接概念与推导、独立应用与查漏 | 原有三阶段结构 |
| 讨论、讨论压缩 | 区分讲解、提示和检查尝试；保存卡点与实际表现 | 原有对话／压缩结构 |
| 公式／图／表 Lens 及三份修复 | 六份完整中文稿，按教学对象组织解释 | 复用 Lens v2 |
| Lens 追问 | 围绕当前对象补背景、变式和反例 | 原有追问协议 |
| 地图三稿 | 知识依赖、概念区别、推导与应用；自由节点和关系 | 原有地图 v4 |
| 旁批两稿 | 条件提醒、例题转折、前后呼应；保持人物与密度 | 原有旁批 v2 |

默认稿不固定年级、专业或考试目的。理解顺序、篇幅和例子按材料需要调整，不把“模型讲过”“读者表示理解”或打勾当作掌握证据。

## 2. 教材 Brief

新生成正文恰含九字段：

| 键 | 展示名称 |
| --- | --- |
| takeaway | 核心认识 |
| keywords | 学习主题／关键词 |
| learningScope | 学习范围与定位 |
| motivation | 为什么学习这些内容 |
| prerequisites | 必要基础 |
| knowledgeStructure | 知识结构 |
| coreKnowledge | 核心知识与方法 |
| masteryGoals | 应形成的能力 |
| connections | 后续衔接与应用 |

模型只返回顶层 brief；keywords 是字符串数组，其他八项为非空 Markdown 字符串。不添加 sources、模型名、时间或协议标记。应用使用 TextbookBrief 类型与独立 schema，验证后按既有 Brief 合同归一化 Markdown，保留 LaTeX。

应用在任务及成果中标记 briefProtocol=textbook-v2，通用 documentArtifactProtocol 保持 v1 的独立成果执行流程。任务冻结 responseSchema、正文、文档类型及来源根提示词。未知 Brief 协议或类型错配在模型调用前拒绝。

单篇与 Hub 批量采用相同版本选择；批量计划上下文显式携带教材 Brief 协议。get_brief 保留教材字段，卡片摘要和兼容 summary 使用 takeaway；教材新内容不映射进论文 findings／evaluation／futureWork。

界面与 Markdown 导出按实际协议显示七个正文栏目，关键词仍可编辑。旧教材 Brief 继续读取原字段，界面使用教学含义的栏目标题。卡片与空态使用“学习内容”“当前材料”，不默认整份 PDF 为一章。

术语表可选 Brief 线索时，新教材版本只携带 takeaway、keywords、learningScope、motivation；旧版沿用原四项。符号表、元数据和其他功能不因此获得整份 Brief。

## 3. Lens与输入隔离

- 教材新默认使用现有 Lens v2 的 status、limitations、quickTakeaway、sections、suggestedQuestions 和各对象字段；不新增另一套内容形状。
- 公式解释性质、含义、条件、结构及适用方式；图按实际视觉类型组织读法；表按比较、分类、查数或计算等用途组织。
- 生成与修复的默认选择同时考虑文档种类和协议。旧教材 v1 仍可使用已归档正文，教材新任务不回退到论文稿。
- 任务入队从同一设置快照取出正文、修复稿、来源根及协议，并保存文档类型。读取与恢复不重新判断当前目录来更换教材／论文语义。
- 来源根只承载 PDF。各功能按既有规则独立分支；Brief、地图及辅助成果不额外注入 Reader context。
- Reader context 只在已有支持的功能中调整讲解起点和深度。完整教材 Lens 已内置读者规则，不再追加旧英文 F2 说明。
- 内部 provenance 的 paper_defined 兼容值保留；教材界面显示“原文定义”，论文界面保持原有措辞。
- 修复最多一次，使用该任务实际 schema 和原始输出。partial／unavailable 保留诚实的材料限制，不因教材复杂或未提供证明而随意降级。

## 4. 无损迁移与文件

旧22槽正文及论文／共用稿 SHA-256 基线已归档至 src-tauri/prompts/history/textbook-before-adaptation/。它们用于默认稿识别和自动化保真检查。

配置 schema 保持 2；新增 textbookGeneration=1 和按槽记录的 textbookProtocols，其中 current／previous 分别记录教材 Brief 或 Lens 的协议身份。

1. 旧配置升级前，原始配置另存为 prompt-settings.before-textbook-adaptation-v1.json，不覆盖已有备份。
2. 只升级可识别的教材旧默认稿；兼容历史 F2 段及 CRLF。已有自定义稿和 previousText 保留。
3. 没有 previousText 时，用被替换的旧默认稿填入上一稿；已有上一稿时不覆盖。
4. 旧自定义 Brief／Lens 及其后续编辑仍属旧协议，即使编辑后的文字恰好等于新版正文，也不据此改变身份。
5. 明确恢复新版默认才切换协议；恢复上一稿同时恢复协议身份。重复恢复默认不覆盖可恢复的历史。
6. 论文侧及共用原稿不由教材迁移改写。旧成果和旧在途任务不重写、不自动生成；未知版本在调用前拒绝。

运行时加载独立教材文件，共用三稿继续引用既有文件。完整定稿文档展开所有22槽正文；已有解释、讨论、路线、追问、地图和旁批定稿中的教材部分也已同步。

主要代码入口：textbook_contract.rs、prompt_settings.rs、document_artifacts.rs、library_batch.rs、lens_contract.rs、reading_artifact_module.rs；展示和设置位于 ArtifactPanel、PromptCatalogSection、promptCatalog 及内存 DesktopClient，导出位于 export_module。

## 5. 验证与质量边界

本轮代码检查分规范和计划符合性两轴进行，修复了：
- 教材 Brief 新分支遗漏已有 Markdown 写入归一化。
- 单篇 Brief／Lens 在同次入队中重复读取文档类型，可能出现提示词与类型错配。
- 旁批正文和默认读者说明仍固定称为本章。
- 文库卡片、空态、引用标签未完全采用教材范围措辞。

自动化覆盖：
- 九字段、空内容、额外字段、来源字段禁止、未知协议与类型错配。
- 迁移保留论文 current／previous、自定义教材稿、原始备份、重复读取和默认恢复；相同正文、不同协议来源的恢复。
- 22槽定稿与运行时正文一致，论文与共用稿哈希不变。
- Hub 批量的新旧 Brief schema 分派、教学导出与 LaTeX 保留。
- 论文和教材三种 Lens 的直接成功、一次修复、失败不发布、partial、unavailable，以及来源根、读者注入与收据关系。
- 教材新旧展示、关键词编辑、设置协议状态和内存恢复。

内容评估样本按整书、多章、单章／节选、概念章、推导章、应用章、同符号跨章异义、无练习及材料不完整组织。提示词已针对这些情形审阅；没有运行真实模型生成样本，不能据此宣称教学质量已经实测通过。

不新增自动评分或掌握度判定。真实模型付费验收、人工论文／教材内容核对与桌面视觉验收仍待执行；本轮没有向真实模型发送用户文档。

## 6. 本轮实际验证

- 完整 Rust library：552 项通过，0 失败；包含论文／教材三种 Lens × 直接成功、修复成功、失败、partial、unavailable 共30个生产生成情形。
- 相关前端：68 项通过，7个测试文件，覆盖教材新旧 Brief、关键词编辑、设置协议与恢复、Lens卡片和精读路线。
- TypeScript／Vite生产构建通过；保留现有大chunk提示。
- 22槽定稿与运行时一致；论文及共用稿SHA-256基线保持一致；相关文件diff空白检查通过。
- 日志：textbook-full-rust.log、textbook-ui-tests.log、textbook-build.log。

两轴审查：规范轴2项（写入归一化、冻结快照）已修复；计划符合性2项（旁批范围、卡片/成果教材称呼）已修复。未运行真实模型付费质量评估或桌面视觉验收，未提交Git或部署。
