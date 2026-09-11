# Lens 区块卡片流与深度探讨实现手册（D-064）

- 状态：代码与自动化测试全量落地；全工程 59 个测试套件、400 项测试全部通过
- 日期：2026-09-04（本页可追加，不覆盖旧条）
- ADR：[decisions.md D-064](decisions.md)
- 索引：[README.md](README.md) · [agent-onboarding.md §49](agent-onboarding.md)
- 核心代码：
  - 卡片流容器：[`src/components/BlockCardStream.tsx`](../src/components/BlockCardStream.tsx)
  - 物理区块卡片：[`src/components/BlockCard.tsx`](../src/components/BlockCard.tsx)
  - 专属沉浸问答页：[`src/components/BlockCardQaDetail.tsx`](../src/components/BlockCardQaDetail.tsx)
  - 聚合与接线：[`src/ArtifactPanel.tsx`](../src/ArtifactPanel.tsx) · [`src/App.tsx`](../src/App.tsx)
  - 样式体系：[`src/styles.css`](../src/styles.css)
- 自动化测试：
  - 卡片流与专属页：[`src/components/BlockCardStream.test.tsx`](../src/components/BlockCardStream.test.tsx)
  - 聚合与多版本集成：[`src/ArtifactPanel.test.tsx`](../src/ArtifactPanel.test.tsx)

**本页是实现手册与踩坑日志，不是设计草稿。** 代码与本页冲突时先改本页或 ADR，再改代码。下一个 agent：先读 **§0、§1、§2、§5、§6、§8**，再动文件。

---

## 0. 现状一句话与设计哲学

彻底消除了旧版将段落翻译、解释、Lens 细碎平铺成横向超长 Chip 列表导致的视觉上下文割裂与列表冗长问题。转而采用**以 PDF 物理区块为中心（Block-Centric）的卡片流**体系：

1. **物理阅读顺序流（Physical Reading Stream）**：严格按论文的实际阅读流（`pageNumber` 升序，同页内按 `blockIndex` / 垂直位置升序）排列。
2. **多模态高保真预览（Rich Preview）**：数学公式实时 KaTeX 渲染、图表/表格展示高清截屏缩略图并支持一键 Lightbox 全屏放大、正文段落展示 Markdown 高亮与行号。
3. **两级分层导航模式（Two-Tier Architecture）**：
   - **外层卡片流 (`BlockCardStream`)**：紧凑整洁，仅保留区块初始分析结果、多维度功能 Tab（翻译/解释/Lens）与二次预检防误触机制；绝不内嵌长对话，保持滚动轻盈。
   - **内层专属问答页 (`BlockCardQaDetail`)**：点击「💬 深入探讨此区块 →」推入全屏专属研读页。顶部渲染高清图表原图并平铺全部 Lens 结论与证据链，底部固定常驻吸底输入框，专为多轮深度推演设计。

---

## 1. 核心不变量（改动任何相关代码前对照）

违反任一条即视为视觉/体验回归，不要尝试绕过：

