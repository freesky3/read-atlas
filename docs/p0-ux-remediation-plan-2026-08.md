# Read Desktop P0 用户体验修复规格

状态：已审查并修正实现；自动化门槛通过，桌面人工发布门槛待验收  
日期：2026-08-31  
适用范围：V2 首次面向真实用户使用前的稳定性修复  
目标读者：产品、UI、React、Tauri/Rust、测试

> 本文第 3–11 节的“问题描述”和“当前复现”记录的是修复前基线，不代表当前实现仍保留这些缺陷。发布状态以本节和第 13 节为准。

## 0. 2026-08-31 实现审查与修正记录

### 0.1 审查结论

原实现已覆盖大部分界面，但在数据安全、失败任务重试权威性、通知生命周期、阅读器真实尺寸计算和响应式降级方面仍有发布阻断缺陷。本次审查已直接修正这些实现，并通过前后端全量自动化；由于尚未完成真实 Tauri 环境中的故障注入、系统缩放和长路径/锁文件验证，当前不能将第 13 节的 P0 Definition of Done 判定为全部完成。

### 0.2 审查中发现并修正的关键缺陷

1. Legacy 重置的回滚路径可能删除已生成的备份，跨卷 copy/remove 也无法提供原子性；现已改为同卷原子 rename、严格预检、精确备份目录、失败后保留诊断目录并进行受检回滚。旧的无预检重置 IPC 已移除。
2. Workspace 切换只检查了局部 busy 状态，没有检查 queued/running/paused 的持久任务；现会阻止静默切换并引导用户进入任务中心。
3. 通知 action 曾可保存 React 回调，重复状态还可能产生通知风暴；现使用受控 action 枚举、同步去重、凭据脱敏、错误持久化，并对兼容 `status` 桥接做等价消息去重和警告/错误分级。
4. 重试风险曾由前端根据字段猜测，并可能落回错误 Provider 或只入队而不唤醒 worker；现由后端投影 `retryDisposition`，严格复用冻结 route，以稳定幂等键创建新 Job，并显式唤醒 worker。不可归因、Local 和旧 route 默认不可重试。
5. 适宽/适页曾使用假定页面尺寸；现读取 PDF 页面的真实 viewport、旋转角和 Reader 容器尺寸，并通过 `ResizeObserver` 保持当前 fit 模式。
6. OCR Block 缺少复制和完整键盘路径，隐藏 action 仍可能进入 Tab 顺序；现只渲染选中 Block 的操作，支持键盘选择、Escape、复制规范化文本，并在虚拟页卸载时恢复可预测焦点。
7. Overlay 焦点管理原本分散且多层 Escape 可能穿透；现共享栈式 focus trap，并补齐 Lightbox、Settings、Reset Confirm、Operations Drawer 和顶栏菜单的焦点恢复。
8. 响应式实现曾用 root `overflow-x: hidden` 遮住溢出；现让 Reader 顶栏真实换行/收纳，并在更多菜单中保留适宽、适页、旋转和导出的键盘等价入口。
9. Gemini 没有持久化探针模型列表时，已保存的自定义模型会被误判为预设；现使用内置模型目录判定，并为该恢复路径加入回归测试。

### 0.3 当前验证状态

| 范围 | 实现状态 | 自动化证据 | 仍需人工/真机验证 |
| --- | --- | --- | --- |
| P0-01 Legacy 安全重置 | 已实现 | Rust 重置归档/重复执行/数据库验证测试；`cargo test` 通过 | Windows 文件占用、只读目录、磁盘满、超长路径、大型 Workspace、进程中断后的恢复演练 |
| P0-02 自定义模型 | 已实现 | 自定义 Gemini 恢复和字段校验回归；`npm test` 通过 | 真实 Provider 探针后在预设/自定义间往返并重启应用 |
| P0-03 Workspace 出口 | 已实现 | 组件接线与重置预检测试；前端全量通过 | 文件选择取消、被另一进程锁定、活动任务切换提示的桌面回归 |
| P0-04 全局反馈 | 已实现，保留兼容桥 | 通知脱敏、持久化、去重、受控 action 测试 | 各主流程逐一故障注入；屏幕阅读器播报顺序 |
| P0-05 安全重试 | 已实现 | 后端三种 disposition 及现有持久 Job/去重测试；Rust 全量通过 | 真实 Provider 的提交前失败、提交后断网和可能重复计费确认 |
| P0-06 Reader 控制 | 已实现 | fit/旋转/缩放边界与 Reader 回归测试 | 混合尺寸、横向扫描、500 页 PDF 和系统缩放 |
| P0-07 OCR 键盘路径 | 已实现 | Testing Library 键盘、复制、Escape、虚拟滚动测试 | NVDA/VoiceOver 语义核验和旋转 bbox 目视检查 |
| P0-08 Overlay/分隔条 | 已实现基础合同 | Overlay 组件和 Outline 键盘回归；前端全量通过 | 多层 Overlay、菜单焦点恢复和鼠标 scrim 的真实桌面回归 |
| P0-09 响应式 | 已实现代码降级 | TypeScript/生产构建通过 | 支持矩阵截图、200% 文字、长中英文标题和长 Workspace 路径 |

