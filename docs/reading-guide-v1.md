# Reading Guide V1（桌面端 AI 好友批注）

状态：historical（2026-08-20 产品授权，D-040；角色冻结已由 D-068 取代）
范围：Read Desktop **V1** 预计算三人英文页边批注。现行中文角色库与 V2 生成见 [reading-guide-generation.md](reading-guide-generation.md) 与 [guide-characters.md](guide-characters.md)。
参考：相邻 `read_addon` 的 Reading Guide V2 生成思想（两级流水线、可定位锚点、部分发表）。不迁入 Dexie、MV3 runtime、独立 Block View 窗口或句子级文字层。

本文是旁批的产品、数据、生成和 UI 合同。与本文冲突时，先改 ADR / 本文。现有 `reading_roadmap`（Keshav 三遍 Todo）是另一套成果，不要复用它的 slot、Job kind 或 UI。

## 1. 产品身份

几个固定性格的好友已经读完这篇论文，把有限、带态度的英文笔记写在 PDF 页边。这是旧书，不是实时陪读，不是笔记系统，不是 Semantic View。

| 表面 | 任务 |
| --- | --- |
| Brief | 整篇定向 |
| Discussion | 开放追问 |
| Lens | 单个公式 / 图 / 表 |
| Outline | 论证地图 |
| **Reading Guide** | 预计算的多角色页边批注 |

- 旁批是层，不是第三工作台。壳仍只有 `pdf_discussion` / `pdf_outline` / `outline_only`。
- 讨论栏与批注总开关独立。引用 / 翻译 / 解释 / Lens 在有批注的 Block 上照常可用。
- 锚在已发布 OCR Block 整框。不做句子级高亮，不用 PDF.js 文字层。

## 2. 明确不做（V1）

- 独立 `blockview.html` 窗口、Dexie / MV3。
- 句子级高亮、常驻蜘蛛网引线、每块铅笔开关。
- 批注内聊天、点批注入讨论、浅/深阅读模式。
- 请某人离场、改人设、调话量、跨篇记忆。
- 类型摊派、人工笔记、真人协作、插件数据导入。
- 打开论文或 OCR 完成后自动生成。
- 生成完成抢焦点或强制打开层（首次发表除外）。

## 3. 人格（代码冻结）

| id | 显示名 | 色 | 气质 |
| --- | --- | --- | --- |
| `alin` | 阿林 | `#2458a8` | 把难处讲清楚；难处可写短段 |
| `laozhou` | 老周 | `#b42318` | 话少锋利；几乎不超过两句 |
| `xiaxia` | 小夏 | `#9a6700` | 类比 / 直觉；中等长度 |

第一版不可编辑。性格决定开口；一处一个主讲人；其他人可以只留色笔。

## 4. 墨水

四种墨水：`trace`（色笔）、`note`（`line` 一句或 `short` 短段）、`reply`（`@` 互评）。没有 `related` 跳转芯片。跨节提醒写在正文里。

硬门闩（发布前）：

- `anchor.blockId` 必须落在当前已发布 OCR 上，页码一致，bbox IoU ≥ 0.5。对不上则丢掉，计入 `dropped`。
- 同一 Block 至多一条 `note`。其他人只许 `trace` 或对该 note 的 `reply`。
- `reply.parentId` 必须指向 `note`；一簇最多两轮。超出丢掉。
- 全书 0 条可定位墨水 → Job 失败，旧 head 保留。仅有合法 traces 可以发表。

密度气质（约 10 页会议论文）只进提示词，不作为 schema 失败条件。

## 5. 阅读面

- 方案 B：有字的卡片钉在该页 PDF 右侧。层开启时每一页预留同一宽度页边槽（240px），安静页可以空着。
- 静息：角色色条 + 按 y 排卡片。悬停或选中才出角色色引线。纯痕迹不拉线。
- 有批注的 Block 选中色从薄荷绿换成浅红。工具条 DOM 不改。
- 总开关关掉：痕迹和卡片都藏、页边槽收掉，不自动打开讨论。
- 第一版卡片上没有「问」。

## 6. 入口与 Job

顶岛「旁批」不新开工作台：

- 无 OCR → 拦住，先 OCR。
- 未生成 → 计划确认后入队。不强制先有地图；有地图则只读借用。
- 生成中 → 忙碌态，不重复入队。
- 已发表 → 主点击开关层。菜单：显示 / 重新生成 / 删除。
- 重生成成功前旧书仍可看。删除二次确认、不可恢复。

