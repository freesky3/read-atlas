# Markdown 渲染与归一化

讨论气泡、Brief、Lens、旁批、精读路线的正文都走同一套 Markdown。模型经常产出**挤在一行的有序列表**和**内侧带空格的 `**粗体**`**；历史成果已经落库，不能靠改 SQLite 修复显示。

权威实现：

| 层 | 何时跑 | 文件 | 作用 |
| --- | --- | --- | --- |
| 显示层 | 每次渲染 | `src/markdown.ts` `prepareMarkdown` → `MarkdownBody` | 修**已存储**和流式中的文本。不写回 DB。 |
| 写入层 | 生成 / 校验成功后 | `src-tauri/src/reading_artifact_module.rs` `normalize_markdown_field` | 修**新写入**的 Lens 与 Brief 字段。 |

改其中一层时，对照另一层的规则表，避免再分叉。现场样本只读用户工作区，**禁止**改 `<Workspace>/.read-desktop`。

## 1. 文件入口

| 要动的事 | 先看 |
| --- | --- |
| 显示流水线、粗体配对、挤在一起的列表 | `src/markdown.ts` |
| 渲染组件（remark-gfm + KaTeX） | `src/MarkdownBody.tsx` |
| 选区复制 GFM | 本页 **§9**；`clipboardMarkdownFromSelection` |
| 显示层单测 | `src/markdown.test.ts` |
| DOM 断言（`<strong>` / `<li>`） | `src/MarkdownBody.test.tsx` |
| 写入层归一化 | `reading_artifact_module.rs` `normalize_markdown_field` |
| Lens 生成时调用 | v1 在 validate 前执行 `normalize_lens_markdown_fields`；v2 不经过旧写入归一化，见 [Lens 合同](lens-generation.md) |
| Brief 生成时调用 | `lib.rs` `parse_orientation_pack` 对 takeaway / findings / evaluation 等字符串字段 |
| 公式预处理 | `preprocessLaTeX`（`\textbf{\kappa}` → `\boldsymbol{\kappa}`、控制字符恢复） |
| 只读调试脚本（会漂移，不是源） | `tools/inspect_lens.ts`、`tools/apply_normalize.ts` |

讨论、成果、旁批、精读都 `<MarkdownBody>{text}</MarkdownBody>`。不要再开第二条 Markdown 管线。

## 2. 显示层流水线

`prepareMarkdown(text, streaming)`：

1. 流式且公式未闭合 → **原样返回**（不要补 `**`、不要拆列表）。
2. `preprocessLaTeX`
3. 对非公式/非代码片段（`mapUnprotected`）：
   1. `decodeLiteralBreaks`：字面 `\n` / `\r` → 换行；**后面是字母则不动**（保护 `\nu`、`\rho`）。
   2. `repairInlineEmphasis`：按对处理 `**` / `__` / `*`。
   3. `splitPackedListItems`：把挤在一段里的 `1. …；2. …` 拆成行首列表。
4. `wrapBareInlineMath`（正文裸 `z_i`）
5. `linkPaperCitations`（`[p.N]` / `[block:…]`）

公式、`$$…$$`、`` `code` ``、围栏代码不参与粗体/列表修复。

## 3. 写入层流水线

`normalize_markdown_field`：

1. `decode_literal_escapes`：同上，**禁止**无条件 `replace("\\n")` / `replace("\\t")`（会把 `\nu`、`\text` 吃掉）。
2. 折叠连续空格 / Tab 为单个空格；**保留换行**（已是合法列表和缩进子弹）。
3. `repair_inline_bold`：去 `**` 内侧空白；奇数个 `**` 在下一换行或文末补闭合。按 **char** 迭代，不要字节切片。
4. `split_packed_list_items`：与显示层同一套「终止符或连续编号」规则。

调用点：

- Lens v1：`normalize_lens_markdown_fields` 走 `quickTakeaway.markdown`、`overallMarkdown` / `explanationMarkdown` / `summaryMarkdown`、`sections[].markdown`。
- Brief：`parse_orientation_pack` 走 takeaway、classification、context、backgroundAndProblem、coreMethod、findings、evaluation、futureWork、summary、researchQuestion、method、limitations。**keywords 不走。**

