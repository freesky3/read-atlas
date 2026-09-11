# UI/UX 重构规格说明书：文献大厅、超沉浸阅读工作台与 Liquid Glass 设计系统

- **版本**: V2 UI/UX Redesign
- **日期**: 2026-08-17
- **状态**: Approved (待进入实现规划)

---

## 1. 背景与重构目标

### 1.1 现状与痛点
1. **视图流转单一**：当前应用冷启动即呈现并排三栏（左侧文库 300px + 中间 PDF + 右侧讨论），未选论文时大面积留白，文库信息密度低。
2. **沉浸空间受限**：进入阅读后，左侧文库依然常驻，挤占横向宽度；顶栏高度 72px 占用过多纵向高度；AI 对话使用嵌套气泡框，公式与长段分析受限。
3. **视觉风格不契合**：现有的复古暖黄底色（`#eee9df`）与杂色噪点偏向小说阅读风格，缺乏现代学术工具的严谨、清晰与启发性。

### 1.2 重构目标
1. **独立文献大厅（Library Hub）作为主入口**：支持文件夹层级树（Folder Tree Hierarchy）与本地 `Papers/` 对齐，论文卡片完整展示一句话核心导读（Full TL;DR），提供网格与表格一键切换。
2. **超沉浸双画布阅读工作台（Focus Reader Workspace）**：
   - 阅读模式下彻底收起文库侧边栏，横向 100% 留给 PDF（~60%）与 AI 研读助手（~40%）。
   - 顶栏压缩为 42px 极简浮岛条，支持一键全屏折叠与无级拖拽。
   - PDF Block 悬浮采用微型胶囊（Micro-Action Capsule），本地已生成资产打上绿色勾标（`✓`）。
   - AI 回答采用**无边框全宽流式学术排版（Frameless Full-Width Editorial Layout）**，公式居中高亮。
3. **克制纯净的 Liquid Glass 设计系统**：
   - 采用纯净中性冷色调（黑白灰冷灰 + 极简科技蓝点缀），保持学术推导的高对比度与舒适度。
   - 组件引入 Apple visionOS 风格的流体玻璃质感（厚透镜磨砂、1px 细高光棱边、圆角浮岛与大胶囊形态、微弹交互）。

---

## 2. 视图流转与状态架构

```
                     ┌─────────────────────────────┐
                     │     应用启动 (Cold Start)    │
                     └──────────────┬──────────────┘
                                    │
                                    ▼
                     ┌─────────────────────────────┐
                     │   文献管理大厅 (Library Hub)  │
                     │  - Papers/ 文件夹层级目录树   │
                     │  - 实时检索 / 过滤 / 导入 PDF │
                     │  - [田 网格卡片] / [≡ 结构表格]│
                     └──────────────┬──────────────┘
                                    │
             点击论文卡片/表格行     │  点击 "‹ 返回文库 (Esc)"
             (恢复页码/缩放/讨论)   │
                                    ▼
                     ┌─────────────────────────────┐
                     │ 超沉浸阅读工作台 (Focus Reader)│
                     │  - 42px 悬浮极简顶岛          │
                     │  - 60% 宽幅 PDF + 微型 OCR 胶囊│
                     │  - 40% 无边框全宽 AI 思考讨论 │
                     │  - 支持一键 100% 纯 PDF 全屏   │
                     └─────────────────────────────┘
```

### 2.1 状态模型定义
在 `App.tsx` 顶层维护视图状态：
- `viewMode`: `'library' | 'reader'`（进入应用默认为 `'library'`；选择论文后切为 `'reader'`，点击返回按钮或按 `Esc` 切回 `'library'`）。
- `libraryViewType`: `'grid' | 'table'`（持久化至 `localStorage`）。
- `activeFolder`: 当前选中的文件夹路径（根目录为 `Papers/`）。
- `chatWidth` & `isPdfOnly`: 阅读器分栏宽度比例与单栏全屏模式切换。

---

## 3. 核心界面与交互规范

### 3.1 文献管理大厅（Library Hub）
1. **顶栏浮岛（Floating Hub Header）**：
   - 左侧：品牌 Logo `R` + 标题 `Read Desktop` + 文献统计与 Workspace 胶囊。
   - 右侧：`＋ 导入 PDF`（Primary Capsule）、`⚡ 任务中心`、`⚙ 设置`、主题切换快捷胶囊。
2. **文件夹层级目录树（Folder Tree Sidebar）**：
   - 树状展示本地 `Papers/` 的子目录，带展开/折叠箭头（`▾` / `▸`）与文献数量徽标。
   - 支持根目录与各子目录快速切换，过滤右侧论文列表。
3. **主舞台（Main Stage）**：
   - 顶部工具栏：实时检索输入框（模糊匹配标题/作者/一句话摘要）、排序下拉、网格/表格切换胶囊。
   - **学术网格卡片（Grid View）**：
     - 卡片标题、作者、年份、OCR/Brief 状态胶囊。
     - **完整一句话核心贡献摘要（Full TL;DR）**：无省略号截断，左侧带强调色高光条，便于快速掌握论文精髓。
   - **结构化文献表格（Table View）**：
     - 列：论文标题、一句话核心摘要、第一作者、年份、状态、操作，高密度展示。

### 3.2 超沉浸阅读工作台（Focus Reader）
1. **极简 42px 顶栏浮岛（Lean Topbar Island）**：
   - 左侧：`‹ 返回文库 (Esc)` 胶囊按钮、论文标题（单行优雅截断）、当前页码 `p. 3 / 18`。
   - 右侧：OCR 状态指示、`⤢ 纯看 PDF` 一键全屏切换、比例重置。