### 0.4 自动化执行结果

2026-08-31 在当前工作区执行：

- `npm test`：43 个测试文件、226 个测试通过；
- `npm run build`：TypeScript project build 与 Vite production build 通过；
- `cargo test`：276 个 Rust 测试通过；
- `git diff --check`：通过，仅有 Git 的 LF/CRLF 工作区提示。

这些结果只满足第 12.1 节的自动化门槛，不替代第 12.2、12.3 节以及第 13 节要求的真实桌面验收。

## 1. 目的与 P0 定义

本计划不增加新的 AI 模式。P0 只处理满足以下任一条件的问题：

1. 可能造成用户文件、数据库或付费任务状态不可恢复；
2. 阻断首次设置、打开 Workspace、阅读 PDF 或处理失败任务等主路径；
3. 操作失败但用户无法察觉、理解或恢复；
4. 核心操作只能用鼠标完成，或对键盘用户完全不可达；
5. 在项目支持的最小窗口或常见系统缩放下，关键操作被裁切或无法访问。

下列能力很重要，但不属于本轮 P0：全文检索、私人笔记、批量导入/多选、跨论文比较、Zotero/Obsidian 互操作、多设备同步和新的 AI Artifact。

## 2. 执行摘要

| ID | 问题 | 风险 | 发布门槛 | 主要入口 |
| --- | --- | --- | --- | --- |
| P0-01 | Legacy Workspace 重置的文案与实际删除行为冲突 | 数据丢失 | 必须先修 | `SettingsWorkbench.tsx`、`SettingsWorkspacePage.tsx`、`v2_workspace.rs` |
| P0-02 | 自定义模型 ID 输入框输入首字符后消失 | 设置阻断 | 必须修 | `SettingsModelsPage.tsx` |
| P0-03 | Workspace 设置缺少选择/更换入口 | 首次设置与故障恢复死路 | 必须修 | `SettingsWorkbench.tsx`、`SettingsWorkspacePage.tsx` |
| P0-04 | 全局错误反馈隐藏在不可聚焦的状态悬浮层 | 静默失败 | 必须修 | `App.tsx`、全局通知组件 |
| P0-05 | 普通失败任务没有安全的重试或回到来源入口 | 失败不可恢复 | 必须修 | `OperationsDrawer.tsx`、Job IPC |
| P0-06 | 阅读器缺少跳页、适宽/适页、旋转及一致缩放边界 | 核心阅读阻断 | 必须修 | `App.tsx`、`PdfReader.tsx` |
| P0-07 | OCR Block 只能用鼠标触发，缺少语义和键盘路径 | 核心能力不可达 | 必须修 | `PdfReader.tsx` |
| P0-08 | Drawer、Lightbox、菜单和分隔条的 Escape/焦点链不完整 | 键盘迷失、误退出 | 必须修 | `App.tsx`、`OperationsDrawer.tsx`、共享 Overlay 基础组件 |
| P0-09 | 设置页和阅读顶栏在常见窗口宽度下溢出 | 关键控件被裁切 | 必须修 | `styles.css`、设置页和顶栏 |

建议实施顺序：P0-01 → P0-02/P0-03 → P0-04 → P0-05 → P0-06/P0-07 → P0-08 → P0-09。P0-04 的统一反馈接口应在后续工作流接线前完成，避免继续增加 `setStatus(string)` 分支。

---

## 3. P0-01：Legacy Workspace 重置必须可预览、可回滚

### 3.1 问题描述

设置界面承诺 `papers/` 下的原始 PDF 不会被删除，并概括为“保留工作区目录下的原始 PDF”。实际后端 `reset_legacy` 会删除根目录下的 `workspace.sqlite3`，并递归删除整个旧 `library/` 目录；现有 Rust 测试还明确断言其中的 `paper.pdf` 被删除。

这不是单纯的文案问题：旧用户可能把唯一 PDF 副本放在 `library/`，并基于“原始 PDF 会保留”的承诺确认重置。

### 3.2 当前复现

