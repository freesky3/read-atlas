# Full Outline V1

状态：historical（2026-08-17 产品授权；2026-09-10 起新生成由 [outline-generation.md](outline-generation.md) / D-069 覆盖）
范围：Read Desktop 深入阅读论证地图的 **v3** 合同：抽单元 → 构图、固定角色／关系枚举、narrative DAG。
参考：相邻 `read_addon` 的论证图语义、角色/关系枚举、校验思路和测试向量。不迁入 Dexie、MV3 runtime、消息总线、vision/文本降级或插件 UI overlay。

本文保留 v3 已实现行为，供旧任务、旧成果和自定义旧稿继续解释。新总图／局部图不要按本文的连通、无环或固定枚举实现。

本文是 Outline **v3** 的产品、数据、生成、持久化和 UI 合同。

## 1. 产品身份

| 表面 | 任务 |
| --- | --- |
| Brief | 整篇定向：一句话贡献与 Orientation Pack |
| Discussion | 开放追问 |
| Lens | 单个公式 / 图 / 表 |
| **Outline** | 深入阅读的论证地图：逻辑单元如何咬合、证据在哪 |

- 节点是可独立理解的**论证单元**，不是章节、不是目录复述、不是 Brief 加长版。
- 模型必须阅读论文内容并理清逻辑关系。禁止用 PDF.js 文本层或 OCR 全文冒充「已读完整 PDF」。
- 地图是论文的稳定资产，不是某次聊天的衍生品。

## 2. 明确不做（V1）

- 第三层及更深的子图。Deep dive 节点只跳 PDF / 打开 Lens，不再生成下一张 Outline。
- 自动在 OCR 完成后生成 Outline。
- 预授权全部 Deep dive。
- 重 OCR 或重生成后自动重跑、或把旧边迁到新 Block。
- 把 Outline 写入 Discussion，或从 Discussion 历史生成地图。
- Semantic View、Author Research、多 provider、搜索。Reading Guide 由 D-040 单独授权，合同见 [reading-guide-v1.md](reading-guide-v1.md)。
- 通用窗口管理器（任意拖拽、嵌套分栏、同一表面开两份）。
- Overview 阶段上传页图 / display crop，或为无 caption 的图编摘要。
- 硬性 10–18 节点失败条件；为消化未引用对象而编造节点。
- 读取或迁移 `read_addon` 的 IndexedDB 旧图。

## 3. 前提与输入

生成 Overview 或 Deep dive 之前必须同时满足：

1. 当前 Document Revision 有**已发布** OCR revision。
2. 当前论文模型支持原生 PDF；调用前走现有体积 / 页数 / 加密 / 能力检查。不支持则拦住，不静默截断，不用 OCR 正文冒充全文。
3. 存在或按现有流程初始化论文根（Gemini Files + Interactions）。Outline 从论文根**旁支**，复用 Files URI（`fileReuse`），不重复上传 PDF。

还没有 OCR：只给「先 OCR」入口。
还没有论文根：走与首次 Chat / Lens 相同的初始化确认，成功后再继续 Outline 计划卡。两张卡不合并。

### 3.1 两层输入

| 层 | 职责 | 内容 |
| --- | --- | --- |
| 论文根 PDF | 真正读懂 | 已缓存的原生 PDF |
| 瘦 OCR 目录 | 引用白名单 | 见下 |

目录行：

```text
id · page · type · blockIndex · bbox
excerpt?              // 正文约 80 字；公式优先 LaTeX
nearbyCaptionBlockId? // 仅当同页、版式上就是该对象的题注
```

- 页眉 / 页脚 / 纯参考文献 chrome 不进目录，或标成不可作主证据（与 Reader 点击归属一致）。
- 图 / 表无摘要：不编摘要、不寄 crop；模型靠 PDF 看见对象，靠 `page + blockIndex + bbox` 指认。对不上就不许引用。
- 目录 + PDF 超窗口：计划失败并说明原因，禁止静默丢掉中间页。
- Brief / Glossary / Symbol Table 只作只读定向，不是证据权威。
- 不读 Discussion，不写 Discussion。以后「带到讨论」必须用户显式触发；V1 可以只留入口位置，不实现发送。

## 4. 两层地图

恰好两层。

