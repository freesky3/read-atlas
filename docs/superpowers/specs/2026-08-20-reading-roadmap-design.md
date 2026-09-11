# Reading Roadmap（精读路线）设计规格

> 2026-09-09：本页保留历史设计。当前目标、完整提示词、字段和来源根接线以 [导师式精读路线生成合同](../../reading-roadmap-generation.md) 为准；原从零掌握、固定图表／电梯稿、第三遍仅按需、根调用等描述已调整。

> 状态：approved（2026-08-20 产品授权）
> 范围：Read Desktop 论文精读引导系统。
> 参考：V2 产品规格、Full Outline V1 合同、Orientation Pack 数据合同。

## 1. 产品身份

| 表面 | 职责 | 方向 |
|------|------|------|
| Brief | 一句话贡献与 Orientation Pack | 客观整理 → 提供结果 |
| Outline（论证地图）| 论证单元如何咬合、证据在哪 | 客观刻画论文逻辑结构 |
| Discussion | 开放追问 | 用户主导、自由问答 |
| Lens | 单个公式/图/表的深度解读 | 按需、逐对象 |
| **Reading Roadmap（精读路线）** | **从零到能转述/能质疑的行动路径** | **教练引导 → 激发思考、内化知识** |

### 1.1 核心哲学

将学术界经典的 **Three-Pass Approach（三遍阅读法）**与 AI 生成的**论文特异性提示**结合，把通用方法论变成**针对这一篇论文的、可勾选的行动清单**。

核心不是再画一遍章节结构，而是：让读者每一步都知道「现在该看哪、该想什么、看完必须能说出什么、什么时候可以停」。

**AI 是学术教练，不是代读者。** 不提供结论，而是提出指向性问题，引导读者主动发现和思考。

### 1.2 与 Outline 地图的明确分工

- **Outline 地图**：客观回答「这篇论文的论证结构是什么」——节点是论证单元，边是逻辑依赖。是论文的静态 X 光片。
- **Reading Roadmap 精读路线**：主观引导「读者应该怎么一步步搞懂这篇论文」——任务是可执行动作，附带时间预算、完成标准和自检问题。是读者的个人健身计划。

两者互不依赖、互不替代。

## 2. 明确不做（V1）

- 不做多轮生成（先出 Pass 0+1、再个性化生成 Pass 2/3）。V1 一次性生成 Pass 0–3。
- 不收集读者背景和阅读目标。V1 默认预设「刚接触该领域、具有基本数理知识的大三学生、目标是快速了解」。高级用户通过 Settings → Prompts 自定义提示词。
- 不做强制自检交互（答题、写一句话总结等）。纯勾选模式。
- 不替换右侧面板。精读路线是左侧浮动面板，讨论/成果完全不受影响。
- 不依赖 Outline 地图。两者可独立生成、独立使用。
- 不依赖 OCR。和 Brief 一样从原生 PDF 生成（如有 OCR 则利用 Block ID 锚定证据药丸）。
- 不做跨论文的全局精读计划。精读路线绑定单篇论文。
- 不做论文间路线对比或聚合。
- 不做计时器或番茄钟。时间预算只是参考文字。

## 3. 前置依赖

生成 Reading Roadmap 之前必须满足：

1. 当前 Document Revision 有**已发布的 Orientation Pack**（Brief / TerminologyGlossary / SymbolTable / Metadata）。如果还没有，走与首次 Chat / Lens 相同的初始化确认流程，成功后自动继续 Roadmap 生成。两张确认卡不合并。
2. 当前论文模型支持原生 PDF。不支持则拦住，不静默截断。
3. 存在或按现有流程初始化论文根。Roadmap 从论文根**旁支**，复用 Files URI（`fileReuse`），不重复上传 PDF。

**可选增强**：如果当前 Document Revision 已有 OCR revision，生成时附带 OCR 目录（Block ID、页码、类型、bbox），使 AI 输出的证据引用能精确锚定到 OCR Block，支持跳转高亮。无 OCR 时退化为纯页码引用。

## 4. 清单骨架（Pass 0 + Three-Pass）

### 4.1 Pass 0 | 前置对齐（10–20 分钟，可选但强烈建议）

**目标**：让读者不在正文里迷路。

AI 应生成：

