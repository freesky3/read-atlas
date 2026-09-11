# Read Desktop V2 产品规格

> 状态：实现基准（2026-08）  
> 说明：本文汇总 V2 产品讨论中已经确认的行为。标记为“待打磨”的输出格式允许在不改变数据合同和产品边界的前提下优化。

## 1. 目标

V2 完成三个纵向目标：

1. 补齐 V1 的真实阅读闭环，包括引用、Chat 上下文、失败恢复、后台任务和真实 Brief。
2. 建立以 Mistral OCR 为语义基础的 Block 交互，并提供 Formula、Figure、Table Lens。
3. 建立可长期使用的本地文库，包括嵌套 Collection、文件双向同步、阅读状态与空间管理。

V1 基础闭环是 V2 的发布门槛。各模块可以并行开发，但不能用演示 selection、伪引用、不可恢复任务或静态 Brief 代替真实闭环。

## 2. 明确不做

- 不做文档级或文库级文本搜索。
- 不引入 FTS5 或向量数据库。
- 不做 Semantic View 或正文重排。
- 不使用 PDF.js 文本提取结果进行阅读顺序、布局分析或全文语义回退。
- 不把 OCR 文本伪装成完整 PDF 模型上下文。
- 不做跨论文 Chat、跨 Workspace 统计或跨 Workspace 搜索。
- 不做多文档标签页、并排阅读或跨论文比较。
- 不做 V1 数据迁移；开发阶段允许重置旧 Workspace。
- 不在本工作中修改已经完成的模型设置页面。
- 不接入 Crossref、Semantic Scholar 等外部 metadata 检索。
- 不允许直接编辑 Brief、翻译、解释、Lens 或其他模型输出，也不提供私人笔记。

## 3. 核心领域模型

```text
Workspace
├── Collection (任意嵌套，对应 Papers/ 或 Textbooks/ 下文件夹)
├── Paper (kind 由路径第一段推导：Papers → paper，Textbooks → textbook)
│   ├── PaperMetadata
│   ├── DocumentRevision
│   │   ├── NativePdfSource
│   │   ├── OcrRevision
│   │   │   └── OcrBlock
│   │   ├── PaperOrientationPack
│   │   ├── Discussion
│   │   │   └── MessageBranch
│   │   └── Artifact
│   │       ├── Translation
│   │       ├── Explanation
│   │       └── Lens
│   │           └── LensQa
│   ├── OutlineMap
│   │   ├── OverviewGraph
│   │   └── DeepDiveGraph
│   ├── ReadingState
│   └── UsageReceipt
├── DurableJob
└── ReconciliationConflict
```

关键约束：

- 一个 Paper 在当前 Workspace 中只能有一个受管理的物理 PDF。
- Document Revision 由 PDF 内容变化产生；OCR Revision 只是同一 PDF revision 的语义解析版本。
- 所有 Artifact、Chat 路径、任务和 usage 都显式绑定其依赖版本。
- SQLite 是论文身份、成果、任务和分支路径的本地事实来源；provider ID 只是续接或加速索引。

## 4. Workspace 与文库

### 4.1 目录布局

```text
<Workspace>/
├── Papers/                  # 论文 PDF，用户可见
│   └── 任意嵌套 Collection/
│       └── original-name.pdf
├── Textbooks/               # 教材章节 PDF，规则与 Papers/ 相同
│   └── 任意嵌套 Collection/
│       └── ch3.pdf
├── export/                  # 阅读成果导出（懒创建），不入库
└── .read-desktop/           # 应用内部数据
    ├── workspace.sqlite3
    ├── artifacts/
    ├── cache/
    ├── logs/
    └── trash/
```