论文 Lens v2 的模型正文原样通过结构校验后发布，不调用上述写入归一化；真实换行与 LaTeX 不被此步骤重写。v1 和已有成果继续原有行为，显示层仍使用同一条 Markdown 管线。

写入层修的是**下一次生成**。用户库里 2026-08 的 Brief/Lens 仍靠显示层。

## 4. 规则目录

以后每发现一种坏输出，在本节加一行，并补测试。不要用「邻接汉字就插空格」这类无配对正则。

### 4.1 粗体 `**`

CommonMark：开标签右侧、闭标签左侧都不能紧贴空白。`**监督学习 **：` 不会变粗。

| ID | 输入 | 输出 | 不要做 |
| --- | --- | --- | --- |
| B1 | `**术语定义 **：` | `**术语定义**：` | 在闭合 `**` 和 `：`/`，` 之间插空格 |
| B2 | `这是** 粗体 **测试` | `这是 **粗体** 测试` | 把内侧空格留在星号里 |
| B3 | `实现**有监督…）**的生物` | `实现 **有监督…）** 的生物` | 只在**外侧**、且邻接汉字时补空格 |
| B4 | `执行**监督学习**：` | `执行 **监督学习**：` | 把这当成开标签（旧 `fixCjkEmphasis` 的坑） |
| B5 | 奇数个 `**`（非流式） | 在下一 `\n` 或文末补 `**` | 流式生成时补闭合（下一 chunk 可能带来闭合） |

`*` 行首 `* item` 是列表，不当斜体开标签。

### 4.2 有序列表

CommonMark 只认**行首**的 `1. `。`1. 甲；2. 乙` 会整段变成第一条。

拆行当且仅当标记（`1.` / `2、` / `1)` / 行首 `- `）满足：

- 已在行首，或
- 前一个非空白是终止符 `。！？!?；;：:，,` 或 U+FFFD，或
- 本段已经出现编号 `n`，当前是 `n+1`

| ID | 输入 | 输出 | 不要做 |
| --- | --- | --- | --- |
| L1 | `1. 甲；2. 乙；3. 丙` | 三个行首列表项 | 只靠提示词，不修历史数据 |
| L2 | `自测重点应包括：1. 甲；2. 乙` | 冒号后换行再 `1.` | 把 `：1.` 留在同一段 |
| L3 | `1. … $公式$ …；2. …` | 公式完整，`;2.` 仍拆行 | 在 `$…$` 里找 `1. ` |
| L4 | `See Figure 1. The results` | 不拆 | 凡是 `N. ` 就换行 |
| L5 | `in 1. Introduction and 2. Methods` | 不拆 | 用「本段已有 1. 则 2. 必拆」且不看是否真的在列清单 |
| L6 | 字面 `前言：\n1. 第一`（反斜杠 + n） | 真换行后再认列表 | `replace("\\n")` 无条件替换（会破坏 `\nu`） |

编号前面若是拉丁字母（`Figure 1.` / `Eq. 1.`），不当列表标记。不要在 `**…**` 内部拆列表。

子弹 `- ` 只在行首或终止符之后拆，避免把 `foo - bar` 变成列表。

### 4.3 字面转义

| ID | 输入 | 输出 | 不要做 |
| --- | --- | --- | --- |
| E1 | 字面 `\n1.`（后面不是字母） | 换行 + `1.` | |
| E2 | `$\nu$` / `\text{Target}` / `$\rho$` | 原样 | `replace("\\n")` / `replace("\\t")` / `replace("\\r")` |

`\t` 在写入层仅当后面不是字母时才变 Tab。

## 5. 现场样本（只读）

样本来自 `<Workspace>/.read-desktop\workspace.sqlite3`，**只读**。再修规则时优先用这些形状，不要编假数据替代。

### 5.1 Brief 列表挤在第一条

`artifacts.kind = 'brief'`，教材《突触可塑性》篇 `findings`：

```text
1. 监督学习中反向传播…复合脉冲；2. 强化学习中多巴胺神经元…动作选择；3. TD学习…；…；10. 常见易错点：…
```

同篇 `evaluation`：

```text
自测重点应包括：1. 数学推导：…；2. 机制解析：…；3. 计算与比较：…。
```

另一教材 Brief 把字面 `\n` 写进 JSON 字符串（Python repr 为 `'…：\\n1. **生命…'`），不是真换行。