1. 创建 Legacy Workspace，并在 `<workspace>/library/` 放置 PDF；
2. 在设置中点击“重置工作区数据”；
3. 界面提示原始 PDF 会被保留；
4. 输入 `RESET`；
5. 后端删除整个 `library/`，PDF 不再存在。

### 3.3 目标体验

重置必须从“立即删除”改为“先检查、再归档、最后切换”：

1. 用户先看到精确预览：旧数据库路径、旧 `library/` 路径、PDF 数量、文件总数、总大小、备份位置；
2. UI 明确区分“将保留的 V2 文件”和“将归档的 Legacy 文件”；
3. 重置期间旧数据先移动到可恢复备份，不调用 `remove_dir_all`；
4. V2 初始化失败时自动回滚；
5. 成功后显示备份位置和“打开备份目录”；
6. P0 阶段不自动清除该备份，后续再设计保留期和清理策略。

推荐确认文案：

> 将把旧数据库和 `library/` 中的 N 个文件移动到 `<backup path>`，然后创建 V2 Workspace。该备份不会在本次操作中删除。若初始化失败，Read Desktop 会自动恢复旧目录。

### 3.4 建议的后端修改

新增只读预检命令，避免 UI 根据路径猜测：

```text
inspect_legacy_reset(root) -> LegacyResetPreview
```

`LegacyResetPreview` 至少包含：

- `rootPath`
- `legacyDatabasePath` / `legacyDatabaseExists`
- `legacyLibraryPath` / `legacyLibraryExists`
- `fileCount` / `pdfCount` / `totalBytes`
- `backupPath`
- `warnings`
- `canProceed`

新增执行命令或扩展现有命令：

```text
execute_legacy_reset(root, expectedPreviewDigest, confirmation) -> ResetResult
```

执行顺序：

1. 再次解析并校验绝对 Workspace 根路径；拒绝根目录、符号链接跳转和超出 Workspace 的目标；
2. 停止 watcher，拒绝在有活动任务或未关闭数据库句柄时开始；
3. 重新预检，并校验 `expectedPreviewDigest`，防止确认后目录内容变化；
4. 在同一 Workspace 内创建不可冲突的 `.read-desktop-backups/reset-<timestamp>-<uuid>/`；
5. 通过同卷原子 rename 将 `workspace.sqlite3` 文件集和 `library/` 移入备份目录；
6. 写入 `manifest.json`，记录原路径、大小、时间和恢复状态；
7. 初始化 V2，并执行 SQLite `quick_check`；
8. 初始化失败时按 manifest 反向 rename，恢复 Legacy 文件集；
9. 成功后返回备份路径和摘要，由 UI 展示。

现有 `db.rs` 已有 checkpoint、数据库副本校验和带恢复文件的原子 restore 原语，可复用其安全校验思路；但 Legacy `library/` 仍需独立的目录归档协议。

### 3.5 前端修改

- 将当前单步确认框改为“预检 → 确认 → 执行结果”三态；
- 预检未完成前禁用最终确认；
- 显示真实路径、文件数、大小和警告，不使用笼统的“PDF 不会被删除”；
- 执行时禁止关闭对话框，除非后端明确支持取消；
- 成功结果提供“打开新 Workspace”和“打开 Legacy 备份”；
- 失败结果保持可见，提供“复制诊断信息”和“打开恢复目录”；
- 所有状态通过 P0-04 的通知接口报告。

### 3.6 验收标准

- [ ] 任何包含 Legacy PDF 的重置都不会直接删除其唯一副本；
- [ ] 预检展示的文件数、字节数和执行时处理的内容一致；
- [ ] 模拟 V2 初始化失败后，旧数据库和 `library/` 完整恢复；
- [ ] 重复点击执行不会产生第二次重置或覆盖第一次备份；
- [ ] 权限不足、磁盘不可写、文件被占用时在任何删除/移动前失败；
- [ ] UI 文案与真实路径、真实保留策略完全一致；
- [ ] 成功和失败都可以从任务/通知记录中再次找到。

### 3.7 必需测试

- Rust：预检不改变文件系统；成功归档；初始化故障回滚；符号链接拒绝；目标冲突；权限错误；幂等重试；
- React：预检前不可确认；输入确认词；摘要渲染；执行失败保持对话框；成功展示备份路径；
- 桌面实测：包含中文名、长路径、只读文件、1000 个文件和至少 2 GB Legacy 目录。

---

## 4. P0-02：自定义模型 ID 必须保持稳定输入状态

### 4.1 问题描述

非 Gemini Proxy 的模型选择器把 `paperModel === "__custom__"` 同时作为“当前处于自定义模式”和 select 的值。用户输入第一个字符时，`paperModel` 立即变为真实文本，条件不再成立，输入框随即卸载。