```text
Overview（粗地图）
  节点示例：「用 RNN 做模拟」
        │
        │ inspector「生成 / 打开局部图」
        ▼
Deep dive（该主张内部的关系图）
  节点示例：架构 · 公式 · 数据 及其 narrative / cross_link
        │
        ▼
  单击节点 → PDF / Lens
  禁止再生成第三张 Outline
```

- Overview：先能顺着主链读完整篇论证。
- Deep dive：这个主张内部怎么走，不是整篇再概括一遍。
- Deep dive 绑定 `overviewRevision + nodeId`。没有当前 Overview head，不能生成 Deep dive。

## 5. 图合同

复用插件的宽松角色 / 关系枚举，桌面自有协议版本，不假装二进制兼容。

### 5.1 节点

论证单元字段至少包括：`nodeId`、`roleClass`（含 `other` + `roleLabel`）、`title`、`takeaway`、`importance`（`core | supporting`）、`evidenceIds`（只允许目录内 Block ID）、`confidence`。

### 5.2 边

| 类型 | 含义 | 布局 | 拓扑 |
| --- | --- | --- | --- |
| `narrative` | 先懂这个再懂那个 | 决定 TB 分层 | 弱连通 DAG；可多入口 / 多出口 / 分支 / 汇合；禁止环。有并行主张 / 方法 / 证据时应做成同层兄弟并允许汇合；论文本身是一条线时允许保持单链。阅读序号是事后算的建议走法，不决定边。 |
| `cross_link` | 比较、限定、回指 | 默认隐藏，不参与分层 | 允许有环 |

关系标签用宽松枚举 + `other`。已知的「把节点角色填进 relationClass」映射到 `other`，不删整张图。

### 5.3 合格线

10–18 个 Overview 节点只是可读性建议，**不是**失败条件。

失败（不可把图写成当前 head）：

- 零单元或零节点
- narrative 有环或不（弱）连通
- 边端点不存在
- 引用了目录外 ID
- 空的 repair 主数组

覆盖率：明显主张句、公式、图、表若未被任何节点引用，记入 `coverage.warnings`。禁止为消化它们编造节点。有警告的有效图仍可发布。

单元有效但图无效：发布 `partialArguments`（可读单元列表），画布走列表，不画假图。

## 6. 生成协议

协议身份（变更 prompt/schema 语义必须 bump）：

- `outline-catalog-v1`
- `outline-extract-v3`
- `outline-compose-v3`
- `outline-deep-dive-v3`

Gemini Interactions 结构化输出走现有 `gemini_response_format`：`{ type: "text", mime_type: "application/json", schema }`，禁止 OpenAI `json_schema` 外壳。

### 6.1 Overview：一个 Job，两个顺序 Unit

Job kind：`outline_overview`。

1. **计划卡（不花钱）**：本地冻结目录、检查 OCR / 论文根 / 窗口、估算两次调用 + 最多一次 repair。用户确认后才入队。
2. **Unit A 抽单元**：模型只产出证据锚定的论证单元。本地校验 ID 白名单。成功则 checkpoint 冻结单元清单。
3. **Unit B 构图**：只读冻结单元 + 同一目录，产出 narrative DAG + cross_link + overview 文本。
4. 构图失败：最多一次**非破坏性**语义 repair（保留原指令与有效单元，禁止为满足 schema 删光主数组）。仍无效 → Job 以 `partial` 结束，发布可读单元列表，**不**切换到假图 head。
5. 成功 → 原子发布 graph + units + coverage，切换 Overview head。

取消：不切换当前 head。已 checkpoint 的单元可在用户再次确认后从构图接着走，不重抽。
暂停 / 托盘退出：走现有 Job 规则。已提交付费请求进 `interrupted_unknown`，不自动重试。

抽单元和构图禁止混在一次 Attempt 里连跑（避免构图失败连带重抽计费）。

### 6.2 Deep dive：一个 Job，一次结构化 Unit

Job kind：`outline_deep_dive`。

- 点节点先打开 inspector，展示局部页范围和一次调用粗费用；用户点「生成局部图」才入队。
- 命中 `overviewRevision + nodeId` 缓存则直接打开，不请求。
- 输入：复用 PDF 根 + **局部图集** + 冻结的 Overview 节点文本 + 一跳 narrative 邻居 takeaway。
- 局部图集：该节点证据所在页 ∪ 一跳 narrative 邻居证据所在页（**不再 ±1 页**），取这些页上的全部瘦目录行。超出此白名单的 ID 丢弃。
- 一次结构化调用产出局部单元 + 图。最多一次 repair。失败保留空态或旧卡，不动总图 head。
- 成功只移动该节点的 Deep dive head。