- `Papers/` 与 `Textbooks/` 不放 manifest、OCR JSON、crop、数据库、对话或日志。
- PDF 保留自然文件名，不改造成 `<document>/<revision>/source.pdf`。
- Collection 与 Windows 文件夹双向同步；PDF 展示文件名与 Windows 文件名双向同步。
- 标题、作者、年份、DOI 等 metadata 与文件名相互独立。
- 数据库中的文档路径使用 Workspace 相对路径，且第一段必须是 `Papers` 或 `Textbooks`。

### 4.2 导入与监听

- 用户复制 PDF 到任意层级 Collection 后，等待复制稳定，完成校验和 SHA-256 后自动导入。
- 使用路径、SHA-256 和可用的 Windows file identity 追踪文件。
- 正常外部重命名或移动自动同步到 SQLite。
- 应用内移动/重命名写入操作日志，防止 watcher 把自身事件重复解释为外部变更。
- 启动或恢复监听后执行 reconciliation；文件系统是存在性、名称和路径的事实来源，SQLite 是论文身份和成果的事实来源。

### 4.3 重复与冲突

- 检测到与现有论文 SHA-256 相同的第二个 PDF 时，不导入、不建立多路径别名，也不自动删除。
- 重复文件进入待处理冲突，提醒用户决定保留哪个位置。
- 目标重名、身份无法确认或连续快速变化时，暂停受影响文件的自动同步。
- 冲突期间不覆盖、不删除文件；已有成果可查看，依赖原 PDF 的新模型任务暂停。
- 外部移出或删除只把论文标记为 `source_missing`，保留全部成果，并支持按 hash 重新定位。

### 4.4 PDF 内容变化

- 受管理 PDF 在原路径被外部修改且 SHA-256 改变时，自动创建同一 Paper 的新 Document Revision。
- Collection、metadata 和标签保持不变；新 PDF 成为当前阅读版本。
- 旧成果保留为旧 revision 历史，但不叠加到新 PDF。
- `Papers/` 只保留当前 PDF，不在隐藏目录永久复制每个旧 PDF。
- 旧 PDF 已不可用时，旧成果仍可查看，但不能继续依赖旧完整 PDF 追问或重新生成。

### 4.5 Collection、标签与 Workspace

- Collection 支持任意嵌套。
- Paper 支持多个手动标签；多标签筛选默认使用 AND。
- V2 支持创建、打开和切换多个 Workspace，但同一时间只激活一个。
- 每个 Workspace 数据完全隔离；模型配置、API 凭据、主题和语言属于应用全局设置。
- 切换 Workspace 前必须暂停运行中任务或取消切换。
- Workspace 整体被外部移动后，可通过重新定位恢复。

## 5. Reader 与 OCR

### 5.1 PDF.js Reader

- PDF.js 是唯一正式 Reader。
- 内置 WebView2 PDF Viewer iframe 已删除；PDF.js 失败只提供系统 PDF 应用 fallback。
- 极端兼容失败时使用系统 PDF 应用打开。
- PDF.js 只负责原貌渲染、翻页、缩放、旋转和坐标体系。
- 不使用 PDF.js 做文本布局、阅读顺序或全文分析。
- 页面 Canvas 主要保存在内存，不持久化完整页面渲染图。
- 主窗口一次只显示一篇 PDF；切换论文时保存并恢复完整阅读状态。

### 5.2 OCR 前后能力边界

OCR 前：

- 用户只能阅读原始 PDF。
- 不启用 Text Layer、文字搜索、复制、字符选择、翻译、解释、Block 引用或 Lens。
- 用户必须显式点击 OCR，并在启动前确认页数、模型和费用估算。

OCR 后：

- Mistral OCR 是语义权威。
- OCR 结果以 overlay 映射到原 PDF；原 PDF 始终是唯一阅读表面。
- 不提供自由框选或字符级选择，只提供 OCR Block 选择。
- 不提供页范围 OCR；用户每次启动整篇 OCR。

### 5.3 OCR 数据合同

OCR revision 至少保存：

- page；
- block type；
- 归一化 bbox；
- 文本、LaTeX 或 HTML；
- OCR 模型与协议/schema 版本；
- 原始文本 JSON（压缩，去除不必要的重复二进制和内联页面图像）。