- 这篇论文默认的 3–7 个前置概念（复用 Orientation Pack 术语表条目，各用 2–3 句话解释，不要长文）
- 「如果你完全没接触过 X，先用 10 分钟看 Y」的最小补课路径
- 这篇论文在解决什么**具体痛点**（用一句人话，不要复述摘要）
- 这篇论文**不是**在做什么（防止读者带着错误期待往下读）

**完成标准**：能用自己的话回答「这篇东西大概在改什么问题」。

### 4.2 Pass 1 | 鸟瞰 + 决策（5–15 分钟）

**目标**：严格按 Keshav：标题、摘要、引言、各级标题、结论、扫一眼参考文献。不要读正文细节。

AI 生成**论文特异化 5C**：

- **Category**：这是系统设计 / 理论证明 / 实证 / 测量 / 综述 / 攻击-防御 中的哪一类？
- **Context**：它主要站在哪 2–3 篇工作的肩膀上？和最近的主流路线差在哪？
- **Correctness**：一眼看上去，最关键的假设是什么？这个假设在什么场景下会崩？
- **Contributions**：作者声称的贡献有几条？哪一条才是真正的「卖点」？
- **Clarity**：结构清不清晰？有没有「贡献写得很满、证据位置却很虚」的味道？

额外加两个决策任务：

- 值不值得进入 Pass 2？
- 如果只准记住一句话，那句话是什么？

**完成标准**：能在 30 秒内向别人介绍这篇论文，并给出「继续 / 停」的理由。

**任务数量上限**：不超过 8 项。

### 4.3 Pass 2 | 抓住内容，不淹死在细节（30–90 分钟）

**目标**：这一遍必须「对着图和主张走」，而不是按页码走。

AI 生成这类任务：

- 按顺序看指定图表：Figure X 的横纵轴是什么、比较的是谁、最反直觉的点在哪
- 用自己的话重述核心方法的 4–7 个步骤（先不碰证明/复杂推导）
- 找出「主张 → 证据」的对应：Claim A 靠 Table 2，Claim B 靠 Figure 4
- 标出 3 个暂时不懂但可以先跳过的技术点
- 标出 2–5 篇「不读就很难懂这篇」的引用
- 写一段 150–250 字的转述：问题、方法、关键结果、限制

**完成标准**：能不看论文，把主线讲给同领域同学听；能指出最强证据和最弱证据。

**任务数量上限**：不超过 12 项。

### 4.4 Pass 3 | 虚拟复现 + 批判（按需，2–6 小时）

**目标**：只有「要复现、要审稿、要在这上面做研究」才需要。

AI 应把「virtually re-implement」拆成可执行动作：

- 列出作者没写死、但复现时必须猜的实现细节
- 指定 1–2 个关键公式/算法：先自己推/自己写伪代码，再对照原文
- 挑战每一个关键假设：如果把假设改成相反的，结论还成不成立
- 指出实验里最容易「看起来很好、其实不可比」的地方
- 产出三份东西：一页复现清单、一页缺陷清单、一页可延展问题

**完成标准**：能解释「如果我来做，会在哪一步做出不同选择」。

### 4.5 全局尾声

清单末尾附加：
- 一个「30 秒电梯稿」模板
- 「如果只读一张图，该读哪张、为什么」

## 5. 任务项数据合同

### 5.1 单个任务项

```typescript
interface RoadmapTask {
  id: string;                      // 稳定 ID，用于勾选状态持久化
  text: string;                    // 任务描述（Markdown，含证据药丸）
  timeMinutes: number;             // 建议用时（分钟）
  required: boolean;               // 必做 vs 选做
  completionCriteria: string;      // 完成标准（纯文字）
  selfCheckQuestions: string[];    // 「问自己」问题列表（1–3 个）
  evidence: EvidencePill[];        // 引用的具体论文对象
}

interface EvidencePill {
  label: string;                   // 显示文本，如 "Figure 3" / "Algorithm 1" / "Table 2"
  page: number;                    // PDF 页码
  blockId?: string;                // OCR Block ID（如有 OCR）
  bbox?: [number, number, number, number]; // 归一化 bbox（如有 OCR）
}
```

### 5.2 Pass 结构