Gemini Proxy 分支也存在不同的自定义状态处理方式，应统一，避免两套逻辑继续分叉。

### 4.2 目标体验

- 选择“自定义模型 ID”后，输入框持续存在；
- 输入内容不会因重新渲染、切换思考强度或 Provider 列表刷新而丢失；
- 切回预设模型后保留本次自定义草稿；
- 保存前验证空值和首尾空格；
- 保存成功后重新打开设置，仍显示为自定义模式和原始 ID。

### 4.3 建议修改

不要再用魔法字符串同时表达模式和最终值。增加 UI 层状态：

```text
modelSelectionMode: "preset" | "custom"
selectedPresetModelId: string
customModelIdDraft: string
```

持久化合同仍可保持单一 `paperModel: string`，只在加载和保存边界转换：

- 加载：若值命中已知选项，则进入 `preset`；否则进入 `custom`；
- 保存：`preset` 保存选项 ID，`custom` 保存 trim 后的草稿；
- Provider 切换时，草稿按 Provider 实例保存，不共享到其他实例；
- 不限制 `/`、`:`、`.`、`-` 等常见模型 ID 字符，只拒绝空值、控制字符和不合理长度；
- 校验错误显示在字段下方，并将焦点留在输入框。

### 4.4 验收标准

- [ ] 输入第一个字符后输入框不卸载、不丢焦点；
- [ ] 可以粘贴、删除和输入常见模型 ID 符号；
- [ ] 自定义值保存、关闭设置、重新打开后不变；
- [ ] 切换预设再切回自定义，草稿仍存在；
- [ ] 空值不能保存，错误与字段关联；
- [ ] Gemini Proxy 与其他 Provider 使用相同的模式语义。

### 4.5 必需测试

新增 `SettingsModelsPage.test.tsx`，覆盖首字符、连续输入、粘贴、预设/自定义切换、保存重载、空值校验和 Provider 实例隔离。

---

## 5. P0-03：Workspace 设置必须提供完整出口

### 5.1 问题描述

`SettingsWorkbenchProps` 声明并由 App 传入 `onChooseWorkspace`、`onOpenWorkspace`，但组件解构时没有接收，`SettingsWorkspacePage` 也没有对应按钮。Workspace 异常时，用户被引导去设置，却可能无法在那里选择、切换或打开 Workspace。

### 5.2 目标体验

Workspace 设置页固定提供：

- 当前路径和当前状态；
- “打开文件夹”；
- “更换 Workspace”；
- 未选择时的主按钮“选择 Workspace”；
- `reset_required` 时的“检查并安全重置”；
- 操作中的进度、失败原因和恢复建议。

### 5.3 建议修改

1. 在 `SettingsWorkbench` 正确解构 `onChooseWorkspace` 和 `onOpenWorkspace`；
2. 将两个回调显式传入 `SettingsWorkspacePage`；
3. Workspace 卡片根据状态渲染唯一主操作：
   - `null`：选择 Workspace；
   - `ready`：打开文件夹，次要操作为更换；
   - `reset_required`：安全重置，次要操作为选择其他目录；
   - `busy`：禁用更换并解释正在进行的动作；
4. 更换前检查活动任务；存在活动任务时复用安全退出语义，不静默切换；
5. 目录选择取消不是错误，不弹红色通知；权限或锁错误必须进入 P0-04 通知系统。

### 5.4 验收标准

- [ ] 无 Workspace 时可仅通过设置页完成选择；
- [ ] 当前 Workspace 可以从设置页在资源管理器中打开；
- [ ] 可以更换到另一个合法 Workspace；
- [ ] 有活动任务时不会静默切换；
- [ ] 选择取消、路径无权限、已被其他进程锁定三种结果有不同反馈；
- [ ] `reset_required` 不再进入无操作可执行的死路。

### 5.5 必需测试

扩展 `SettingsWorkbench.test.tsx`：断言回调接线、状态对应按钮、busy 禁用、取消不报错、失败通知和焦点返回。

---

## 6. P0-04：建立统一、可访问、可行动的反馈系统

### 6.1 问题描述

当前大量成功和失败只写入 `status: string`。Reader 中唯一展示位置位于 Token/健康状态的 hover popover，触发区不可聚焦，状态圆点也不随错误变色。因此键盘、触屏以及没有主动悬停的鼠标用户会错过 Workspace、导出、Chat、任务和设置失败。

同一个字符串同时承担健康状态、短暂成功反馈和严重错误，无法表达严重度、来源、持续时间或可执行动作。