OCR 内部按页保存、校验和恢复；全部页校验成功后原子发布。

### 5.4 重跑与删除

- 重跑 OCR 时，旧 OCR 和成果保持可用；新 revision 完整校验后原子切换。
- 只对页码、类型、位置和内容均高置信匹配的 Block 自动迁移 Artifact 锚点。
- 无法可靠匹配的成果标为“基于旧 OCR”，从当前页面移除标记，但保留在成果历史。
- Lens QA 跟随对应旧 Lens 保留；Chat 引用是快照，不受重 OCR 影响。
- 用户明确删除 OCR 时，级联删除翻译、解释、Lens、Lens QA 和其他 OCR 依赖材料；保留 PDF、Chat、Brief、metadata 和 usage 审计。

## 6. Block 操作与引用

| OCR Block 类型                   | 操作               |
| -------------------------------- | ------------------ |
| 正文、标题、列表、注释、图表标题 | 引用、翻译、解释   |
| 公式                             | 引用、Formula Lens |
| 表格                             | 引用、Table Lens   |
| 图片/示意图                      | 引用、Figure Lens  |
| 未识别类型                       | 引用、解释         |

- 公式不提供普通解释，表格不提供普通翻译。
- 图表标题作为独立文本 Block 操作。
- 翻译和普通解释只作用于一个 Block。
- 跨 Block 需求通过引用篮进入 Chat。
- 引用篮支持跨页、非连续 Block；Composer 显示可定位、排序和移除的引用卡片。
- 发送时携带 Block 内容、页码、类型、bbox 和 revision。
- 发送后清空临时引用，消息永久保存发送时引用快照。
- 页面使用低干扰的“译 / 解 / Lens”标记表示已有成果。

## 7. 右侧面板与阅读成果

右侧面板默认只有两个固定、互斥标签：

```text
讨论 | 阅读成果
```

- 不左右并排挤在同一栏里同时开讨论和成果。
- Full Outline 不是第三永久标签。顶岛「地图」把工作台切到预设 `PDF | 地图`。合同见 [full-outline-v1.md](full-outline-v1.md)。
- 阅读成果始终只显示一个当前详情。
- 打开另一 Block 的翻译、解释或 Lens 时替换当前详情。
- 旧成果持久化并可从成果索引找回。
- 切换标签保留草稿、滚动位置和当前成果状态。
- Brief、翻译、解释、Lens 和模型输出在 V2 中只读，不提供私人笔记。
- 术语表和符号表是唯一允许字段级 override 的模型成果；用户修改永远优先。

## 8. 论文上下文与模型

### 8.1 模型职责

- 一个当前“论文模型”统一用于 Orientation Pack、全局 Chat、普通解释和 Lens。
- 翻译可以单独配置更小、更便宜的模型，不创建或续接论文根。
- Mistral 固定只负责 OCR。
- 每个 `document revision + provider + model` 懒创建独立论文上下文根。
- V2 论文根只支持 `native_pdf`；模型不支持原生 PDF 时在调用前阻止。

### 8.2 分支结构

```text
论文上下文根
├── Discussion A
│   ├── 当前消息路径
│   └── 编辑/重生成产生的消息分支
├── Discussion B
├── Explanation A (一次性旁支)
├── Formula Lens A
│   └── Lens 局部问答
├── Figure Lens B
└── Table Lens C
```

- Lens 与普通解释始终从论文根分叉，不从当前 Discussion head 分叉。
- Chat、Lens 和解释相互隔离。
- Provider ID 失效时从本地 PDF、Artifact 和分支路径重建。

### 8.3 原生 PDF 能力检查

- 调用前检查文件大小、页数、加密状态和 provider 已知限制。
- 不静默截断、不拆成局部上下文、不使用 OCR 文本冒充完整上下文。
- 不支持时阻止 Orientation Pack、Chat、普通解释和 Lens，并提示更换兼容模型或文件。
- Reader、Mistral OCR 和已具备 OCR 的翻译仍可使用。

