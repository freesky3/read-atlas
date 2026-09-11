# 聊天记录跳转刻度轴、38px 极简顶栏与增强型液态玻璃重构设计规范

## 1. 背景与目标

在严肃科学文献阅读与 AI 研讨场景中，用户需要：
1. **最大化垂直与水平研讨视野**：消除右侧讨论区原先层叠的 3 层头部控件（`~144px`），精简为单行 **38px**，消除非讨论区域的冗余占位。
2. **长篇问答秒级跳转（Chat Mini-Map Navigator）**：在消息列表旁引入轻量垂直刻度轴，悬停时即时展示高质感磨砂预览卡片（问题标题 + 回答摘要），点击平滑定位并高亮。
3. **Thread 分支内联重命名**：在 38px 顶栏中动态展示当前 Thread 标题，支持双击直接内联重命名并持久化。
4. **增强型液态玻璃材质升级（Enhanced Liquid Glass Refraction）**：参考多层内高光（4-layer Inset Highlight）、45° 对角高光折射（Diagonal Sheen）与深浅立体边缘，全面升级悬浮顶岛、胶囊按钮、跳转刻度 Tooltip、提问气泡与输入框。

---

## 2. 界面与交互架构

### 2.1 整体拓扑图

```text
Focus Reader 右侧研讨区新结构：
┌──────────────────────────────── 38px 单行极简多功能顶栏 ──────────────────────────────┐
│ [讨论] [成果(N)] │ 🏷️ [Thread 名称 ▾ (双击编辑)] │ ＋(新分支) 🌳(分支树) 📥(归档) │
├──────────────────────────────────────────────────────────────────┬────────────────────┤
│                                                                  │ ▎(刻度 #1 Q&A)     │
│ 消息流 (Message Stream - 独立平滑滚动)                            │ ▎(刻度 #2 Q&A)     │
│  - User: 增强型液态玻璃右侧圆角气泡                                │ █(活动态刻度 #3)   │
│  - Assistant: 全宽无框流式排版                                    │   [悬浮浮现预览卡]  │
│                                                                  │ ▎(刻度 #4 Q&A)     │
├──────────────────────────────────────────────────────────────────┴────────────────────┤
│ 磨砂玻璃输入框 Composer (附带状态小字与快捷发送按钮)                                      │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 核心组件与交互规范

### 3.1 38px 极简多功能顶栏与 Thread 双击重命名 (`CompactRailHeader`)

1. **结构集成**：
   - **左侧**：`[讨论]` / `[阅读成果 (N)]` 药丸切换器。
   - **中央**：Thread 名称胶囊，常态展示当前 Thread 标题（如 `Main discussion` 或自定义名称），右侧带有 `▾` 下拉指示符。
   - **右侧**：`＋`（新建分支）、`🌳`（全屏对话树视图）、`📥`（归档分支）。
2. **双击重命名交互**：
   - **单击小三角 `▾`**：弹出分支列表浮层，可点击切换到其他分支。
   - **双击标题文本（Double Click）**：标题立即替换为 `<input autoFocus ... />` 文本输入框。
   - **保存与取消**：按 `Enter` 键或失焦（`onBlur`）自动保存并触发 `onRenameThread(threadId, newTitle)`；按 `Esc` 键取消修改。
   - **长文本截断**：超过可用宽度的标题使用 `text-overflow: ellipsis` 截断，并在悬停时展示完整名称。

### 3.2 聊天记录跳转刻度轴 (`ChatTimelineNavigator`)

1. **定位与排布**：
   - 依附于消息流右侧边缘，宽度固定为 18px，不压缩正文可用宽度。
   - 从 `messages` 中自动提取所有用户提问（`message.role === 'user'`），每轮 Q&A 映射为一个弹性等比刻度（Tick）。
2. **悬停预览卡片（Hover Tooltip）**：
   - 鼠标悬停在刻度上时，在刻度左侧即时浮现高保真液态玻璃预览卡片（宽度 `260px`）。
   - 卡片内容：
     - 头部：轮次序号与提问时间（例如 `#2 · 01:22`）。
     - 正文：粗体用户问题标题。
     - 底部：AI 回答前 2 行纯文本摘要（去 Markdown 标记）。