```typescript
interface RoadmapPass {
  passNumber: 0 | 1 | 2 | 3;
  title: string;                   // 如 "前置对齐" / "鸟瞰 + 决策"
  subtitle: string;                // 如 "让读者不在正文里迷路"
  timeBudget: string;              // 如 "10–20 分钟"
  exitCriteria: string;            // 该 Pass 的整体退出条件
  tasks: RoadmapTask[];
}
```

### 5.3 完整 Roadmap

```typescript
interface ReadingRoadmap {
  version: number;                 // Schema 版本
  generatedAt: string;             // ISO 8601
  paperTitle: string;              // 论文标题（冗余，方便显示）
  loadPointExtraction: LoadPointExtraction; // 内部负荷点抽取（不展示给用户）
  passes: RoadmapPass[];           // Pass 0–3
  elevatorPitch: string;           // 30 秒电梯稿模板
  oneChart: {                      // 如果只读一张图
    label: string;
    page: number;
    blockId?: string;
    reason: string;
  };
}

interface LoadPointExtraction {
  coreClaims: string[];            // 核心主张（≤3 条）
  novelStep: string;               // 方法里真正新的那一步
  keyFigures: string[];            // 最重要的 2–4 张图/表
  fragileAssumption: string;       // 最脆弱的假设
  confusingTerms: string[];        // 最容易误读的术语
  readerBlockPredictions: string[];// 读者卡点预测
  minReproPath: string;            // 最小可复现路径
}
```

## 6. 存储与持久化

### 6.1 Artifact 层

Reading Roadmap 作为新的 Artifact Kind 存储：

```
artifact_kind = "reading_roadmap"
object_key = "roadmap"          // 每篇论文每个 revision 只有一份
```

遵循现有 Artifact 合同：
- 绑定 `document_revision`
- 原子发布（生成完整后才可见）
- 重生成时新 revision 覆盖旧 head，旧版本保留在历史
- 依赖快照冻结 Orientation Pack 版本、论文根、provider + model

### 6.2 勾选进度层

独立于 Artifact 内容，存储在 `roadmap_progress` 表：

```sql
CREATE TABLE roadmap_progress (
  paper_id    TEXT NOT NULL,
  roadmap_id  TEXT NOT NULL,       -- artifact ID
  task_id     TEXT NOT NULL,       -- 任务项 stable ID
  completed   INTEGER NOT NULL DEFAULT 0,
  completed_at TEXT,
  PRIMARY KEY (paper_id, roadmap_id, task_id),
  FOREIGN KEY (paper_id) REFERENCES papers(id),
  FOREIGN KEY (roadmap_id) REFERENCES artifacts(id)
);
```

- 勾选/取消勾选即时写入
- Roadmap 重生成后，task ID 变化，旧进度自动失效（新 roadmap_id），不迁移
- 切换 Document Revision 后，旧 Roadmap 进度只读保留

## 7. UI 与交互

### 7.1 入口

**顶岛按钮**：工作台顶部栏新增「📋 精读」按钮，位于「🗺️ 地图」旁边。

- 仅在 Reader 视图（`viewMode === "reader"`）显示
- 按钮状态：
  - 未生成时：正常态，点击后展示「生成精读路线」确认卡
  - 生成中：显示转圈/进度指示
  - 已生成时：显示完成进度徽标（如 `12/28`），点击切换面板开关

### 7.2 浮动面板

**方向**：从左侧滑出，覆盖 PDF 画布左半部分（约 40–50% 屏幕宽度）。

**视觉**：液态玻璃半透明面板（`backdrop-filter: blur(24px) saturate(180%)`），与应用整体 Liquid Glass 设计语言一致。

**行为**：
- 点击「精读」按钮 → 面板滑入
- 再次点击「精读」按钮 → 面板滑出收起
- 右侧讨论/成果区**完全不受影响**
- 面板内独立滚动
- 面板打开时 PDF 仍然可以接收跳转指令（点击证据药丸时 PDF 跳页并高亮）

**面板状态机**：

```
[closed] ──点击「精读」──→ [open]
   ↑                        │
   └───点击「精读」/Esc───────┘

面板内容:
  [未生成] → 显示生成确认卡（费用估算 + 「生成精读路线」按钮）
  [生成中] → 显示 Job 进度条 + 阶段指示
  [已生成] → 显示完整 Pass 0–3 清单
  [生成失败] → 显示错误信息 + 重试按钮
```

