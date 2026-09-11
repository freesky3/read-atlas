# UI/UX 细节打磨、交互修复与功能增强设计规格 (2026-08-22)

> 状态：Ready for Review
> 范围：Read Desktop 交互体验全链路优化、细节打磨与关键 Bug 修复。
> 涉及模块：Reading Roadmap、AI 旁批（GuideIsland）、任务与调度中心（Operations Drawer）、回收站与删除确认系统、PDF 导入与 SQLite 软删除约束、提示词中心（Prompt Catalog）。

---

## 1. 需求背景与核心目标

根据用户使用反馈，本期重点解决以下 7 个关键体验与稳定性问题：

1. **精读路线（Reading Roadmap）证据跳转高亮缺失**：点击路线中的证据药丸（如 `📄 Figure 3 (p.4)`）后，阅读器仅翻页但未在页面中聚焦高亮对应 OCR 区块。
2. **AI 好友旁批（GuideIsland）样式失真与菜单收回缺陷**：顶岛旁批按钮出现生硬纯蓝色与「菜单」字样；展开的下拉菜单出现白底白字对比度灾难，且点击外部区域无法自动收起。
3. **任务中心右上角关闭按钮不居中且无 Liquid Glass 质感**：关闭叉号偏心变形，缺乏磨砂玻璃内阴影与悬停微光。
4. **废纸篓图标不清晰且缺乏安全二次确认**：Emoji 废纸篓在 Windows 下渲染为圆柱筒；删除操作调用原生英文 `window.confirm`，极易误触。
5. **误删/恢复后重新导入 PDF 抛出 SQLite UNIQUE 约束冲突**：由于 `papers.relative_path` 在软删除后仍占位，重新导入或恢复后导入同名/同路径 PDF 会导致数据库报错。
6. **提示词中心遗漏可编辑槽位**：检查并确保全部 19 项业务提示词（包含 `reading_roadmap`）在设置中心 100% 可视化编辑、重置与回滚。
7. **全应用体验与细节统一一致性**：统一全应用所有浮层（下拉菜单、气泡）的点击外部/Esc 收起逻辑，优化全应用模态弹窗关闭交互。

---

## 2. 详细设计与实现方案

### 2.1 Reading Roadmap 证据智能跳转与高亮聚焦机制

#### 现象与根因
- `ReadingRoadmapPanel` 中点击证据按钮调用 `onJumpToPage(ev.page, ev.blockId)`。
- LLM 在生成精读路线时回传了 `page` 与 `label`（如 "Figure 2", "Section 3.1"），但 `blockId` 大多为空；`PdfReader` 接收到没有 `blockId` 的跳转时仅更新页码，未对目标内容提供视觉聚焦。

#### 解决方案
1. **智能区块解析器（Evidence Resolver）**：
   - 在前端跳转处理器中，若 `targetBlockId` 存在且匹配有效 OCR Block，直接调用 `setFocusedBlockId(targetBlockId)`。
   - 若 `targetBlockId` 为空，根据 `ev.label` 与目标页 `pageNumber` 自动解析候选 OCR Block：
     - 若包含 "Figure" / "Fig." / "图"，优先匹配该页 `blockType === "figure"` 的区块；
     - 若包含 "Table" / "表"，优先匹配该页 `blockType === "table"` 的区块；
     - 若包含关键词或小节号，对该页 block 的 `textContent` 进行精准子串匹配；
     - 命中后自动将该 block 的 `id` 设为 `focusedBlockId`。
2. **视觉动效增强**：
   - 在 `PdfReader` 中，被聚焦的 Block 触发 `.is-focused` 翡翠绿脉冲光晕与平滑滚动居中；
   - 若目标页没有 OCR 或无法匹配具体区块，对目标页触发 1.2s 柔和的视口边界光晕指示，明确提示读者所在页面。

---

### 2.2 AI 好友旁批（GuideIsland）样式、颜色与交互全面重构