| # | 不变量 | 违规反例（严禁出现） |
| --- | --- | --- |
| **I1** | **物理流阅读排序**：卡片流必须按 `pageNumber` 升序，同页按 `blockIndex` / 坐标升序排列。 | 按生成时间乱序排列，导致读者来回上下翻页找上下文。 |
| **I2** | **防误触预检卡片 (Pre-flight)**：点击尚未生成的 Tab 时，**绝不自动调用 API 消耗 Token**，必须呈现预检卡片，待用户确认「✨ 开始生成」后再触发。 | 切换 Tab 瞬间静默调用大模型 API。 |
| **I3** | **外层卡片高度独立性**：展开一张或多张卡片时，已展开卡片必须保持其自然内容高度，**禁止 flex 弹性收缩挤压**；父容器必须支持平滑纵向滚动查看完整内容。 | 展开第二张卡片时，第一张卡片高度被挤压收缩变形。 |
| **I4** | **外层卡片极简性**：多轮长问答历史与输入框必须收敛到专属问答页，外层卡片仅保留「💬 深入探讨此区块」入口与记录数 Badge。 | 将十几轮对话气泡直接展开在卡片流内部，把列表撑至上万像素。 |
| **I5** | **专属页自然平铺流**：专属页严禁使用生硬的「收起/展开完整分析」折叠层；图表在最上方渲染，随后自然流式铺开结论、解析、证据锚点、对话与追问建议。 | 把分析结果塞进狭小的手风琴折叠框里，用户看两行就得点一次展开。 |
| **I6** | **三级图表截屏加载链路**：只要 Artifact 是 Lens 产物（或图表/表格），截屏加载必须按优先级走：预载传参 -> 本地磁盘缓存 -> 精确 Bbox PDF Canvas 动态裁剪。**不得单凭 OCR `blockType` 判空断定非图表**。 | 当 OCR 模型把图表识别为段落时，截屏加载逻辑直接放弃，顶部空白。 |
| **I7** | **输入框固定吸底与底部留白精简**：输入框必须常驻固定在独立页面的最底端（`.block-qa-bottom-bar`），底边 padding 控制在 `8px 16px 10px` 紧凑范围，把纵向空间最大化留给阅读区。 | 输入框跟随页面滚在最底下，或者输入框底下留出上百像素大片空白。 |
| **I8** | **三行自适应与 Windows 原生控件去黑化**：输入框支持 Shift+Enter 换行；在 1~3 行以内随字数平滑增高且**强制隐藏垂直滚动条（`overflow-y: hidden`）**；4 行以上才激活极细滚动条。输入框必须无黑边、无缩放把手。 | 1 行时就露出 Windows WebKit 默认的上下滚动小箭头；输入框带有突兀黑框。 |

---

## 2. 组件分层与职责划分

```
ArtifactPanel (scopeTab === "block")
  │
  ├── Top Navigation Bar (类别筛选胶囊: 全部 / 文本 / 图表 / 公式 / 表格)
  │
  └── BlockCardStream (卡片流容器)
        │
        ├─ [视图 1: focusedQaBlockId === null] (主卡片流模式)
        │     └─ BlockCard (物理区块卡片)
        │           ├─ Rich Preview: 公式 KaTeX / 图表缩略图 / 正文行号
        │           ├─ Context Tabs: [ 翻译 ] [ 解释 ] [ Lens 深度分析 ]
        │           ├─ Pre-flight Card: 未生成功能预检与安全确认
        │           ├─ Version Switcher: 多版本切换与版本删除
        │           └─ Entry Button: 「💬 深入探讨此区块 (N) →」
        │
        └─ [视图 2: focusedQaBlockId !== null] (专属沉浸问答模式)
              └─ BlockCardQaDetail (独立专属问答页)
                    ├─ Top Header: 「← 返回区块列表」· 页码跳转 · 「带到主讨论 →」
                    ├─ Dedicated Crop Card: 高清原图 · 放大遮罩 · Lightbox 全屏弹窗
                    ├─ GeneratedDetail: 核心要点 (Takeaway) · 深度推演 · 证据锚点
                    ├─ Lens QA Stream: YOU / LENS 对话气泡 · 正在推演骨架屏
                    ├─ Suggested Chips: 「💡 追问建议」药丸按钮组
                    └─ Pinned Bottom Bar: 常驻吸底 · 3 行自适应无滚动条毛玻璃输入胶囊
```

### 核心组件职责契约

| 组件 | 文件 | 核心职责 |
|---|---|---|
| `BlockCardStream` | [`src/components/BlockCardStream.tsx`](../src/components/BlockCardStream.tsx) | 维护类型筛选状态 (`filter`)、各卡片独立展开状态 (`expandedIds`)、聚焦的问答区块 (`focusedQaBlockId`)；负责返回卡片列表时的平滑定位与高亮脉冲。 |
| `BlockCard` | [`src/components/BlockCard.tsx`](../src/components/BlockCard.tsx) | 单个物理区块的渲染沙盒。管理 Tab 状态、预检卡片渲染、版本归并显示、缩略图 Lightbox。点击深入探讨时向上触发 `onOpenQa(blockId, artifact, cropSrc)`。 |
| `BlockCardQaDetail` | [`src/components/BlockCardQaDetail.tsx`](../src/components/BlockCardQaDetail.tsx) | 单个区块的深度研读推演页。实现三级图表截屏解析、无折叠自然排版、3 行无滚动条自适应输入框与常驻吸底。 |
| `bundleArtifactsByBlock` | [`src/ArtifactPanel.tsx`](../src/ArtifactPanel.tsx) | 纯数据聚合算法。将离散在 SQLite 存储里的各种 Artifact 按空间与标识聚合成以 Block 为单位的完整 Bundle。 |