### 6.2 目标体验

反馈按层级出现：

1. 字段错误：紧贴对应输入；
2. 当前操作失败：页面内错误条；
3. 跨页面或后台任务事件：全局通知；
4. 可长期追溯的失败：任务中心记录；
5. 顶栏健康状态只表示 Provider/Workspace/任务健康，不再充当唯一错误出口。

### 6.3 建议的数据模型

新增集中通知模型，而不是继续传播裸字符串：

```text
AppNotice {
  id
  severity: info | success | warning | error
  title
  message
  source: workspace | provider | import | reader | chat | artifact | job | export
  entityId?
  createdAt
  persistent
  dedupeKey?
  action?: { kind, label, payload }
}
```

提供窄接口：`notifyInfo`、`notifySuccess`、`notifyWarning`、`notifyError`。action 使用受控枚举，不向状态中保存 React 回调。

### 6.4 展示规则

- success/info：默认 4 秒自动消失；
- warning：默认 8 秒，涉及付费、stale 或中断时保持；
- error：默认保持到用户关闭或完成恢复操作；
- 同一 Job 的同一状态通过 `dedupeKey` 合并，不连续刷屏；
- 通知显示短消息和主动作，技术详情折叠；
- `aria-live="polite"` 用于普通反馈，当前操作失败使用 `role="alert"`；
- 健康状态触发器改为可聚焦 button，并随真实状态变色；
- 错误文本经过脱敏，禁止显示 API Key、完整凭据、未脱敏请求体或敏感 URL 参数。

### 6.5 迁移策略

1. 先建立 `NoticeProvider/NoticeCenter` 和测试；
2. 保留 `status` 作为兼容层，但所有 `catch` 和后台失败必须同时走结构化通知；
3. 按 Workspace、导入、Provider、Chat、Artifact、导出、Job 顺序替换 `setStatus("...failed")`；
4. 最终将顶栏 `status` 改为健康摘要，禁止业务错误只写入该字段；
5. 代码评审规则：新增 `catch` 若没有 inline error、notice 或显式再抛出，不得合入。

### 6.6 验收标准

- [ ] 导入、Workspace、Provider、Chat、Artifact、导出和任务失败无需悬停即可看见；
- [ ] 键盘用户能聚焦通知、执行动作并关闭；
- [ ] 同一错误不会产生通知风暴；
- [ ] 错误不会被 4 秒自动清除；
- [ ] 顶栏状态颜色与 Workspace/Provider/任务健康一致；
- [ ] 通知中不出现凭据和未脱敏请求内容；
- [ ] 成功、警告、错误在视觉和屏幕阅读语义上可区分。

### 6.7 必需测试

- 通知生命周期、去重、动作、Escape/关闭、ARIA live region；
- 各主流程至少一个失败集成测试；
- 脱敏回归测试；
- 触屏/键盘无法 hover 时仍可查看最新错误。

---

## 7. P0-05：失败任务必须有安全恢复路径

### 7.1 问题描述

任务中心对 queued/running/paused 提供暂停、继续、优先级和取消，但普通 failed 任务通常只有错误文本。用户无法判断应该重试、修改 Provider、回到论文重新生成，还是为了避免重复计费而停止。

简单地给所有失败任务增加“重试”也不安全：远程请求可能已经提交，盲目重放可能造成重复请求和重复计费。

### 7.2 目标体验

每张失败任务卡必须回答三件事：

1. 什么失败了；
2. 是否可能已经产生远程请求或费用；
3. 用户下一步可以安全做什么。

至少提供以下一个动作：

- 安全重试；
- 重新检查原 Provider；
- 打开对应 Provider 设置；
- 回到来源论文/Artifact；
- 查看技术详情；
- 明确标记“不可安全重试”。

### 7.3 建议的 Job 投影扩展

后端计算而不是前端猜测：

```text
retryDisposition: safe | confirm_possible_charge | unavailable
retryReason: string
sourceLocator?: { paperId, revisionId, artifactKind, objectKey }
```

规则：

- 本地阶段失败或确认未发出远程请求：`safe`；
- 已提交但没有可靠 receipt：`confirm_possible_charge`；
- Provider route 丢失、旧任务不可归因或状态不确定：`unavailable`，走现有 recovery 引导；
- 重试创建新 Job，并记录 `retryOfJobId`，保留原失败记录，不能覆盖审计历史；
- 对可能重复计费的重试使用二次确认，并显示原因，不使用通用“确定吗”。

### 7.4 前端修改