### 7.3 面板内部布局

```
┌─────────────────────────────────┐
│ 📋 精读路线  Reading Roadmap     │  ← 面板标题
│ 12/28 任务完成 · Pass 2 进行中    │  ← 全局进度摘要
├─────────────────────────────────┤
│                                 │
│ ▼ Pass 0 · 前置对齐  ██████░░  │  ← 可折叠 Pass 区块
│   10–20 分钟                    │
│                                 │
│   ☑ 了解 X 概念 (3 min)        │  ← 已完成任务
│     完成标准：能用自己的话...    │
│     问自己：为什么...           │
│                                 │
│   ☐ 理解论文的核心痛点 (5 min)  │  ← 未完成任务
│     完成标准：...               │
│     问自己：...                 │
│     📄 p.2 · Abstract           │  ← 证据药丸（可点击跳转）
│                                 │
│ ▼ Pass 1 · 鸟瞰 + 决策  ░░░░░  │
│   5–15 分钟                     │
│   ...                           │
│                                 │
│ ▶ Pass 2 · 抓住内容     ░░░░░  │  ← 收起态
│ ▶ Pass 3 · 虚拟复现     ░░░░░  │
│                                 │
├─────────────────────────────────┤
│ 🎤 30秒电梯稿 · 📊 只读一张图   │  ← 底部快捷卡片
└─────────────────────────────────┘
```

### 7.4 证据药丸交互

任务描述中引用的 Figure/Table/Algorithm/公式渲染为可点击的液态玻璃微型药丸：

```
[📄 p.7 · Figure 3]  [📄 p.12 · Algorithm 1]  [📄 p.15 · Table 2]
```

- 点击后 PDF 阅读器平滑滚动到目标页
- 如有 OCR Block ID，高亮对应 Block 边框（复用现有 `focusedBlockId` 机制）
- 不需要关闭精读面板即可看到 PDF 跳转结果（PDF 在面板右侧仍然部分可见，且面板半透明）

## 8. AI 生成流水线

### 8.1 Prompt 槽位

新增 `PromptSlotId::ReadingRoadmap`（第 17 个生产提示词槽位），在 Settings → Prompts 中可编辑。

默认提示词核心指令：

```
你是论文精读教练，不是摘要机器人。

任务：为下面这篇论文生成一份「从零掌握」的 Todo-list。
要求：
1. 以 Keshav 的 Three-Pass Approach 为骨架，增加 Pass 0（前置对齐）。
2. 不要写成章节大纲。每个任务必须是可执行动作，并绑定这篇论文的具体内容（图号、表号、算法、假设、符号、关键实验）。
3. 面向读者从零开始。先判断这篇论文默认的背景知识，给出最小补课清单。
4. 每一遍都包含：时间预算、必做任务、完成标准、停止/继续决策。
5. 每个任务后附 1–3 个「读完必须能回答」的问题。问题要针对这篇论文，不要泛泛而谈。
6. Pass 1 不超过 8 项，Pass 2 不超过 12 项。
7. 区分「必做」和「选做」。
8. 语言像教练布置作业，不像在写综述。
9. 「图表优先」：第二遍的核心任务围绕 2–3 张最关键的图表展开，问读者「这张图如果少了误差线/少了某个 baseline，你还会信吗」。
10. 先输出论文负荷点抽取（核心主张、新方法步骤、关键图表、脆弱假设、读者卡点预测），再基于抽取生成 Todo-list。不要让任务和抽取互相矛盾。
11. 最后给一个 30 秒电梯稿模板，以及「如果只读一张图，该读哪张、为什么」。
12. 每个 Pass 都给退出条件（硬性停止点）。

默认读者背景：刚接触该领域、具有基本数理知识的大三学生，目标是快速了解。
```

### 8.2 Job 定义

```
job_kind = "reading_roadmap"
dedupeKey = "roadmap:{revisionId}:{provider}:{model}"
```

- 单阶段 Job（无 checkpoint 切分）
- 从论文根旁支调用，复用 Files URI
- 输入附带 Orientation Pack 的 Brief、术语表、符号表（JSON 格式）
- 如有 OCR，附带瘦 OCR 目录（与 Outline 相同格式）
- 严格 JSON Schema 输出（`response_format`）