#### 现象与根因
- `.guide-island-caret.is-on` 被硬编码为 `background: #2458a8`，破坏了 Liquid Glass 半透明质感。
- `.guide-island-menu` 采用未充分适配暗色主题的 `var(--glass-card, #fff)` 且直接使用浏览器默认按钮样式，导致浅白背景搭配浅灰文字（白底白字）。
- 没有外部点击（Click Outside）与按键监听，导致必须再次点击原按钮才能关闭。

#### 解决方案
1. **胶囊分段结构 Liquid Glass 拟态重构**：
   - 结构重构为：`[ ✎ 旁批 ] [ ▾ ]` 分段胶囊，完全继承 `btn-liquid-pill` 的磨砂、4 层内阴影与 45° 高光光斑；
   - 激活态采用柔和的主题色微光（`rgba(59, 130, 246, 0.18)` + 翡翠蓝边框），告别粗暴纯色背景。
2. **下拉菜单 Liquid Glass 升级**：
   - `.guide-island-menu` 升级为 `backdrop-filter: blur(24px) saturate(180%)`，采用 `--liquid-glass-shadow`；
   - 菜单项按钮采用半透明悬浮药丸（`.guide-menu-item`），自适应深浅色文字（暗色下亮白/柔灰，浅色下深灰墨黑），悬停呈现微光脉冲。
3. **交互闭环**：
   - 引入标准 Click-Outside 钩子与 `Escape` 键盘监听，点击页面任何空白区域或按 `Esc` 立即平滑收起菜单。
4. **生成确认卡与批注卡片同步美化**：
   - 同步润色 `GuidePlanCard` 与 `GuideCard` 的边框折射与字体排版，确保全套旁批系统风格浑然一体。

---

### 2.3 全局统一 Liquid Glass 居中微型圆钮

#### 现象与根因
- `.operations-head-actions .icon-button` 与 `.outline-button` 共享了 `padding: 6px 14px` 样式，导致圆形叉号按钮被拉伸为扁椭圆，且叉号字符视觉偏心。

#### 解决方案
1. **标准微型圆形关闭按钮规范**：
   - 抽取全局通用的 `.btn-liquid-close`（或统一 `.icon-close-button`）：
     - 尺寸：`width: 28px; height: 28px; min-width: 28px;`
     - 几何居中：`display: grid; place-items: center; line-height: 1; padding: 0;`
     - 拟态质感：`border-radius: 50%; background: var(--glass-surface); border: 1px solid var(--glass-border); box-shadow: var(--liquid-glass-shadow); backdrop-filter: blur(16px);`
     - 悬停微动：`transform: scale(1.08); color: var(--ink); border-color: var(--blue);`
2. **全应用覆盖核对**：
   - 任务中心（Operations Drawer）右上角关闭按钮
   - 设置中心（Settings Workbench）右上角关闭按钮
   - 精读路线（Reading Roadmap）右上角关闭按钮
   - 论证地图检查器（Outline Inspector）关闭按钮
   - 确保全应用所有弹窗/抽屉的关闭图标与交互体验 100% 像素级对齐。

---

### 2.4 废纸篓标准 SVG 图标与防误删 Liquid Glass 模态框

#### 现象与根因
- 顶栏使用 Unicode `🗑`，在 Windows 字体下形似圆柱石墩；
- 删除操作调用浏览器原生 `window.confirm`，既无法定制 Liquid Glass 样式，又容易误按回车造成误删。

#### 解决方案
1. **标准矢量 SVG 垃圾桶图标**：
   - 替换所有废纸篓/删除按钮中的 Emoji 为精致的 SVG 矢量图标（带桶身、桶盖与开槽线条），在暗色/浅色下清晰锐利。