没有用户点「生成局部图」，不准预生成。

### 6.3 删除与重生成

- 已发布总图 / 局部图工具条提供 **重生成**、**删除**。删除必须二次确认，写明不可恢复。
- 总图重生成：立即入队新的 `outline_overview`；生成期间继续显示旧图 + 进度横幅。成功后切 head，并物理删除上一版 Overview revision（CASCADE 旧局部图）。
- 总图删除：摘掉头指针，删除该 Document Revision 下全部 Outline 修订，取消该论文所有在途局部图 Job，回到计划卡。
- 局部图重生成 / 删除只动当前节点。成功后删除该节点上一份局部图修订。
- 生成中两按钮禁用。取消只走任务中心。在途再 `start_*` 报错，不静默合并、不排队第二条。

### 6.4 提示词目录

全部生产系统提示词可在 Settings → Prompts 编辑。存在应用账号 `prompt-settings.json`，不按论文、不随工作区重置。每槽：保存 / 恢复上一份 / 恢复默认。空稿或翻译 / Lens 生成·repair 缺 `{output_language}` 拒绝保存。入队时把本趟全文写入 Job payload；执行和 repair 只读这份。抽单元、构图、局部图出厂稿要求：有并行就分叉汇合，不禁止单链；narrative 边带中文关系短标签。画布默认画出细线 `label`。工作区 `schema_meta.outline_epoch < 3` 时一次性清空全部 Outline 修订 / 头指针 / 计划缓存 / Outline Job，并把三份 Outline 提示词强制回出厂稿。不自动入队。代码不再读 v1/v2 图。

## 7. 版本与过期

依赖快照冻结：`documentRevision + ocrRevision + paperModel + protocolVersion`。Deep dive 额外冻结 `overviewRevision + nodeId`。

- 新 Overview 成功发布后才成为 head。失败的重生成不覆盖当前 head。
- 重生成或删除成功后，旧修订从库里删掉，不做版本浏览。OCR 更新后仍显示「基于旧 OCR」，只读可打开。
- 当前地图上未生成的节点保持空，等用户再点。
- 不做「把旧边自动接到新 Block」。

删除 OCR：级联删除依赖该 OCR 的 Outline revision（与翻译 / 解释 / Lens 相同的 OCR 依赖族）。PDF、Chat、Brief、usage 保留。删除预览必须列出 Outline。

删除 Paper：随 30 天回收站走，停止 Outline Job。

## 8. 模块与存储

独立深模块 `OutlineModule`。调度复用 `JobModule`。UI 只经 `DesktopClient` 的 `open / command / watch`，不直接 SQL。

建议表（名称可在实现时微调，语义不可少）：

| 表 | 权威内容 |
| --- | --- |
| `outline_revisions` | 不可变 revision：`overview` 或 `deep_dive`；`units_json` / `graph_json` / `coverage_json` / `catalog_digest` / 依赖快照 / protocol / status |
| `outline_heads` | 当前 Document Revision 的 Overview head |
| `outline_deep_dive_heads` | `(overview_revision_id, node_id)` → Deep dive revision |
| `outline_plans` | 未花费的计划 / 授权回执（可选与 Job payload 合并，但确认前不得入队付费 Unit） |

不要只加 `artifacts.kind = outline` 一个 blob 来同时表达总图 head 和按节点缓存。

Job `artifact_key` / `dedupe_key`：

- Overview：`outline-overview:{revisionId}:{ocrRevisionId}:{protocol}`
- Deep dive：`outline-deep-dive:{overviewRevisionId}:{nodeId}`

同 key 的重复请求合并。

## 9. UI 合同

### 9.1 壳：有限预设，不是窗口管理器

表面：`pdf` / `discussion` / `artifacts` / `outline`。Author Research 只留槽位，V1 不实现。

V1 预设（按论文写入 `reading_states`，不要只放 `localStorage`）：

| 预设 | 布局 | 何时 |
| --- | --- | --- |
| `pdf_discussion` | PDF \| 讨论 | 打开论文的默认（现状） |
| `pdf_outline` | PDF \| 地图 | 点顶岛「地图」的默认 |
| `outline_only` | 仅地图 | 用户再选，不是默认 |