## 9. Orientation Pack 与 metadata

首次论文初始化生成：

```text
PaperOrientationPack
├── Brief
├── TerminologyGlossary
├── SymbolTable
└── Metadata
```

- 四者由同一次完整 PDF root 调用生成，但独立版本化保存。
- 打开缺失 Brief 的 PDF 时显示非阻塞顶部提醒，不自动产生费用。
- 首次 Chat、普通解释或 Lens 请求若论文根缺失，进入同一个初始化确认；成功后自动继续原操作。
- 确认框说明完整 PDF 上传、论文模型和费用估算。
- 术语表与符号表允许字段级 override；符号具有作用域，不合并不同章节的同名符号。
- OCR 完成后可本地将页码证据对齐到 Block/bbox；不唯一时不猜测。
- Metadata 始终由 Orientation Pack 生成，而不是只在 PDF 内嵌数据缺失时生成。
- Metadata 来源优先级：用户修改 > 模型生成 > PDF Document Info/XMP > 文件名兜底。
- 模型 metadata 自动应用到未被用户锁定的字段。

更换论文模型不重新生成 Orientation Pack；新模型只会创建自己的论文根，首次请求不能保证缓存命中。

## 10. Chat 与 Discussion

### 10.1 多讨论

- 一篇 Paper/Document Revision 可以有多个独立 Discussion。
- 每个 Discussion 内部是正常的多轮对话，追问可看到同一讨论中的上一条回答。
- 新建 Discussion 从论文根创建干净分支，不读取其他 Discussion。
- 右侧“讨论”标签内部提供讨论切换器和“新建讨论”，不增加第三个顶层标签。
- 默认名称由首条问题生成，可由用户改名。
- V2 不做讨论内容搜索，只按最近使用时间列出。

### 10.2 消息分支

- 重新生成回答从同一用户消息创建新的回答分支，旧回答保留。
- 编辑旧问题从其父节点创建新分支，原问题和后续路径保留。
- 界面在分叉处显示简洁的分支切换。
- 只有当前选中路径进入后续上下文，其他分支不污染当前 Discussion。
- 每个回答保存实际 provider、model、context epoch 和 usage receipt。

### 10.3 模型切换

- 更换论文模型后继续当前可见 Discussion，并插入模型变化分界。
- 下一次发送时为新 `provider + model` 懒创建论文根和 context epoch。
- 从 SQLite 重建必要路径；不能跨 provider 复用父 response ID。
- 上下文允许时重放必要历史；超限时使用可审计的历史摘要，同时保留原消息。
- 切回旧模型时优先续接仍有效的旧根；失效则本地重建。

### 10.4 Document Revision 变化

- Discussion 绑定不可变 Document Revision。
- 新 PDF revision 成为当前后，创建新的空白 Discussion 和论文根。
- 旧 Discussion 只读保留并标为“旧版本讨论”。
- 用户可显式把旧消息或旧 Artifact 引用到新 Discussion，并携带来源 revision。
- 重新 OCR 或删除 OCR 不切换或删除 Chat。

### 10.5 流式与 partial

- Chat 流式输出。
- 中断或取消后保留已经输出的 partial，并明确标为未完成或已取消。
- partial 不伪装成成功的完整回答。

## 11. 翻译与普通解释

### 11.1 翻译

- 输入为选中 OCR Block、邻近 Block、现有 Brief、命中的术语条目和符号条目。
- 不发送完整 PDF，也不依赖论文根。
- 保存所用 OCR、Brief、术语表、符号表和模型版本。
- 翻译是原子发布的只读、版本化 Artifact，不提供局部追问。

### 11.2 普通解释