2. **Liquid Glass 安全二次确认模态框（`TrashConfirmModal`）**：
   - 替代 `window.confirm`，弹出应用内磨砂玻璃对话框：
     - **警示标题**：⚠️ 移入废纸篓确认
     - **论文信息**：突出显示待删除论文的真实标题与所在文件夹路径
     - **说明文案**：明确告知「该论文将被移入 30 天回收站，当前正在运行的后台分析任务将立即终止。您随时可在任务中心回收站中一键恢复。」
     - **操作按钮**：明确区分「取消」（次要灰钮）与「确认移入废纸篓」（警示红钮，带微弱呼吸光）。
3. **大厅与工作台统一入口**：
   - 在 Library Hub 的文献卡片/表格上下文菜单或快捷操作栏中，同步提供该安全的删除确认入口。

---

### 2.5 修复 SQLite 软删除唯一索引与无缝导入/恢复机制

#### 现象与根因
- SQLite `papers` 表定义了全局 `relative_path TEXT NOT NULL UNIQUE`。
- 当论文被移入废纸篓时，数据库仅更新 `deleted_at = <timestamp>`，`relative_path` 依然留在表中。
- 当用户再次导入同名/同相对路径的 PDF（或从回收站恢复后尝试重新导入），`register_new_paper` 试图再次 `INSERT INTO papers`，触发 `UNIQUE constraint failed: papers.relative_path` 导致崩溃报错。

#### 解决方案
1. **SQLite 软删除部分唯一索引（Partial Unique Index）**：
   - 建立针对未删除记录的部分唯一索引：
     `CREATE UNIQUE INDEX IF NOT EXISTS idx_papers_active_relative_path ON papers(relative_path) WHERE deleted_at IS NULL;`
   - 并在数据库初始化及运行时容错处理旧的无条件 UNIQUE 约束。
2. **智能防重与无缝恢复流水线（`import_pdf` 加固）**：
   - 当用户导入 PDF 时：
     - **场景 A（该文件已在文献库中）**：
       检测到 `sha256` 或有效 `relative_path` 已存在于未删除文献库中，不报红字错误，而是自动选中该文献、切换至阅读器并高亮提示「该文献已在库中，已为您直接打开」。
     - **场景 B（该文件当前位于回收站中）**：
       检测到匹配的已删除记录，自动调用 `restore_paper` 将其从回收站优雅复原，无缝进入阅读工作台，并提示「已从回收站自动恢复该论文」。
     - **场景 C（全新文件）**：
       正常导入并创建新 revision。

---

### 2.6 设置中心 19 种业务提示词全量补齐与编辑支持

#### 检查结果
- 后端 Rust `PromptSlotId` 共定义了 19 个业务提示词槽位（`paper_root`, `orientation_pack`, `discussion`, `discussion_compaction`, `translation`, `explanation`, `lens_formula`, `lens_figure`, `lens_table`, `lens_repair_formula`, `lens_repair_figure`, `lens_repair_table`, `lens_qa`, `outline_extract`, `outline_compose`, `outline_deep_dive`, `reading_roadmap`, `guide_context`, `guide_annotate`）。
- 前端 `src/promptCatalog.ts` 中的 `PROMPT_SLOTS` 数组遗漏了第 17 个槽位 `reading_roadmap`。

#### 解决方案
1. 在 `src/promptCatalog.ts` 中注册 `reading_roadmap`：
   - Group: `Reading`（或 `Roadmap`）
   - Label: `精读路线`
   - Description: `基于三遍阅读法（Three-Pass Approach）生成论文精读指导与自检清单`
2. 验证前端 Prompts 面板完整展示所有 19 个提示词的分组列表，且每个槽位均支持：
   - 实时文本编辑与字数/行数自适应；
   - 变更状态检测（`已修改` / `系统默认`）；
   - 「保存」、「恢复上一次」与「恢复默认」原子操作。

---

### 2.7 全局交互体验与细节精细化打磨

1. **浮层点击外部自动关闭（Click-Outside）全量排查**：
   - 旁批菜单（`GuideIsland`）
   - 顶栏 Thread 切换下拉菜单
   - 缩放/字号下拉选择器
   - Library Hub 排序下拉菜单