- failed 卡片显示精简错误、失败阶段、Provider、时间和计费风险；
- “回到来源”先关闭抽屉，再打开论文和对应面板；
- “查看详情”显示脱敏错误与 job id，提供复制；
- 触发恢复后显示新 Job 链接；
- 当任务失败通知出现时，P0-04 的主动作是“查看任务”。

### 7.5 验收标准

- [ ] 每种 failed Job 至少有一个明确、安全的下一步；
- [ ] 不确定远程状态时绝不一键重放；
- [ ] 安全重试产生新 Job，并可追溯到原 Job；
- [ ] “回到来源”可定位正确论文和功能区域；
- [ ] 错误详情经过脱敏；
- [ ] 失败通知与任务卡不会重复触发多个重试。

### 7.6 必需测试

- Rust：三种 retry disposition、重复调用幂等、retry lineage、Provider route 丢失、可能计费状态；
- React：每种 disposition 的按钮和文案、确认流程、回到来源、busy 防重复点击；
- 桌面实测：坏 Key、断网、超时、进程中断、Provider 返回 429/5xx。

---

## 8. P0-06：补齐阅读器基础控制并统一状态边界

### 8.1 问题描述

顶栏页码是纯文本，只有缩小和放大。`rotation` 已经保存、恢复并传入 `PdfReader`，但没有用户修改入口。顶栏缩小允许到 50%，而规格和 Reader 其他逻辑使用 60% 下限，存在状态边界不一致。

用户阅读长论文时无法直接跳页、适宽、适页或旋转扫描页，只能滚动寻找。

### 8.2 目标控制组

顶栏的阅读控制按以下顺序提供：

- 上一页；
- 可编辑页码输入 `/ 总页数`；
- 下一页；
- 缩小、当前比例、放大；
- 适宽；
- 适页；
- 顺时针旋转 90°。

窄窗口时保留页码，缩放模式与旋转进入“阅读视图”菜单。

### 8.3 建议实现

- 抽取唯一常量：`MIN_ZOOM = 60`、`MAX_ZOOM = 180`、`ZOOM_STEP = 10`；
- 页码输入维护本地 draft，只在 Enter/blur 时提交；非法值不改变当前页并显示字段错误；
- 上一页/下一页按 `[1, pageCount]` clamp；
- 旋转使用标准化的 `0/90/180/270`；
- 适宽/适页根据当前容器和 PDF 原始 viewport 计算数值 zoom，P0 暂不新增持久化 `zoomMode`；
- 使用 `ResizeObserver` 重新计算当前适配命令，不在普通滚动时持续改写 zoom；
- 外部跳页只走已有 `currentPage` 控制路径；遵守 `agent-onboarding.md` 的 Reader 约束，不重新引入 drift recovery、`scrollIntoView` 追页或每次 offset 变化强制校正；
- 快捷键仅在 Reader 聚焦且不位于 input/textarea/dialog 时生效：`PageUp/Down` 翻页，`Home/End` 首末页，`Ctrl/Cmd + +/-` 缩放；旋转快捷键应进入统一快捷键表并避免输入冲突。

### 8.4 验收标准

- [ ] 可以直接跳到 1、任意中间页和最后一页；
- [ ] 非法页码不会导致空白、NaN 或越界；
- [ ] 旋转后 OCR bbox、引用缩略图和阅读状态一致；
- [ ] 缩放永远保持在统一的 60%–180%；
- [ ] 适宽/适页在窗口变化后结果可预测；
- [ ] 普通向上滚动不会被吸回之前的程序化跳转页；
- [ ] 重启后页码、数值 zoom 和 rotation 正确恢复。

### 8.5 必需测试

- 纯函数：页码 clamp、zoom clamp、rotation normalize、fit zoom；
- React：页码输入、按钮 disabled、快捷键输入冲突、旋转传递；
- Reader 回归：虚拟页窗口、OCR bbox 旋转、滚动不回弹；
- 桌面实测：1 页、500 页、横向扫描、不同页面尺寸混排 PDF。

---

## 9. P0-07：OCR Block 操作必须有键盘和语义等价路径

### 9.1 问题描述

OCR Block 命中层目前是带 `onClick` 的空 `div`。它没有 `role`、`tabIndex`、键盘处理和可读名称，因此引用、翻译、解释和 Lens 等核心操作只能由鼠标触发，屏幕阅读器也无法知道页面上有哪些可操作 Block。

P0 不要求立即实现完整 PDF Text Layer，但必须让已有 Block 能力可达。

### 9.2 建议的 DOM 结构

避免把操作按钮嵌套在另一个 button 内。每个 Block 使用：