- 输入只允许一个 OCR Block，并携带文本、页码、类型和 bbox。
- 从论文根创建一次性旁支；不读取或污染 Chat/Lens。
- 不提供局部追问；用户需要继续讨论时引用原 Block 到 Chat。
- 结果原子发布。重生成成功后新 revision 成为当前，旧版本保留；失败时继续显示旧版本。
- 输出暂定采用：核心意思、展开解释、在论文中的作用、必要术语/符号、依据与不确定性。
- 篇幅按 Block 难度自适应，不增加“简短/详细”模式开关。

## 12. Lens

### 12.1 通用规则

- Formula、Figure、Table Lens 都是逐对象、用户显式触发的持久 Artifact，不批量自动生成。
- Lens 拥有完整原生 PDF 全局上下文，但不读取或污染全局 Chat。
- Lens 保留独立局部问答，以支持与论文主讨论无关的背景追问。
- 用户显式“带到讨论中”时，才把 Lens 摘要、证据和版本附到当前 Chat Composer。
- Lens 初始结果原子发布；Lens QA 流式输出。
- 重新生成期间旧 Lens/QA 可读；新结果校验并发布成功后删除旧 QA，失败则全部保留。

### 12.2 统一输出合同（暂定，待打磨）

三类 Lens 共用渐进式骨架：

1. 对象是什么以及在本文中的作用。
2. 如何阅读对象及核心结论。
3. 直观解释和必要背景知识。
4. 与前后文、论文论点及其他公式/图/表的关系。
5. 前提、假设、适用条件和限制。
6. 不确定、歧义或识别可能有误的部分。
7. PDF 页码、Block 与 bbox 证据。

类型扩展：

- Formula：符号及作用域；只有论文提供依据时才还原推导/计算步骤。
- Figure：坐标轴、图例、视觉编码、趋势、异常点和对比关系。
- Table：字段、单位、比较维度、关键数值、模式和例外。

必须区分论文直接证据、背景知识/推演和无法可靠判断的内容。不得把模型推演伪装成论文原文。

## 13. Usage、缓存与费用

### 13.1 单次 receipt

每次 Chat 回答、翻译、解释、Lens 生成和 Lens QA 保存独立 receipt。字段按 provider 实际能力记录：

- 总输入 tokens；
- 命中缓存 tokens；
- 未命中 tokens；
- 输出 tokens；
- reasoning tokens；
- 缓存命中率；
- 延迟；
- 估算/实际费用；
- PDF 文件复用；
- provider session 续接；
- 论文根分支；
- provider/model/context epoch。

未知字段保持“未知”，不能显示为 `0`。文件复用、session resume 和 token cache 必须分开展示。

### 13.2 聚合展示

- 单次回答下显示本次指标。
- Lens 标题显示该 Lens 累计。
- Paper 与 Library 统计显示整篇和 Workspace 累计。

### 13.3 授权

- 整篇 OCR 和首次论文根/Orientation Pack 启动前确认费用。
- 高频的 Chat、翻译、解释、Lens 和 Lens QA 以点击动作作为单次授权，不重复弹窗。
- 入口附近低干扰展示模型和粗略费用；完成后展示真实 receipt。

### 13.4 缓存生命周期

- 自动 token cache 由 provider 管理，应用只展示命中情况。
- 显式付费 cache 在论文最后一次模型活动后默认保留 2 小时；只有真实请求才续期，PDF 保持打开不续期。
- Session/response 分支遵循 provider 保留期，不发送虚假保活请求。
- 缓存到期只导致下一次未命中和论文根重建，不重新生成 Orientation Pack 或 Artifact。
- V2 不提供永久缓存选项。

## 14. Durable Job 与调度

### 14.1 任务类型与发布方式

- OCR、Orientation Pack、翻译、解释和 Lens 初始结果为可恢复结构化任务，采用原子发布。
- Chat 和 Lens QA 流式输出。
- 全局任务中心位于顶部入口，跨论文显示，不作为右侧第三标签。

### 14.2 不可变依赖快照

任务启动时冻结：