对照：英文论文 Brief 已是 `1.\n2.\n3.`，拆行必须是 no-op。

### 5.2 Lens 星号露出来

`lens_figure` id `99c7e9ba-…`（生成时间 UTC `2026-08-23T16:32` = 北京 8 月 24 日 00:32）库里已经是：

```text
- **小脑（Cerebellum）**：执行**监督学习**，…
1. **无监督学习**：…
2. **强化学习**：教学信号为**标量（Scalar）评价**，…
3. **监督学习**：教学信号为**多维误差向量（Vector error）**。
```

界面曾显示 `执行**监督学习 **`、`**无监督学习 **：`。根因是显示层旧正则，不是库损坏。不要写回这条 artifact。

## 6. 测试锁

前端（必须 `--maxWorkers=1 --fileParallelism=false`）：

- `src/markdown.test.ts`：B1–B4、L1–L6、E1–E2、真实 Brief 含 `$…$` 的 findings；选区复制 C1–C5 见 **§9**。
- `src/MarkdownBody.test.tsx`：`**监督学习**，` → `<strong>`；挤在一起的 `1.；2.；3.` → 三个 `<li>`；copy 出 `1. ` 与 `$`/`$$`。

Rust：`cargo test --locked normalize_markdown`

- 原有 packed / bold / 乱码终止符 / live DB 平衡 `**`
- `normalize_markdown_preserves_latex_commands`
- `normalize_markdown_does_not_split_figure_prose`

改规则后两边都要绿。只改 Rust 测、不改 `prepareMarkdown` 测，历史 Brief 仍会坏。

## 7. 踩过的坑

1. **邻接汉字插空格会拆开合法粗体。**  
   旧 `fixCjkEmphasis` 把 `字**：` 当成开标签，得到 `**监督学习 **：`。闭合 `**` 前的空格让 micromark 放弃 emphasis。必须**先配对再决定外侧空格**。

2. **只在 Lens 写入时归一化修不好已有 Brief。**  
   `c13a354` 的 `normalize_lens_markdown_fields` 不跑 Brief，也不跑旧行。显示层必须自己拆列表。

3. **无条件替换 `\n`/`\t` 会吃 LaTeX。**  
   `\nu`、`\text`、`\rho`、`\tau` 都会中招。后面是 ASCII 字母就不要当转义。

4. **「看见 `N. ` 就换行」会拆正文。**  
   `Figure 1. The results`、`1. Introduction and 2. Methods` 不是列表。要终止符或连续编号，且编号前不是拉丁词。

5. **在 `$…$` 里拆列表会剪断公式。**  
   先 `mapUnprotected`。真实 findings 里 `;2.` 在公式之后、不在公式里。

6. **不要写回用户工作区。**  
   显示层修一次渲染即可。生成路径再写入干净文本。

7. **不要对整个 `src-tauri` 跑 `cargo fmt --`。**  
   会误格式化 `library_commands.rs` / `paper_module.rs` / `lib.rs` 里无关的 collection IPC。只 fmt 正在改的文件，或改完后把无关 diff checkout 掉。

8. **`is_list_terminator` 曾经是死代码。**  
   旧拆行器「不在行首就一律拆」，注释还写着「不要把 `?` 当终止符」，和实现不一致。现在终止符真的会用：`?` / `,` 只在**后面紧跟列表标记**时才拆，用来兜 `1. 甲?2. 乙` 这种 JSON 乱码，不会把句中问号拆碎。

9. **`tools/apply_normalize.ts` 会过期。**  
   那是写入层的 TypeScript 影子，不是权威实现。对行为有疑问以 `markdown.ts` + `normalize_markdown_field` 和它们的测试为准。

## 8. 追加新规则的清单

1. 从只读 SQLite 抽出原文（`repr` 能看见字面 `\n`）。
2. 在本节规则表加一行 ID，写输入 / 输出 / 禁止项。
3. `prepareMarkdown` 单测锁字符串；若涉及 DOM，补 `MarkdownBody.test.tsx`。
4. 若生成路径也会产出这种形状，补 `normalize_markdown_*` Rust 测，并确认 Brief 字段也会走到 `normalize_markdown_field`。
5. 保护 `$…$`、拉丁 `Figure N.`、LaTeX 命令。
6. 不改用户 DB 证明修复；刷新 WebView 即可验收历史成果。
7. 复制行为的新规则加在 **§9** 的 C 表，并补 `clipboardMarkdownFromSelection` 单测。