Job `kind = reading_guide`。一次压缩阅读上下文（论文根 PDF；可复用 Outline Overview 的页码/takeaway，不当章节目录）→ 6 页目标 / 8 页硬顶分批，三人写在同一 JSON。分批不再传整本 PDF。锚点对不上就丢，不猜页首块。允许 partial。

## 7. 持久化

独立 `reading_guide_revisions` / `reading_guide_heads` / `reading_guide_plans`，不进通用 `artifacts`。中间 batch 只进 `job_checkpoints`。`reading_states.guide_layer_visible` 持久化层开关。

协议：`reading-guide-desktop-v1`。SQLite schema 4。OCR 换版 → `stale`，不自动重跑。

## 8. IPC

| 命令 | 作用 |
| --- | --- |
| `get_reading_guide` | 投影 |
| `plan_reading_guide` | 本地估价 |
| `start_reading_guide` | 入队 |
| `delete_reading_guide` | 硬删 |

层开关走 `save_reading_state`，不是独立命令。结构体参数包一层 `{ request }`（D-019）。浏览器 memory adapter 不得假装生成成功。

## 9. 文件入口

| 要动的事 | 先看 |
| --- | --- |
| 合同 | 本文、D-040 |
| 存储 / 投影 | `guide_module.rs` |
| 协议 / 校验 | `guide_protocol.rs` / `guide_validate.rs` |
| 切批 | `guide_batching.rs` |
| 入队 / 删除 | `guide_commands.rs`；`#[tauri::command]` 留在 `lib.rs` |
| 页边 layout | `src/guide/marginLayout.ts` |
| 顶岛 | `src/guide/GuideIsland.tsx` |
| PDF 叠层 | `PdfReader.tsx` |

生成管线、页边 UI 与顶岛已按本合同落地。本文件在与代码冲突时仍优先；先改 ADR / 本文。

## 10. 实现踩坑（2026-08-22 现场，下一个 agent 必读）

权威排障入口也写在 [agent-onboarding.md](agent-onboarding.md) **§6（虚拟列表）** 和 **§30（旁批）**。这里只记合同落地时已经付出过代价的约束。

### 10.1 生成

- 上下文那一轮可以给模型看整本 PDF。**每一批旁批禁止再上传 PDF**，走 `interact_text`。否则长文后几批传输失败，任务停在 annotating，usage 不涨（provider 在发 HTTP 前已 committed）。
- Catalog 必须带 block excerpt，并限制每页块数。没有 excerpt 时模型会输出一篇总评，45 页只剩 1 条旁批——看起来像提示词太稀，其实是目录太瘦。
- 提示词变 denser 时用 `GUIDE_PROMPT_GENERATION` 擦旁批两槽。**禁止**加 `PROMPT_SETTINGS_SCHEMA`（会冲掉用户改过的另外 17 个槽）。
- 墨水存盘：`GuideInk::to_value()` 写成 `{ kind, speakerId, anchor, body }`。不要 `serde_json::to_value(&enum)` 的外部标签。前端 `src/guide/inks.ts` 仍要兼容缺 kind、snake_case、`{ Note: {...} }`、page 0（用 OCR block 补页码）。

### 10.2 阅读器叠层

- 槽高度 = 画布高度，溢出在槽内滚。卡片把 row 撑高会和虚拟列表 stride 打架，表现为页重叠、旁批画到下一页上。
- 引线 SVG 钉在页行上、不随槽滚动。`y2` 必须用卡片 `top - gutter.scrollTop`（`guideLeaderLine`），否则槽内一滑就和卡片错位。
- 顶岛是 `[✎ 旁批 · N][▾]`，不要把「菜单」二字画进 caret。主按钮的数字来自 `guideNotes(normalizeGuideInks(head.inks))`。没有数字且状态是「旁批已就绪」= 前端一个 note 都没解析出来。
- 生成完成只按新 `head.id` 跳一次第一条可定位墨水。不要每个 `paper` 事件都 `setPage`。

### 10.3 和 PDF 虚拟列表的耦合

旁批层会改变页行宽度（+240+16）。虚拟列表必须按**画布高度**定位，不能按 gutter 伸长后的 row。详见 onboarding §6。现场三次回归：

| 现象 | 根因 | 现行修复 |
| --- | --- | --- |
| 任务完成、阅读区全黑、底栏仍 `Pages 31–35 kept active` | spacer 把视口留在空白；480ms 内跳过 `scrollIntoView` | 绝对定位槽，滚动条位置映射页码 |
| 页与页重叠、旁批盖到下一页 | 统一 pitch 920 / 量到 min-height 220 | `onPageExtent` 累计高度 |
| 上滑一直弹回第 33 页 | `initialScrollOffset` 回传触发 drift recovery | 只在外部翻页时写 `scrollTop` |