```text
div.ocr-block-hit[role=group]
  button.ocr-block-target
  div.ocr-block-actions
    button 引用
    button 翻译
    button 解释
    button Lens
    button 复制
```

`ocr-block-target`：

- `aria-label` 包含页码、Block 类型、序号和截断文本；
- `aria-pressed` 表示当前是否被选中；
- Enter/Space 选择或取消选择；
- Escape 清除当前 Block，但不退出 Reader；
- 选中后操作区进入正常 Tab 顺序；
- 非文字 Block 使用“第 N 页图片/公式 Block”而不是空名称；
- 使用 `:focus-visible` 提供高对比焦点环；
- “复制”复制 canonical OCR 文本，并通过 P0-04 报告成功/失败。

### 9.3 验收标准

- [ ] 仅使用 Tab、Enter、Space、Escape 可以选择 Block 并执行所有现有动作；
- [ ] 屏幕阅读器能读出页码、类型、序号和文本摘要；
- [ ] 焦点状态与鼠标 selected 状态视觉一致但语义独立；
- [ ] 页面切换或 Block 消失时焦点落到可预测位置；
- [ ] 操作按钮没有嵌套交互元素或重复 Tab 停靠点；
- [ ] 鼠标点击、Guide 高亮、引用篮和旋转 bbox 行为不回归。

### 9.4 必需测试

扩展 `PdfReader.test.tsx`：可访问名称、Tab 顺序、Enter/Space、Escape、操作按钮、复制、非文本 Block、页面卸载后的焦点恢复，并保留现有虚拟化和 bbox 测试。

---

## 10. P0-08：统一 Overlay、焦点和 Escape 合同

### 10.1 问题描述

App 的全局 Escape 在 `showOperations` 等 Overlay 打开时直接 return，而 `OperationsDrawer` 本身没有 Escape 监听；因此任务抽屉无法用 Escape 关闭。Lightbox 缺少 dialog 语义和焦点圈定，某些菜单项和拖动分隔条仍是点击 `div` 或仅支持指针。

这类分散的 window listener 会形成顺序冲突：用户以为关闭当前浮层，实际可能清除 Block、关闭其他菜单或返回文库。

### 10.2 统一合同

建立共享 `ModalSurface` / `DrawerSurface` / `PopoverSurface` 和 Overlay stack：

- 只有最上层 Overlay 消费 Escape；
- 打开时记录触发元素；关闭后恢复焦点；
- Modal/Drawer 设置 `role="dialog"`、`aria-modal="true"` 和可见标题；
- 焦点被限制在最上层 Modal/Drawer；
- 背景使用 `inert`，不是仅 `aria-hidden`；
- 点击 scrim 是否关闭由组件显式配置；危险确认默认不允许误触关闭；
- closed Overlay 不应继续留在 Tab 顺序中；
- Lightbox 自己关闭，绝不能让 Escape 穿透并返回文库。

分隔条统一为 `role="separator"`：

- `tabIndex=0`；
- `aria-orientation`、`aria-valuemin/max/now`；
- 方向键按固定步长调整，Shift + 方向键使用大步长；
- 双击或 Home 恢复默认宽度。

### 10.3 验收标准

- [ ] Operations Drawer、Settings、Reset Confirm、Lightbox、Context、Thread 菜单都可用 Escape 关闭最上层；
- [ ] 关闭后焦点返回原触发按钮；
- [ ] Tab/Shift+Tab 不进入背景内容；
- [ ] 多层 Overlay 时一次 Escape 只关闭一层；
- [ ] Reader 和 Outline 分隔条可用键盘调整；
- [ ] 鼠标点击 scrim 的行为与危险级别一致。

### 10.4 必需测试

- Overlay stack 顺序、焦点 trap、焦点恢复、Escape 不穿透；
- Drawer 关闭后任务按钮重新获焦；
- Lightbox Escape 不改变 `viewMode`；
- separator 的方向键和边界 clamp；
- 使用 Testing Library `user.keyboard` 覆盖完整键盘路径。

---

## 11. P0-09：在支持窗口与系统缩放下保证关键操作可见

### 11.1 问题描述

`.settings-page` 使用 `width: min(100%, 1100px)`，同时增加水平 padding，却没有 `box-sizing: border-box`，会产生额外宽度。实测普通桌面窗口已经出现横向滚动和右侧内容裁切。Reader 顶栏也缺少真正的收纳断点，标题、页码、AI 操作、任务、设置和状态长期保持单行。

### 11.2 支持矩阵

P0 至少保证：

- 1440×900，100%；
- 1280×720，100%；
- 1100×720，100%；
- 1280×720，125% 系统缩放对应的有效视口；
- 1440×900，150% 系统缩放对应的有效视口。