2. **宽幅 PDF 阅读画布（约 60% 宽度）**：
   - 居中高质量渲染 PDF 页面，周边留白舒适，双指捏合缩放（60%–180%）。
   - **微型 OCR 悬浮胶囊（Micro-Action Bar）**：鼠标移入 Block 时，右上角平滑浮现超微胶囊（`＋ 引用`、`🔬 Lens`、`🌐 翻译`）。
   - **本地资产已生成状态打绿勾（`✓`）**：若对应 Block 已在本地生成了 Lens 或翻译，图标旁带绿色徽标，一目了然。
3. **无边框全宽 AI 研读助手（约 40% 宽度）**：
   - 顶部标签：`[💬 研读讨论]` 与 `[📑 阅读成果]`。
   - **用户提问**：右对齐轻量小气泡。
   - **AI 回答**：**无多余外边框与卡片限制**，采用 100% 全宽度的自然学术排版，公式居中渲染并支持一键复制，引用 Block 点击可平滑定位到 PDF 对应页码。
   - **底部提问区**：微型引用篮胶囊贴合在多行输入框上方。

---

## 4. Liquid Glass 设计系统与样式 Tokens

### 4.1 设计原则
- **色彩克制（Scholarly Restraint）**：严禁过度饱和与刺眼杂色，采用中性黑白灰冷灰为主，深冷蓝与翠绿作为功能点缀。
- **晶莹通透（Frosted Specular Refraction）**：浮岛、胶囊与侧边栏采用 `backdrop-filter: blur(24px) saturate(180%)`，上沿配置 1px 高光边框（`inset 0 1px 1.5px rgba(255,255,255,0.95)`）。
- **流体微交互（Fluid Micro-Interactions）**：按钮与卡片 hover 带 `scale(1.02)` 柔和浮动与透镜阴影扩散，点击带 `scale(0.97)` 弹性反馈。

### 4.2 CSS Tokens 规格

```css
:root, [data-theme="liquid-light"] {
  --bg-base: #f8fafc;
  --bg-gradient: linear-gradient(180deg, #f1f5f9 0%, #e2e8f0 100%);
  --glass-surface: rgba(255, 255, 255, 0.68);
  --glass-surface-subtle: rgba(255, 255, 255, 0.42);
  --glass-card: rgba(255, 255, 255, 0.75);
  --glass-card-hover: rgba(255, 255, 255, 0.95);
  --glass-border: rgba(255, 255, 255, 0.75);
  --glass-border-subtle: rgba(226, 232, 240, 0.6);
  --glass-specular: inset 0 1px 1.5px 0 rgba(255, 255, 255, 0.95), 0 8px 24px -4px rgba(15, 23, 42, 0.06);
  --glass-specular-hover: inset 0 1.5px 2px 0 rgba(255, 255, 255, 1), 0 14px 28px -4px rgba(15, 23, 42, 0.12);
  --glass-blur: blur(24px) saturate(180%);
  --text-main: #0f172a;
  --text-muted: #64748b;
  --accent: #0f172a;
  --accent-blue: #2563eb;
  --paper-sheet: #ffffff;
  --success: #16a34a;
}

[data-theme="liquid-dark"] {
  --bg-base: #0b0f19;
  --bg-gradient: linear-gradient(180deg, #0b0f19 0%, #030712 100%);
  --glass-surface: rgba(22, 29, 46, 0.65);
  --glass-surface-subtle: rgba(22, 29, 46, 0.4);
  --glass-card: rgba(30, 41, 59, 0.55);
  --glass-card-hover: rgba(30, 41, 59, 0.85);
  --glass-border: rgba(255, 255, 255, 0.12);
  --glass-border-subtle: rgba(255, 255, 255, 0.06);
  --glass-specular: inset 0 1px 1px 0 rgba(255, 255, 255, 0.18), 0 10px 30px rgba(0, 0, 0, 0.4);
  --glass-specular-hover: inset 0 1px 1.5px 0 rgba(255, 255, 255, 0.3), 0 16px 36px rgba(0, 0, 0, 0.5);
  --glass-blur: blur(28px) saturate(190%);
  --text-main: #f8fafc;
  --text-muted: #94a3b8;
  --accent: #f8fafc;
  --accent-blue: #38bdf8;
  --paper-sheet: #172033;
  --success: #4ade80;
}
```

---

## 5. 后端与数据层兼容性说明

1. **零破坏性变更**：
   - 保持所有 Tauri 原生 IPC 通信命令不变（如 `import_pdf`, `start_ocr`, `send_chat`, `get_document_state` 等）。
   - 保持本地 SQLite 数据结构与 Artifact 存储结构不变。
2. **新增前端组件结构**：
   - `src/components/LibraryHub.tsx`：文献大厅组件（含文件夹树、网格/表格切换、一句话摘要提取渲染）。
   - `src/components/ThemeManager.tsx` 或顶层 Theme Provider：管理 `liquid-light` 与 `liquid-dark` 及持久化。
   - `src/styles.css` 重构与 Token 替换：替换掉旧的 warm grain 样式，全面引入 Liquid Glass 样式体系。

---

## 6. 验证与测试策略

1. **前端组件与逻辑测试**：
   - Vitest 单元测试覆盖 `LibraryHub` 视图切换、文件夹过滤、论文选择。
   - 确保 `sendChat`, `blockQuotes`, `PdfReader` 原有 39 个 Vitest 测试 100% 通过。
2. **静态类型与代码构建**：
   - `npx tsc -b` 0 错误。
   - `npm run build` 打包构建验证。
3. **Rust 后端验证**：
   - `cargo check --locked` 与 `cargo test --locked` 通过。