3. **点击定位与脉冲动效**：
   - 点击刻度线，调用 `targetElement.scrollIntoView({ behavior: 'smooth', block: 'center' })` 平滑滚动。
   - 目标消息触发一次轻量的高亮光晕脉冲动画（`target-highlight`）。
4. **活动刻度追踪（Active State）**：
   - 使用轻量滚动监听器计算当前处于视口中心的消息轮次，对应刻度高亮为翡翠绿（`#16a34a`）。

---

## 4. 增强型液态玻璃材质设计规范 (Enhanced Liquid Glass Tokens)

基于参考样式与 Apple visionOS 拟态美学，制定以下分层参数：

### 4.1 核心 CSS 变量与阴影混合

```css
:root, [data-theme="liquid-light"] {
  /* 增强型液态玻璃立体高光与多层内折射 */
  --liquid-glass-shadow:
    inset 2px -2px 1.5px -1px rgba(255, 255, 255, 0.95),
    inset -2px 2px 1.5px -1px rgba(255, 255, 255, 0.95),
    inset 6px -6px 2px -6px rgba(255, 255, 255, 0.6),
    inset -6px 6px 2px -6px rgba(255, 255, 255, 0.6),
    inset 0 0 2px rgba(15, 23, 42, 0.08),
    0 4px 12px rgba(15, 23, 42, 0.05);

  --liquid-glass-hover-shadow:
    inset 2px -2px 2px -1px rgba(255, 255, 255, 1),
    inset -2px 2px 2px -1px rgba(255, 255, 255, 1),
    inset 8px -8px 3px -6px rgba(255, 255, 255, 0.75),
    inset -8px 8px 3px -6px rgba(255, 255, 255, 0.75),
    inset 0 0 2px rgba(15, 23, 42, 0.12),
    0 8px 24px rgba(15, 23, 42, 0.1);

  --liquid-glass-sheen: linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.75) 0%,
    transparent 22%,
    transparent 78%,
    rgba(255, 255, 255, 0.75) 100%
  );
}

[data-theme="liquid-dark"] {
  --liquid-glass-shadow:
    inset 1px -1px 1px 0 rgba(255, 255, 255, 0.25),
    inset -1px 1px 1px 0 rgba(255, 255, 255, 0.25),
    inset 4px -4px 2px -4px rgba(255, 255, 255, 0.15),
    inset 0 0 2px rgba(0, 0, 0, 0.6),
    0 6px 20px rgba(0, 0, 0, 0.45);

  --liquid-glass-sheen: linear-gradient(
    45deg,
    rgba(255, 255, 255, 0.2) 0%,
    transparent 22%,
    transparent 78%,
    rgba(255, 255, 255, 0.2) 100%
  );
}
```

### 4.2 应用组件范围

1. **悬浮顶岛（Header Island）**：应用 `--liquid-glass-shadow` 与对角高光。
2. **胶囊按钮与药丸切换器**：应用 45° Sheen 边缘与立体按压缩放动效。
3. **用户消息气泡（User Question Bubble）**：采用液态玻璃磨砂折射卡片，右侧靠齐。
4. **跳转轴 Tooltip 预览卡片**：`backdrop-filter: blur(28px)` + 多层内高光阴影。
5. **输入框（Composer）**：高质感磨砂圆角容器，聚焦时光晕平滑过渡。

---

## 5. 实现与测试验证

### 5.1 修改与新建文件列表

- `[NEW] src/components/ChatTimelineNavigator.tsx`: 跳转刻度轴组件。
- `[NEW] src/components/ChatTimelineNavigator.test.tsx`: 跳转刻度轴单元测试。
- `[MODIFY] src/App.tsx`: 集成 38px 顶栏、Thread 双击重命名逻辑与 `ChatTimelineNavigator`。
- `[MODIFY] src/styles.css`: 注入增强型液态玻璃 Token、单行顶栏样式、跳转刻度轴与 Tooltip 样式。

### 5.2 验证标准

1. **单元与集成测试**：Vitest 串行全量通过（16+ 测试文件，新增对刻度解析、Hover Tooltip、双击重命名逻辑的覆盖）。
2. **构建与类型检查**：`tsc -b && vite build` 0 报错。
3. **后端无破坏**：`cargo test --locked` 58/58 测试全通。