```text
document revision
OCR revision
论文根 / context epoch
provider + model
prompt/schema version
Brief / 术语表 / 符号表版本
```

- 任务运行中不得静默切换依赖。
- 依赖仍为当前版本时正常发布。
- 依赖过期时可保存为历史，但不得覆盖当前成果或显示当前页面标记。
- 被更新任务取代时标记为 `superseded`。

### 14.3 重试与错误

- 网络中断、超时、`429` 和可恢复 `5xx` 遵循 `Retry-After` 并有限次指数退避。
- API Key、余额、权限、模型能力和请求格式错误立即停止并显示可操作原因。
- 结构化任务从已校验 checkpoint 恢复；重试必须幂等。
- 不自动切换 provider 或模型。
- 每次 provider 调用独立记录 usage；失败调用计费未知时显示未知。

### 14.4 暂停与取消

- 暂停：在安全 checkpoint 停止后续调用，保留 staging 和恢复游标。
- 取消：禁止发布且不可继续，清理未发布材料，但保留任务和 usage 记录。
- Provider 已接受的请求不保证可撤回；晚到结果不发布。
- 关闭窗口且有任务时提供：托盘继续、退出并稍后恢复、取消任务并退出。

### 14.5 V2 默认并发

- 同一 Paper 同时一个 OCR。
- 同一 `revision + provider + model` 同时一个论文根初始化。
- 同一 Artifact 的重复请求合并。
- 改变当前版本的任务按依赖串行；互不依赖的交互任务允许并行。
- 每个 provider 默认最多 2 个并发付费请求；限流后自动降低。
- 用户正在等待的交互任务优先于尚未开始的后台任务，但不强制中断已提交请求。
- 超限任务进入可见队列，支持调整顺序、暂停和取消。
- V2 不增加并发设置项。

## 15. 空间、删除与远端对象

### 15.1 本地空间分类

1. 原始 PDF。
2. 持久成果：OCR、Orientation Pack、Chat、翻译、解释、Lens/QA。
3. 可重建缓存：缩略图、临时 Lens crop、派生投影、短期诊断和过期 provider 复用信息。

- 已发布 Lens 必需的对象图/display crop 属于持久成果。
- 运行中任务材料不可被普通清缓存删除。

### 15.2 空间管理 UI

- 文库顶部提供独立“空间管理”入口，不放入模型设置页。
- 显示 Workspace 总量和三类空间分解。
- 每篇论文显示总占用，可展开 PDF、OCR、Brief/术语/符号、Chat、翻译、解释、Lens/QA 和缓存。
- 论文详情显示同源空间摘要。
- 普通清理只直接删除可重建缓存。
- 删除持久成果必须进入对应流程并预览级联影响。
- 回收站单独显示占用、剩余保留时间和立即永久删除。
- Provider 远端存储不计入本地数字，单独显示已知值或“未知”。

### 15.3 删除 Paper

- 应用内删除整篇 Paper 时，PDF 与内部成果一起进入 `.read-desktop/trash/`，保留 30 天。
- 恢复优先回原 Collection；冲突时不覆盖现有文件。
- Explorer 外部删除不进入应用回收站，只产生 `source_missing`。
- 软删除时立即停止相关任务和远端续接，并尽力删除 provider 文件与显式付费缓存。
- 远端清理失败不阻塞本地软删除，保存为可重试 tombstone。
- Provider 无删除 API 时显示“远端删除不可验证”和可知保留策略。

## 16. 日志与隐私

- 日常日志不保存完整 PDF、Prompt、Block 正文、API Key、Authorization header 或 provider 原始 HTTP 响应。
- Chat 与已发布 Artifact 是产品数据，不属于诊断日志。
- 除 OCR 原始文本 JSON 外，模型调用只长期保存规范化结果、provider object ID、版本、错误类别和 usage receipt。
- 错误落盘前移除密钥、请求正文和本地绝对路径。
- 诊断包只能由用户主动生成；生成前展示时间范围、内容预览和脱敏说明。