---

## 3. 数据层与聚合算法 (`bundleArtifactsByBlock`)

文档内的 Artifact 实体在底层是按 `kind` 和 `id` 单独落库的，前端通过 `bundleArtifactsByBlock` 将其聚合为 `BlockArtifactBundle`：

```ts
export type BlockArtifactBundle = {
  blockId: string;
  pageNumber: number;
  blockIndex: number;
  blockType: string;
  textContent: string;
  bbox: [number, number, number, number];
  cropDataUrl?: string;
  groupsByKind: Map<string, ArtifactGroup>;
  allArtifacts: ArtifactProjection[];
  isGenerating?: boolean;
  generatingAction?: BlockAction;
};
```

### 匹配回退梯队
1. **精确 ID 匹配**：检查 `artifact.objectKey === ocrBlock.id`，或 `artifact.evidence[].blockId === ocrBlock.id`；
2. **空间交并比 (IoU) 匹配**：若同页内两者的 Bbox `computeIou(b.bbox, artifact.bbox) > 0.4`，判定属于同一区块；
3. **证据链回退**：若 OCR 缺失，直接回退使用 `group.latestArtifact.evidence[0]` 的 `pageNumber` 与 `bbox` 构造独立区块。

---

## 4. 三级高可用图表截屏加载链路 (Crop Pipeline)

在专属问答页和卡片缩略图中，图表/表格原图截屏的加载遵循**严格的三级容灾策略**：

```
[开始加载图表截屏]
      │
      ├─ 1. 检查已载入内存缓存 (initialCropSrc / bundle.cropDataUrl / displayCropSrc)
      │      └─ 命中 → 立即展示，耗时 0ms
      │
      ├─ 2. 尝试从 Tauri 本地持久化资产读取 (desktopClient.open "get_artifact_asset_path")
      │      └─ 获取磁盘路径 → convertFileSrc(path) → 命中并展示，耗时 < 10ms
      │
      └─ 3. 动态 PDF Canvas 局部渲染 (createLensCrops)
             ├─ 页码取值：artifact.evidence[0].pageNumber ?? bundle.pageNumber
             ├─ Bbox 取值：artifact.evidence[0].bbox ?? bundle.bbox
             └─ 调用 pdfjs getPage + render 截屏 → 导出 base64 DataURL
```

> [!IMPORTANT]
> **切勿仅依赖 `bundle.blockType` 判空！**
> 在实际 OCR（如 MinerU / PaddleOCR）推断中，有些 Figure 或 Table 可能会被分类器误标为 `paragraph` 或 `text`。
> 只要 `artifact.kind.startsWith("lens_")`，或者 `normType` 包含 figure/image/table，都必须全力触发截屏获取。

---

## 5. 输入框吸底与多行自适应算法

### 5.1 吸底布局骨架
在 [`BlockCardQaDetail.tsx`](../src/components/BlockCardQaDetail.tsx) 中，页面采用垂直 flex 布局：
- `.block-qa-detail-header`: `flex-shrink: 0;`（顶部常驻栏）
- `.block-qa-detail-body`: `flex: 1 1 0; min-height: 0; overflow-y: auto;`（中段滚动阅读区）
- `.block-qa-bottom-bar`: `flex-shrink: 0; padding: 8px 16px 10px;`（底部常驻吸底栏）