## 9. 选区复制（Ctrl+C → GFM）

**本小节是复制合同与踩坑日志。** 代码与本节冲突时先改本节，再改代码。显示层拆列表 / 修粗体仍看 **§2–§4**；不要把复制规则写进 §44 以免分叉。

下一个 agent：先读 **§9.1 合同摘要** 和 **§9.10 踩坑日志**，再动 `clipboardMarkdownFromSelection`。不要 `cloneContents()` + `textContent`，不要映射回 SQLite 原文，不要写 `text/html`。

### 9.1 合同摘要

| 项 | 事实 |
| --- | --- |
| 产品 | 划选讨论 / Brief 正文 / Lens（含 QA）后 `Ctrl+C`（含右键复制）得到 GFM |
| 来源 | **显示层 live DOM**，不是库里的生字符串，也不是 `prepareMarkdown` 源切片 |
| 剪贴板 | **只** `text/plain`。`preventDefault` 清掉浏览器默认 HTML。贴进 Word 看见 `**` 和 `$` 是合同 |
| 粒度 | **混合**：列表 marker 与 `$`/`$$` 碰到就补全外壳；条目正文、粗体、链接、行内代码按选区切 |
| 无选区 | **不**整篇复制，不劫持 composer / 就地 ✎ |
| 单击公式 | 仍复制该条 LaTeX；mouseup 时选区已非折叠则让路 |
| 单根 | 每个 `MarkdownBody` 的 `onCopy`（旁批、精读、设置样例、BlockCard 预览自动跟上） |
| 跨根 | 只挂 `.message-stream`、`.artifact-detail-body`、`.block-tab-generated` |
| KaTeX | `rehype-katex` **必须** `output: "htmlAndMathml"`，否则没有 `annotation` |

v1 语法：段落空行、嵌套 `ol`/`ul`（`start`+下标）、`$`/`$$`、`**`/`*`、`h1–h3`、行内/围栏代码、GFM 表（相交行 + 表头）、`[p.N]` / `[block:…]`。

明确不做：图片、任务列表、脚注、裸 HTML、无选区整篇按钮、复制为富文本、Orientation 表 UI 编成 GFM、术语表外层 `ul` 再套一层 `- `、PDF/OCR 块「复制」。

### 9.2 文件入口

| 要动的事 | 先看 |
| --- | --- |
| 序列化 / 拦截 | `src/markdown.ts` `clipboardMarkdownFromSelection`、`handleMarkdownCopyEvent` |
| 单根 + 单击公式 | `src/MarkdownBody.tsx` |
| KaTeX 输出 | 同上 `rehype-katex` `htmlAndMathml` |
| 讨论跨根 | `src/App.tsx` `.message-stream` `onCopy` |
| Brief / 成果跨根 | `src/ArtifactPanel.tsx` `.artifact-detail-body` |
| Lens 卡片 / QA 跨根 | `src/components/BlockCard.tsx` `.block-tab-generated`；`BlockCardQaDetail.tsx` `.artifact-detail-body` |
| 纯函数测 | `src/markdown.test.ts` `clipboard markdown from a live DOM selection` |
| 组件测 | `src/MarkdownBody.test.tsx`、`src/ArtifactPanel.test.tsx` 跨节 copy |
| jsdom MathML | `src/test/setup.ts` `getComputedStyle` 兜底 |
| 显示层（拆列表，不是复制） | 本页 §2–§4；`prepareMarkdown` |

`clipboardTextFromMarkdownSelection(root)` 是单根别名，内部仍走 `clipboardMarkdownFromSelection`。不要再实现第三条复制路径。

### 9.3 拦截

`handleMarkdownCopyEvent` 在以下 **全部** 成立时才 `preventDefault` + `setData("text/plain")`：

1. `event.defaultPrevented` 仍为 false（单根已处理后容器不得写第二次）。
2. 选区非折叠。
3. start/end **都不** 在 `input, textarea, select, [contenteditable]`。
4. 与容器内白名单相交：`.markdown-body` 或 `h3.artifact-section-title`。
5. 序列化 `trim` 后非空。抛错则放行浏览器默认。