同一期交付两层地图；**不做**任意拖拽分栏、三栏磁贴、同一表面开两份。讨论 / 成果以后再收进同一套表面系统。

### 9.2 入口

顶岛「地图」与「速览 Brief」并列。点下去切到 `pdf_outline`。

- 无已发布 OCR：地图栏「先 OCR」。
- 无论文根：走现有初始化，再回到计划卡。
- 无 Overview head：计划卡（模型、OCR revision、两次调用、粗费用）。
- 生成中：阶段卡片（抽单元 / 构图 / repair）。禁止画临时假图。
- `partial`：可读单元列表 + 覆盖率 / 失败原因。
- `published`：React Flow 画布。
- 成果索引可有一行 Outline，打开同一表面，不另做第二套详情。

### 9.3 画布

- `@xyflow/react` + `dagre`，`rankdir: TB`，只让 `narrative` 参与分层；加大同层间距。
- 卡片只画小号阅读角标、`roleLabel`、两行标题。takeaway 只在右侧详情。
- 节点可拖；坐标只留在这次打开。Fit / Reset / 换图 / 重生成回到 dagre。边不能手连。不要 MiniMap。
- 边标签是细线 + 小字，不要实心色块底。
- `cross_link` 默认隐藏，inspector 提供过滤器。
- 未选中时详情栏收起。选中后打开，分割条可拖 200–420px，宽度按论文记在 `reading_states.outline_inspector_width`。
- lazy chunk，不进首屏。
- 本地 ErrorBoundary：布局失败只退回可读列表，不准空白整个 Reader。
- Deep dive 替换地图栏中的图，保留面包屑回 Overview。PDF 仍在（除非用户选了 `outline_only`）。

### 9.4 单击与 Esc

- 单击节点：选中并打开详情。再点已选中节点或点「收起详情」则清空选中。点选时不自动跳 PDF。
- 详情里的「跳到原文」和证据药丸才跳 Block。
- inspector：「生成局部图」或「打开局部图」。图/表证据可另开已有 Lens，Deep dive 不自动跑 Lens。
- Esc：弹层 → 取消节点选中 → （已有分层）回文献大厅。一次 Esc 不能既清节点又踢回大厅。

## 10. 授权与模型

- 使用当前**论文模型**，V1 不单独设置 Outline 模型。
- 输出语言与现有成果一致（当前路径 `zh-CN`）。
- Overview：显式计划卡。
- Deep dive：inspector 内二次确认。
- 入口附近展示模型；完成后写 usage receipt。未知字段保持 `null`，不得写成 `0`。
- 高频 Chat / 翻译 / 解释 / Lens 的「点击即授权」不适用于 Overview。

## 11. 实现与验收切分

产品第一期**必须含两层**。对内顺序可验收：

1. Overview 真闭环可合入、可演示（计划卡、两 Unit、画布、跳 PDF、旧版只读）。
2. 同一里程碑立刻接 Deep dive（局部白名单、一次调用、第二张图、面包屑）。

没有总图 head 就不能生成 Deep dive。禁止用精读卡冒充第二层。

验收至少覆盖：

- 无 OCR / 无论文根 / 超窗口计划失败，零付费请求。
- 抽单元成功、构图失败 → partial 列表，head 不是假图；再次确认从构图恢复且不重抽。
- 目录外 ID 被丢弃；图 Block 无 caption 仍可被合法引用（仅定位行）。
- 详情「跳到原文」跳主证据；Deep dive 未点生成按钮不入队。
- 重 OCR 后旧图只读，新 head 为空，旧 Deep dive 不出现在新图上。
- 画布抛错时 Reader 不空白。

## 12. 代码入口（落地时）

| 事 | 先看 |
| --- | --- |
| 表 / 迁移 | `src-tauri/src/v2_workspace.rs`，新 `outline_module.rs` |
| 入队 / worker | `src-tauri/src/lib.rs` `run_job_worker`；新 kind 必须注册 |
| Gemini body / schema | `src-tauri/src/provider_ports.rs` `gemini_response_format` |
| 前端唯一 IPC | `src/desktopClient.ts` |
| 壳预设 | `src/App.tsx` 阅读工作台；不要第三永久右栏 |
| 画布 | 新 `src/outline/` lazy chunk |

改 Rust 命令后必须重启 `tauri dev`。