### 5.2 3 行以内无滚动条高度自适应逻辑
```ts
useEffect(() => {
  const el = inputRef.current;
  if (!el) return;
  el.style.height = "auto";
  
  // 14px 字号 * 1.5 行高 = 21px/行 + 上下 12px padding
  // 1 行 ~33px, 2 行 ~54px, 3 行 ~75-80px, 4 行 ~98px+
  // 88px 为 3 行完全容纳安全门限
  const maxHeightThreeLines = 88;
  const scrollHeight = el.scrollHeight;

  if (scrollHeight <= maxHeightThreeLines) {
    el.style.height = `${Math.max(26, scrollHeight)}px`;
    el.style.overflowY = "hidden"; // 1-3 行强制无滚动条、无滚动箭头
  } else {
    el.style.height = `${maxHeightThreeLines}px`;
    el.style.overflowY = "auto";   // 超出 3 行激活极细滚动条
  }
}, [question]);
```

---

## 6. 踩坑日志与避坑指南 (Pitfall Log & Gotchas)

下一个修改本模块的开发人员或 Agent，请**逐条阅读以下踩坑实录**：

### P1: `flex-shrink: 0` 与多卡片展开高度坍塌
- **现象**：当卡片流中展开第二张或第三张卡片时，第一张卡片的高度被严重挤压缩小，用户无法看全内容。
- **原因**：外层 flex 容器没有给已展开卡片设置 `flex-shrink: 0`，导致弹性盒子自动压缩兄弟节点以试图塞入视口。
- **解法**：卡片流外层链条明确为 `height: 100%; min-height: 0; overflow-y: auto;`，卡片根元素与展开容器设置 `flex-shrink: 0;`。

### P2: Textarea 出现突兀黑框与尺寸把手
- **现象**：在 Windows 生产环境中，提问输入框带有原生黑色实线边框，右下角带有原生拉伸三角形把手。
- **原因**：`<textarea>` 标签只设置了样式名 `composer-compact-capsule`（这是外层胶囊的类），缺失了输入核心类 `composer-input-line`。
- **解法**：必须使用结构 `<div className="composer-compact-capsule"><textarea className="composer-input-line" /></div>`。`.composer-input-line` 中强制包含 `border: 0; outline: 0; background: transparent; resize: none;`。

### P3: 专属页文字「贴边拥挤」（文字非常挤）
- **现象**：进入专属问答页后，标题、分析段落和证据完全贴在屏幕左右两侧物理边缘，行高过窄，压迫感强烈。
- **原因**：容器声明为 `className="block-qa-detail-body artifact-detail"`，缺少了核心的 `.artifact-detail-body` 类，导致水平与垂直内边距均为 `0px`，字号行高配置失效。
- **解法**：补齐 `artifact-detail-body`，并在 CSS 中重设宽裕内边距 `padding: 20px 24px 24px`，正文行高设为 `1.85`，并为分析段落添加卡片底板。

### P4: 专属页过度设计「展开/收起完整分析」手风琴遭用户否定
- **现象**：在专属问答页顶部添加折叠卡片，用户抱怨每次都需要手动点展开才能阅读核心结论。
- **原因**：违背了用户进入「深入探讨」页面的直觉期望——用户进入专属页就是为了看完整分析与提问的。
- **解法**：彻底移除折叠手风琴，顶部呈现自然全景流：高清原图 -> 核心要点 (Takeaway) -> 深度解析 -> 证据 -> 问答历史 -> 追问建议 -> 吸底输入框。

### P5: 顶部图表未渲染（空白）
- **现象**：用户对某图表做了 Figure Lens，进入专属页后只有文字，顶部图片未渲染。
- **原因**：代码仅根据 `bundle.blockType.toLowerCase().includes("figure")` 做判断，而底层 OCR 把该图表识别成了正文块；且从卡片流推入专属页时未同步传递 `displayCropSrc`。
- **解法**：建立三级高可用加载链路；判定规则扩展至 `artifact.kind.startsWith("lens_")`；截屏裁剪坐标以 `artifact.evidence[0].bbox` 为准。