### 9.4 混合粒度

| ID | 碰到 | 输出 | 不要做 |
| --- | --- | --- | --- |
| C1 | `li` | `1. ` / `- ` + **该条里划到的字** | 只出正文、丢掉 marker |
| C2 | `.katex` / `.katex-display` | 整段 `$tex$` / 独占行 `$$\ntex\n$$` | 从 LaTeX 中间切断；无 annotation 时给视觉字形补 `$` |
| C3 | `strong` / `em` / `a` / 行内 `code` | 只给划到的子串包语法 | 碰到就整颗 |
| C4 | 围栏 `pre` | 整块围栏（保留 `language-xxx`） | 切成半个 fence |
| C5 | `table` | 相交的行；有 `thead` 则始终带表头 | 丢掉列名 |

嵌套 `.katex` 在 `.katex-display` 内只序列化一次 display。块级空白文本节点丢掉，避免段间出现一串空行。

### 9.5 方言

有序 `1. `（用 `ol.start`，不要 `1、` / `1)`）；无序 `- `；嵌套每层 2 空格；紧列表（项之间无空行，列表与段落之间空一行）；`**bold**` / `*italic*`；ATX `#` / `##` / `###`；行内 `$x$` 内侧无空格；行间独占 `$$\ntex\n$$`。单击公式同一套。

引用药丸 `.markdown-citation` 用其 textContent（已是 `[p. 3]`），不要带 `#cite-` href。真链接只对 `http(s):` / `mailto:` 写成 `[text](href)`。流式 `.markdown-streaming` 按选区切源文本（此时还没有 `ol`/`katex`）。

### 9.6 跨根与白名单

多段 `.markdown-body` 与选中的 `h3.artifact-section-title` 按 DOM 顺序、空行拼接。标题原文照抄（含 emoji），写成 `### …`。

排除：版本胶囊、证据药丸、元信息脚、INDEX、模型名、`QUICK TAKEAWAY` 标签 span、图片 crop、建议追问 chip 外壳。`.artifact-quick-take > h3` **不是** `artifact-section-title`，v1 当普通字。跨两条讨论消息不加「用户/助手」前缀。`ul.artifact-string-list > li > .markdown-body` 每个根当段落，不要把外层铬列表编成 `- `。

### 9.7 如何追加（保持可扩展）

以后每发现一种坏复制，按顺序做，不要另开管线：

1. **新语法**：在 **§9.4** 加一行 `C6…`，写碰到 / 输出 / 禁止项。在 `clipboardMarkdownFromSelection` 的 walker 里实现。`src/markdown.test.ts` 用 **fixture HTML**（不必先走 ReactMarkdown）锁字符串。
2. **新表面**：只给「多根 + 小节标题」的容器加 `onCopy={(e) => handleMarkdownCopyEvent(e, e.currentTarget)}`。单根 `MarkdownBody` 已经覆盖。新白名单 class 必须写进 **§9.1 / §9.6**，防止把铬吞进剪贴板。
3. **新坑**：只在 **§9.10 表末追加一行**，不要改写旧行、不要把复制坑混进 §7（那是显示层归一化）。
4. 改完跑：`npx vitest run --maxWorkers=1 --fileParallelism=false src/markdown.test.ts src/MarkdownBody.test.tsx src/ArtifactPanel.test.tsx`。动 `MarkdownBody` / KaTeX 输出后再跑全量前端测（MathML 会撞 jsdom，见 P3）。
5. 不要写回用户工作区 SQLite。刷新 WebView 即可验收历史成果。

### 9.8 测试锁

- `src/markdown.test.ts`：C1–C5、跨根 `###`、排除铬、编辑器不拦截、流式源文本、折叠选区 `null`、只写 `text/plain`。
- `src/MarkdownBody.test.tsx`：`prepareMarkdown` 拆开的列表 copy 出 `1.`；真实 KaTeX 出 `$` / 独占行 `$$`；单击让路；无 `text/html`。
- `src/ArtifactPanel.test.tsx`：Brief 跨节 copy 含 `### 📊 主要发现与结论`。跨节测必须用 **Brief**（走 `.artifact-detail-body`）。Lens 在选区 Tab 下走 `BlockCardStream`，没有这个 class。