### 11.3 建议修改

设置页：

- `.settings-page { width: 100%; max-width: 1100px; box-sizing: border-box; }`；
- 所有 grid 子项显式 `min-width: 0`；
- 只允许内容区纵向滚动，禁止 root 横向滚动；
- 窄宽度下 rail 缩为图标/短标签，必要时转为顶部 section switcher；
- 表单标签和控件由双列降为单列。

Reader 顶栏：

- 为标题设定可压缩宽度和省略号；
- 页码属于不可隐藏的核心区；
- OCR 当前状态保留，低频生成操作进入 AI 菜单；
- 导出、任务、设置进入系统菜单；
- 所有被收纳操作仍能通过键盘和可访问名称找到；
- 禁止通过 `display:none` 隐藏唯一入口。

### 11.4 验收标准

- [ ] 支持矩阵中的所有视口均没有 body/root 横向滚动；
- [ ] Workspace、Provider 保存、重置、Reader 页码和任务入口始终可达；
- [ ] 文字放大到 200% 时不出现按钮文字重叠；
- [ ] 收纳后的菜单可用键盘操作并正确恢复焦点；
- [ ] 中英文长文案、超长论文标题和长 Workspace 路径不会撑破布局。

### 11.5 必需测试

- 为关键页面建立固定视口截图回归；
- 增加长中文/英文、长路径和 200% 字体 fixture；
- 至少进行 Library 空态、Library 100 篇、Reader、Settings Workspace、Settings Models、Operations failed 六个页面的窄宽度检查。

---

## 12. 横向测试与发布门槛

### 12.1 自动化检查

每个 P0 合并前必须通过：

```text
npm test
npm run build
cargo test
```

建议在本轮加入：

- Testing Library 键盘与焦点回归；
- `vitest-axe` 或等价 axe 检查；
- Playwright 浏览器视口截图，用于设置和顶栏响应式；
- Tauri 桌面 smoke checklist，用于文件选择、文件锁、权限、备份和恢复。

### 12.2 黄金路径

发布候选必须完整走通：

1. 首次启动 → 设置中选择 Workspace；
2. 配置预设模型与自定义模型；
3. 导入 PDF → 打开 → 跳页 → 缩放 → 旋转；
4. 仅键盘选择 OCR Block，并执行引用/解释；
5. 制造 Provider 或网络失败 → 看见错误 → 进入任务中心 → 安全恢复；
6. 打开和关闭 Settings、Operations、Lightbox，验证焦点恢复；
7. 重启应用，验证阅读状态；
8. 对 Legacy Workspace 执行安全重置，并验证备份与故障回滚。

### 12.3 故障矩阵

至少覆盖：

- 无网络；
- 错误 API Key；
- Provider 429/5xx/超时；
- Workspace 只读；
- 文件被占用；
- 磁盘空间不足；
- 应用在任务运行中被终止；
- Legacy 目录含中文、长路径、符号链接和大量文件；
- 1 页、500 页、横向扫描和混合尺寸 PDF。

## 13. P0 Definition of Done

只有同时满足以下条件，P0 才算完成：

- [ ] Legacy 重置没有不可恢复的直接删除路径；
- [ ] 设置页可完成 Workspace 选择/更换和自定义模型保存；
- [ ] 所有主流程错误都有无需 hover 的可见出口；
- [ ] failed Job 有后端判定的安全恢复策略；
- [ ] Reader 具备跳页、适宽/适页、旋转和统一缩放边界；
- [ ] OCR Block、Overlay 和分隔条具备键盘等价路径；
- [ ] 支持视口没有关键控件裁切或 root 横向滚动；
- [ ] 新增与现有测试全部通过；
- [ ] 完成至少一次真实 Tauri 桌面回归，不只验证浏览器 preview；
- [ ] `feature-specs.md`、`v2-product-spec-2026-08.md`、`decisions.md` 中与真实行为冲突的合同已同步更新。

## 14. 建议的拆分方式

为降低回归风险，不建议把九项合并为一个大改动。推荐拆成可独立验收的变更组：

1. `workspace-reset-safety`：P0-01；
2. `settings-critical-paths`：P0-02、P0-03；
3. `notice-and-job-recovery`：P0-04、P0-05；
4. `reader-essential-controls`：P0-06；
5. `reader-keyboard-access`：P0-07；
6. `overlay-focus-contract`：P0-08；
7. `responsive-release-gate`：P0-09。

每组必须先补失败测试，再修改实现，再完成桌面 smoke；不得以“后续统一处理”为理由保留新的静默失败、裸 `div` 交互或不可恢复删除。