### P6: Windows / WebKit 下 textarea 即使 1 行也出现上下滚动小箭头
- **现象**：输入框仅有单行光标时，右侧紧挨着发送按钮的位置出现灰色的上下滚动小箭头。
- **原因**：Windows WebKit 内核在 `<textarea>` 存在内边距与浮点行高时，会误认为内容发生微小溢出而激活默认的滚动步进器。
- **解法**：在 1~3 行高度以内，强制通过 style 或 class 设置 `overflow-y: hidden`；仅在真实内容高度超过 3 行（`> 88px`）时才切为 `overflow-y: auto`。

### P7: 测试环境 jsdom MathML 报错崩溃
- **现象**：运行单元测试时，涉及 MathML accessibility 测算的方法（如 `getBBox`）导致测试报错。
- **原因**：`src/MarkdownBody.tsx` 的 rehype-katex 配置影响测试虚拟 DOM。
- **解法**：参见 `docs/markdown-rendering.md`，在 `src/test/setup.ts` 中补齐 SVG / DOMMatrix / Canvas 的 polyfill，勿破坏生产渲染配置。

### P8: 返回卡片流时的视口定位跳动
- **现象**：从专属问答页点击「← 返回区块列表」后，页面滚到了顶部，找不到刚才阅读的卡片。
- **原因**：卡片流组件重新挂载时丢失了原来的滚动偏置。
- **解法**：在 `BlockCardStream` 中利用 `handleBackFromQa` 记录 `blockId`，返回后执行 `elem.scrollIntoView({ behavior: 'smooth', block: 'nearest' })`，并附带 1.2 秒高亮脉冲。组件卸载时必须清除定时器 `clearTimeout(qaScrollTimerRef.current)`。

---

## 7. 如何扩展新能力 (Extension Recipes)

### 菜谱 1：为新区块类型（如表格 Table）增加富交互预览
1. 在 `BlockCard.tsx` 中定位 `availableTabs` 与渲染区：
   - 检查 `bundle.blockType.includes("table")`；
   - 增加表格专用解析渲染（如结构化 Markdown 表格渲染器或高亮单元格）；
2. 保持卡片头部预览统一：图表/表格均配备缩略图与 Lightbox 放大模态弹窗；
3. 更新 `BlockCardStream.tsx` 中的类型筛选胶囊：`table` 分类已有计数逻辑，直接对齐。

### 菜谱 2：为专属问答页增加新操作（如复制分析 Markdown / 导出讨论）
1. 在 `BlockCardQaDetail.tsx` 的顶部导航条中添加操作按钮：
   ```tsx
   <button type="button" className="btn-liquid-pill" onClick={handleExport}>
     导出为 Markdown
   </button>
   ```
2. 注意按钮尺寸遵从微型胶囊规范（`padding: 4px 10px; font-size: 12px;`），不挤占中间的页码跳转与返回键。

---

## 8. 变更日志 (Changelog)

- **2026-09-04 (D-064)**：
  - 首发落地 Lens 区块卡片流 (`BlockCardStream`) 与物理区块卡片 (`BlockCard`)；
  - 落地推入式独立专属问答页 (`BlockCardQaDetail`)，支持顶部原图加载、多级回退与 Lightbox 放大；
  - 彻底去除专属页过度设计的「展开/收起完整分析」手风琴，还原自然流畅全景阅读流；
  - 修复输入框黑色原生边框与缩放把手；
  - 修复容器 padding 缺失导致的文字贴边拥挤问题，正文行高放宽至 1.85 并引入卡片化底板；
  - 实现输入框常驻吸底与 1~3 行无滚动条自适应平滑撑高（超 3 行激活 4px 细滚动条）；
  - 全工程 59 个测试套件、400 项自动化测试通过率 100%。

## 2026-09-09：论文 Lens 完整中文稿与 v2

论文 Lens v2 在原成果详情中展示读法、按顺序排列的重点位置和材料局限，不改变卡片流、版本归并或专属问答导航。旧成果继续原有显示，不推造完成状态。字段与当前验证见 [Lens 合同](lens-generation.md)。

## 2026-09-09：Lens 追问完整中文稿

追问已采用完整中文稿，按当前卡点调整讲法并保留有依据的纠正。只更新当前 Lens 分支的对话，不写回原卡片；不新增固定回答栏目或自动修复。逐轮输入和错误输出记录见 [追问合同](lens-qa-generation.md)。