jsdom 的 `ClipboardEvent.clipboardData` 经常是空的，测试里给 `Event("copy")` 挂一个假 `setData`。选区用 `document.createRange` + `getSelection`，与现有公式测相同。

### 9.9 验收（实机，jsdom 替代不了）

1. Brief「主要发现」划三条有序列表 → 记事本里有 `1.` `2.` `3.`。
2. 讨论里划一段含行内公式 → `$...$`；划行间公式 → 独占行 `$$`。
3. 只扫公式可见字形再 `Ctrl+C` → 仍是完整 `$`/`$$`，不是 `E=mc2`。
4. 从小节标题拖到下一节正文 → 剪贴板有 `### 标题` 和两节正文，没有「由 xx 生成」。
5. 输入胶囊里 `Ctrl+C` 仍复制正在写的字。
6. 单击公式（没划选）仍复制 LaTeX；划开后再点公式不改剪贴板。

### 9.10 踩坑日志（只追加，不删旧条）

改代码时若再踩坑，在表末加一行：日期、症状、不要再做、正确做法。

| 日期 | ID | 症状 | 不要再做 | 正确做法 |
| --- | --- | --- | --- | --- |
| 2026-09-04 | P1 | 有序列表 `Ctrl+C` 没有 `1.` | `cloneContents()` 后取 `textContent` | 编号是 CSS `::marker`，不在 DOM 文本里。walker 碰到 `li` 自己写 `1. ` / `- `（C1） |
| 2026-09-04 | P2 | 半选公式没有 `$` | 假定选区 clone 里带得走 `annotation` | 可见字形在 `.katex-html`，LaTeX 在兄弟 MathML。对 **live** `.katex` / `.katex-display` 读 annotation，碰到就整段（C2） |
| 2026-09-04 | P3 | 真实 `MarkdownBody` copy 出 `E=mc2` / `∫f`，单击公式也不写剪贴板 | `rehype-katex` `output: "html"` | 纯 HTML 没有 `annotation[encoding="application/x-tex"]`。必须 `htmlAndMathml`。无 annotation 时输出视觉字、**不要**伪造 `$E=mc2$` |
| 2026-09-04 | P4 | `htmlAndMathml` 后 `BlockCardStream` QA 测在 `getComputedStyle` 炸 | 为了测绿改回 `output: "html"` | jsdom 对 `<math>` 的 stylesheet `cssRules` 为 null。在 `src/test/setup.ts` 兜住 `getComputedStyle`，生产仍用 MathML |
| 2026-09-04 | P5 | Brief 跨「主要发现」和列表，复制又退回默认 | 只把 `onCopy` 挂在单个 `MarkdownBody` | 每节一个根，小标题是外面的 `h3.artifact-section-title`。`copy` 提到成果/讨论容器，按 DOM 顺序拼白名单 |
| 2026-09-04 | P6 | ArtifactPanel 跨节 copy 测找不到 `.artifact-detail-body` | 用 Lens fixture 测跨节 | 当前 Lens 在选区 Tab 走 `BlockCardStream`。跨节测用 **Brief** findings + evaluation |
| 2026-09-04 | P7 | 拖选公式后剪贴板先变成只有公式 | 单击与 `Ctrl+C` 抢写 | mouseup 时选区非折叠则单击不 `writeText` |
| 2026-09-04 | P8 | Word / 微信富文本里列表编号又丢了 | 同时 `setData("text/html", 渲染结果)` | 这些软件优先吃 HTML，`::marker` 再次丢失。只留 `text/plain` |
| 2026-09-04 | P9 | 成果详情里 ✎ 编辑框 `Ctrl+C` 被改写成 GFM | 容器 `onCopy` 无条件 `preventDefault` | start/end 落在 `input` / `textarea` / `contenteditable` 则不接管 |
| 2026-09-04 | P10 | 行间公式 copy 成段中 `$\\int f$` | 测试源写成 `$$\\int f$$` 当行间 | remark-math 要 `$$\ntex\n$$` 才出 `.katex-display`。方言与测试源一致 |

### 9.11 变更日志（只追加）

| 日期 | 做了什么 |
| --- | --- |
| 2026-09-04 | v1：live DOM → GFM；混合粒度 C1–C5；跨根三容器；`htmlAndMathml`；单击让路；只 `text/plain` |