### 8.3 输出后处理

- 解析 JSON → `ReadingRoadmap` 结构体
- 校验 Pass 数量（必须恰好 4 个：Pass 0–3）
- 校验每个任务项有 `id`、`text`、`completionCriteria`、`selfCheckQuestions`
- 校验证据引用的页码在论文页数范围内
- 如有 OCR，校验 Block ID 存在于 OCR 目录中；不存在的退化为纯页码引用
- 通过 `preprocessLaTeX` 处理所有文本字段中的数学公式
- 原子发布为 `artifact_kind = "reading_roadmap"`

## 9. 文件清单

### 9.1 新增文件

| 文件 | 职责 |
|------|------|
| `src/ReadingRoadmap.tsx` | 浮动面板组件：面板框架、Pass 折叠区块、任务项渲染、证据药丸、勾选交互 |
| `src/ReadingRoadmap.test.tsx` | 前端单元测试 |
| `src/readingRoadmapTypes.ts` | `ReadingRoadmap`、`RoadmapPass`、`RoadmapTask`、`EvidencePill` 类型定义 |
| `src-tauri/src/roadmap_module.rs` | Rust 后端：`roadmap_progress` 表、勾选 CRUD、Roadmap 生成 Job 处理 |

### 9.2 修改文件

| 文件 | 变更 |
|------|------|
| `src/App.tsx` | 新增 `roadmapOpen` state、顶岛「精读」按钮、面板 mount |
| `src/types.ts` | 新增 `ReadingRoadmapProjection`、`RoadmapProgressEntry` 类型 |
| `src/styles.css` | 浮动面板样式（`.reading-roadmap-overlay`、`.roadmap-task`、`.evidence-pill-roadmap`） |
| `src-tauri/src/lib.rs` | 注册新命令：`start_roadmap_job`、`get_roadmap`、`toggle_roadmap_task`、`list_roadmap_progress` |
| `src-tauri/src/prompt_settings.rs` | 新增 `ReadingRoadmap` 提示词槽位 |
| `src-tauri/src/provider_ports.rs` | Roadmap 生成的论文根旁支调用与 JSON Schema |
| `src/desktopClient.ts` | 新增 IPC 命令类型 |

## 10. 验证计划

### 10.1 自动化测试

```powershell
npx vitest run --maxWorkers=1 --fileParallelism=false
npm run build
cd src-tauri
cargo test --locked
```

覆盖：
- 面板打开/关闭状态切换
- 任务勾选/取消勾选持久化
- 证据药丸点击触发 PDF 跳转
- Pass 折叠/展开
- 进度计算正确性
- Roadmap JSON Schema 校验
- 提示词槽位注册

### 10.2 手动验证

- 用一篇真实论文端到端生成 Reading Roadmap
- 验证 Pass 0 的前置概念是否复用了术语表
- 验证证据药丸跳转到正确页面和 Block
- 验证面板打开时右侧讨论区可正常使用
- 验证三套主题下面板视觉一致性
- 验证未 OCR 时退化为纯页码引用
- 验证 Roadmap 重生成后旧进度自动失效

## 11. 已确认的产品决策记录

| # | 决策 | 选项 | 理由 |
|---|------|------|------|
| D-R01 | UI 入口 | 顶岛「精读」按钮 | 与「地图」并列，一级入口 |
| D-R02 | 面板形式 | 左侧浮动面板覆盖 PDF | 讨论区不受影响，可同时使用 |
| D-R03 | 生成策略 | 一次性生成 Pass 0–3 | 简单直接，V1 不做多轮生成 |
| D-R04 | 读者背景 | 不收集，默认预设 | 零交互摩擦，提示词可自定义 |
| D-R05 | 完成交互 | 纯勾选模式 | 不强制自检，完成标准和「问自己」已是文字引导 |
| D-R06 | 证据穿透 | 点击跳转 PDF + 高亮 Block | 复用现有 `[p.N]` 机制 |
| D-R07 | 成果关系 | 引用 Orientation Pack，不依赖 Outline | Pass 0 复用术语表，两者独立 |
| D-R08 | 数据模型 | 新 Artifact Kind + 独立进度表 | 遵循现有 Artifact 合同 |