2. **Toast 通知与状态提示系统升级**：
   - 将现有顶部生硬的红色长条报错替换为悬浮式 Liquid Glass Toast 通知，支持自动 3.5s 消失、手动关闭和暗色/浅色自适应。

---

## 3. 涉及修改的文件清单

| 模块 | 文件路径 | 变更类型 | 说明 |
| --- | --- | --- | --- |
| **Reading Roadmap** | `src/ReadingRoadmap.tsx` | MODIFY | 接入智能证据解析，支持 blockId 匹配与页面聚焦 |
| **Reading Roadmap** | `src/App.tsx` | MODIFY | `onJumpToPage` 智能匹配 OCR blocks 并下发高亮 |
| **AI 旁批** | `src/guide/GuideIsland.tsx` | MODIFY | 重构顶岛按钮为分段胶囊，下拉菜单 Liquid Glass 化，增加 click-outside 与 Esc 监听 |
| **AI 旁批** | `src/styles.css` | MODIFY | 移除纯蓝背景硬编码，完善 `.guide-island-menu` 与批注卡片磨砂玻璃样式 |
| **任务中心** | `src/OperationsDrawer.tsx` | MODIFY | 统一右上角关闭按钮为 `.btn-liquid-close` |
| **关闭按钮** | `src/styles.css` | MODIFY | 定义全局 `.btn-liquid-close` 规范，修复偏心与变形 |
| **废纸篓与确认** | `src/App.tsx` | MODIFY | 替换 Emoji 为 SVG 图标，集成 Liquid Glass `TrashConfirmModal` |
| **删除确认模态框** | `src/components/TrashConfirmModal.tsx` | NEW | 专用的 Liquid Glass 防误删二次确认弹窗 |
| **文献大厅** | `src/components/LibraryHub.tsx` | MODIFY | 支持文献卡片/表格安全删除入口 |
| **后端导入与软删除** | `src-tauri/src/paper_module.rs` | MODIFY | 修复软删除唯一索引冲突，支持重导自动打开与回收站自动恢复 |
| **后端数据库架构** | `src-tauri/src/v2_workspace.rs` | MODIFY | 确保 `papers.relative_path` 使用 partial unique index |
| **提示词配置** | `src/promptCatalog.ts` | MODIFY | 补全 `reading_roadmap` 提示词槽位元数据 |
| **自动化测试** | `src/ReadingRoadmap.test.tsx` | MODIFY | 增加跳转与高亮单元测试 |
| **自动化测试** | `src/guide/GuideIsland.test.tsx` | MODIFY | 增加菜单交互与 click-outside 测试 |
| **后端测试** | `src-tauri/src/paper_module.rs` | MODIFY | 增加软删除后重新导入/恢复测试用例 |

---

## 4. 验证与回归计划

1. **前端自动化测试**：
   ```powershell
   npx vitest run --maxWorkers=1 --fileParallelism=false
   npx tsc -b
   ```
2. **后端自动化测试**：
   ```powershell
   cd src-tauri
   cargo test --locked
   cargo clippy --locked --all-targets -- -D warnings
   ```
3. **真实场景端到端验证**：
   - 打开论文，点击精读路线中的 `📄 Figure X` 证据药丸，验证阅读器能否精准定位并出现翡翠绿脉冲高亮；
   - 点击顶栏「旁批 ▾」按钮，验证下拉菜单为磨砂拟态风格，点击其他区域能自动收起；
   - 打开任务中心，检查右上角关闭叉号是否完美正圆居中；
   - 点击顶栏废纸篓图标，验证弹出精致的 Liquid Glass 二次确认弹窗；
   - 删除论文后重新导入同名 PDF，验证能否无缝恢复或打开，绝不报错；
   - 打开设置 → Prompts，验证包含「精读路线」在内的全部 19 个提示词均可编辑并恢复默认。