## 17. 阅读状态

每篇论文至少恢复：

- 页码；
- 页内归一化位置；
- 缩放模式与数值；
- 旋转；
- 当前右侧标签；
- 当前 Artifact；
- Discussion、Composer 草稿和引用篮。

## 18. 旧数据切换

- 检测到旧 Workspace 时返回 `reset_required`，不得激活为当前 Workspace；自动引导用户进入 Workspace 设置并锁定导入。
- 重置必须先展示删除范围，再通过独立的第二次确认调用 `reset_workspace_v2`；取消或关闭界面不得删除任何文件。
- 允许清空旧 PDF、数据库、Chat、Brief、任务和成果。
- 保留应用全局的模型设置、API 凭据、主题和语言。
- 不实现旧 schema 的兼容读取或迁移代码。
- 导入错误不能只写入底部状态栏；Library panel 必须显示可见且可关闭的错误反馈。

## 19. 自主实施里程碑

里程碑拆分、依赖排序、并行安排和验收标准由实现代理维护，不再逐项要求人工确认。只有改变产品行为、费用、隐私或数据安全的事项需要重新决策。

### M1：领域与任务底座

- 新 Workspace 和 SQLite schema。
- Paper、Document Revision、OCR Revision、Artifact、Discussion、UsageReceipt、DurableJob。
- 版本约束、原子发布、依赖快照和任务幂等。

验收：应用重启后任务、成果版本和依赖关系能够恢复；旧 Workspace 进入明确重置流程。

### M2：Reader 与文库

- PDF.js 唯一 Reader。
- 嵌套 Collection 和 `Papers/` 双向同步。
- 导入稳定性、hash、Windows identity、冲突和 `source_missing`。
- 阅读位置完整恢复。

验收：应用内/Explorer 的移动与重命名一致；重复文件不误导入；切换论文不丢阅读状态。

### M3：OCR 阅读闭环

- Mistral OCR 后台任务、checkpoint、校验和原子发布。
- Block overlay、操作矩阵、页面成果标记和引用篮。
- 翻译与普通解释 Artifact。

验收：OCR 前没有伪语义交互；OCR 后引用可定位到正确页和 bbox；跨页 Block 能进入 Chat Composer。

### M4：论文上下文闭环

- Orientation Pack 与 metadata。
- 原生 PDF 论文根和 provider adapter 能力检查。
- 多 Discussion、消息分支、模型 context epoch 和本地重建。
- 单次与聚合 usage 展示。

验收：Chat 不再使用固定 selection 或页码 fallback；切换模型和 provider ID 失效时不丢本地历史。

### M5：Lens 闭环

- Formula、Figure、Table Lens。
- Lens QA、带到讨论、阅读成果索引和原子重生成。
- 统一输出合同的首版 Prompt/schema。

验收：Lens 拥有完整 PDF 上下文但不读取 Chat；不同 Lens 和 Chat 分支互不污染且可审计。

### M6：发布加固

- 失败恢复、暂停/取消、托盘继续、调度和限流。
- 空间管理、缓存、30 天删除和远端 tombstone。
- 日志脱敏、性能、崩溃恢复和端到端测试。
- 验证构建产物和命令路径中不再存在内置 PDF Viewer iframe 回退代码。

验收：所有发布门槛场景通过；任何失败都不会把 partial 结构化结果伪装为当前成果，也不会静默删除用户文件。

## 20. 后续待打磨但不阻塞数据合同

- 普通解释各部分的最终措辞与视觉层级。
- Lens 三种类型的 Prompt、字段顺序与展示组件。
- 费用估算文案与不同 provider 的能力映射。
- 文件冲突提醒的具体按钮和交互文案。
- 空间管理的排序、筛选和图表表现。

这些工作不得改变本文定义的上下文隔离、版本绑定、原子发布、只读成果和删除边界。
