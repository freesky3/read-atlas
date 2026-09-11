import { afterEach, describe, expect, it } from "vitest";
import {
  clipboardMarkdownFromSelection,
  clipboardTextFromMarkdownSelection,
  decodeLiteralBreaks,
  ensureDisplayMath,
  ensureInlineMath,
  formattedLatexFromKatexNode,
  handleMarkdownCopyEvent,
  hasIncompleteMath,
  linkPaperCitations,
  parsePaperCitationHref,
  prepareMarkdown,
  preprocessLaTeX,
  replaceKatexWithDelimiters,
  splitPackedListItems,
  wrapBareInlineMath,
} from "./markdown";

describe("markdown citations and math", () => {
  it("turns page and block markers into local links", () => {
    expect(linkPaperCitations("See [p. 3] and [block: abc-1].")).toBe(
      "See [[p. 3]](#cite-page-3) and [[block: abc-1]](#cite-block-abc-1).",
    );
    expect(parsePaperCitationHref("#cite-page-12")).toEqual({
      kind: "page",
      page: 12,
    });
    expect(parsePaperCitationHref("#cite-block-block-9")).toEqual({
      kind: "block",
      blockId: "block-9",
    });
  });

  it("normalizes LaTeX delimiters and skips broken streaming math", () => {
    expect(preprocessLaTeX("Energy \\(E=mc^2\\)")).toContain("$E=mc^2$");
    expect(hasIncompleteMath("wait $E=mc^")).toBe(true);
    expect(prepareMarkdown("wait $E=mc^", true)).toBe("wait $E=mc^");
    expect(prepareMarkdown("See [p. 2]", false)).toContain("#cite-page-2");
  });

  it("turns rendered KaTeX back into $ and $$ for copy", () => {
    const root = document.createElement("div");
    root.innerHTML = [
      "<p>Energy ",
      '<span class="katex"><annotation encoding="application/x-tex">E=mc^2</annotation><span>E=mc2</span></span>',
      "</p>",
      '<span class="katex-display"><span class="katex"><annotation encoding="application/x-tex">\\int f</annotation><span>integral</span></span></span>',
    ].join("");
    replaceKatexWithDelimiters(root);
    expect(root.textContent).toContain("$E=mc^2$");
    expect(root.textContent).toContain("$$\n\\int f\n$$");
    expect(root.querySelector(".katex")).toBeNull();
  });

  it("formats click-to-copy display math as a standalone $$ block", () => {
    const root = document.createElement("div");
    root.innerHTML =
      '<span class="katex-display"><span class="katex"><annotation encoding="application/x-tex">\\int f</annotation></span></span>';
    const hit = root.querySelector(".katex-display");
    expect(hit && formattedLatexFromKatexNode(hit)).toBe("$$\n\\int f\n$$");
  });

  it("rebuilds the current selection as plain text plus LaTeX", () => {
    const root = document.createElement("div");
    root.innerHTML =
      '<p>See <span class="katex"><annotation encoding="application/x-tex">x+y</annotation>xy</span> now</p>';
    document.body.append(root);
    const range = document.createRange();
    range.selectNodeContents(root);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    expect(clipboardTextFromMarkdownSelection(root, selection)).toContain(
      "$x+y$",
    );
    root.remove();
  });

  it("wraps bare symbol identifiers as inline math", () => {
    expect(ensureInlineMath("h_i(t)")).toBe("$h_i(t)$");
    expect(ensureInlineMath("W_{ij} or w(z_i, z_j)")).toBe(
      "$W_{ij}\\text{ or }w(z_i, z_j)$",
    );
    expect(ensureInlineMath("$\\tau$")).toBe("$\\tau$");
    expect(ensureInlineMath("N")).toBe("N");
    expect(ensureDisplayMath("\\int f(x)\\,dx")).toBe(
      "$$\\int f(x)\\,dx$$",
    );
  });

  it("wraps bare latex tokens inside ordinary prose", () => {
    expect(
      wrapBareInlineMath(
        "from presynaptic neuron j (at location z_j) to postsynaptic neuron i (at location z_i).",
      ),
    ).toBe(
      "from presynaptic neuron j (at location $z_j$) to postsynaptic neuron i (at location $z_i$).",
    );
    expect(wrapBareInlineMath("time constant $\\tau$ and h_i(t)")).toBe(
      "time constant $\\tau$ and $h_i(t)$",
    );
    expect(wrapBareInlineMath("See [p. 2] and N neurons.")).toBe(
      "See [p. 2] and N neurons.",
    );
    expect(prepareMarkdown("location z_i", false)).toContain("$z_i$");
  });

  it("normalizes and recovers corrupted LaTeX formulas and Greek macros", () => {
    expect(
      preprocessLaTeX(
        "\\frac{\\mathrm{d}}{\\mathrm{d}t}\\textbf{\\kappa}(t) = -\\frac{1}{\\tau}\\textbf{\\kappa}(t)",
      ),
    ).toBe(
      "\\frac{\\mathrm{d}}{\\mathrm{d}t}\\boldsymbol{\\kappa}(t) = -\\frac{1}{\\tau}\\boldsymbol{\\kappa}(t)",
    );
    expect(
      preprocessLaTeX("\\mathbf{\\kappa} and \\bold{\\alpha} and \\text{\\beta}"),
    ).toBe("\\boldsymbol{\\kappa} and \\boldsymbol{\\alpha} and \\beta");
    expect(preprocessLaTeX("\x08eta and \x0crac{1}{2}")).toBe(
      "\\beta and \\frac{1}{2}",
    );
  });

  it("fixes CJK emphasis where punctuation is adjacent to asterisks", () => {
    expect(
      prepareMarkdown(
        "该图展示了小脑回路实现**有监督学习（Supervised Learning）**的生物学神经架构。",
        false,
      ),
    ).toContain(
      "实现 **有监督学习（Supervised Learning）** 的生物学神经架构。",
    );

    expect(
      prepareMarkdown("这是**（重要）**提示", false),
    ).toContain("这是 **（重要）** 提示");

    expect(
      prepareMarkdown("这是**【重点】**内容", false),
    ).toContain("这是 **【重点】** 内容");
  });

  it("fixes emphasis with irregular spaces inside asterisks or underscores", () => {
    expect(prepareMarkdown("1. **核心术语定义 **:", false)).toContain(
      "**核心术语定义**:",
    );
    expect(prepareMarkdown("1. **核心术语定义 **:", false)).not.toContain(
      "**核心术语定义 **",
    );
    expect(prepareMarkdown("3. **与上下文的逻辑连接 **: 本段紧随", false)).toContain(
      "**与上下文的逻辑连接**:",
    );
    expect(prepareMarkdown("这是** 粗体 **测试", false)).toContain(
      "这是 **粗体** 测试",
    );
    expect(prepareMarkdown("这是* 斜体 *测试", false)).toContain(
      "这是 *斜体* 测试",
    );
    expect(prepareMarkdown("这是__ 粗体 __测试", false)).toContain(
      "这是 __粗体__ 测试",
    );
  });

  it("does not insert spaces inside closing ** before punctuation", () => {
    const lens = prepareMarkdown(
      "- **小脑（Cerebellum）**：执行**监督学习**，其核心目标是最小化误差。",
      false,
    );
    expect(lens).toContain("**小脑（Cerebellum）**：");
    expect(lens).toContain("**监督学习**，");
    expect(lens).not.toContain("**监督学习 **");
    expect(lens).not.toContain("**小脑（Cerebellum） **");

    const items = prepareMarkdown(
      "1. **无监督学习**：无外部教学信号。\n2. **强化学习**：教学信号为**标量（Scalar）评价**，仅告知优劣。\n3. **监督学习**：教学信号为**多维误差向量（Vector error）**。",
      false,
    );
    expect(items).toContain("**无监督学习**：");
    expect(items).toContain("**强化学习**：");
    expect(items).toContain("**标量（Scalar）评价**");
    expect(items).toContain("**监督学习**：");
    expect(items).toContain("**多维误差向量（Vector error）**");
    expect(items).not.toContain("**无监督学习 **");
    expect(items).not.toContain("**强化学习 **");
    expect(items).not.toContain("**监督学习 **");
  });

  it("splits packed numbered items glued with Chinese terminators", () => {
    const packed =
      "1. 监督学习中反向传播利用链式法则计算梯度，生物学上对应小脑误差信号；2. 强化学习中多巴胺神经元的相位放电定量表征奖赏预测误差；3. TD学习通过Bellman方程解释多巴胺响应迁移；10. 常见易错点：混淆短期与长期可塑性。";
    const out = prepareMarkdown(packed, false);
    expect(out).toMatch(/^1\. 监督学习/m);
    expect(out).toContain("\n2. 强化学习");
    expect(out).toContain("\n3. TD学习");
    expect(out).toContain("\n10. 常见易错点");
  });

  it("splits a live Brief findings paragraph that embeds inline math", () => {
    const findings =
      "1. 监督学习中反向传播利用链式法则（$\\frac{\\partial \\mathcal{L}}{\\partial \\mathbf{r}^k} = [\\mathbf{w}^{k+1}]^T \\frac{\\partial \\mathcal{L}}{\\partial \\mathbf{r}^{k+1}}$）计算梯度，生物学上对应小脑中下橄榄核攀援纤维传递的误差信号诱发浦肯野细胞复合脉冲；2. 强化学习中多巴胺神经元的相位放电定量表征奖赏预测误差（RPE: $\\delta_t = r_t - V_t(s)$），基底节通过D1（直接/Go通路易化运动）与D2（间接/NoGo通路抑制运动）通路的对立多巴胺调节实现动作选择；3. TD学习通过Bellman方程解释了多巴胺响应从非条件刺激（US）向条件刺激（CS）的时间回溯迁移。";
    const out = prepareMarkdown(findings, false);
    expect(out).toContain("\n2. 强化学习");
    expect(out).toContain("\n3. TD学习");
    expect(out).toContain("$\\frac{\\partial \\mathcal{L}}{\\partial \\mathbf{r}^k}");
    expect(out.split("\n").filter((line) => /^\d+\. /.test(line))).toHaveLength(
      3,
    );
  });

  it("starts a list after an intro colon and keeps nested math intact", () => {
    const evaluation =
      "自测重点应包括：1. 数学推导：由误差函数梯度下降推导Delta学习规则；2. 机制解析：结合NMDA受体说明钙模型；3. 计算与比较：对比无界实数突触与二值突触。";
    const out = prepareMarkdown(evaluation, false);
    expect(out).toContain("自测重点应包括：");
    expect(out).toContain("\n1. 数学推导");
    expect(out).toContain("\n2. 机制解析");
    expect(out).toContain("\n3. 计算与比较");
  });

  it("decodes leftover backslash-n line breaks without eating LaTeX commands", () => {
    expect(decodeLiteralBreaks("前言：\\n1. 第一项")).toBe("前言：\n1. 第一项");
    expect(prepareMarkdown("前言：\\n1. 第一项\\n2. 第二项", false)).toContain(
      "\n1. 第一项",
    );
    expect(prepareMarkdown("前言：\\n1. 第一项\\n2. 第二项", false)).toContain(
      "\n2. 第二项",
    );
    expect(prepareMarkdown("the $\\nu$ value and $\\text{Target}$", false)).toContain(
      "$\\nu$",
    );
    expect(prepareMarkdown("the $\\nu$ value and $\\text{Target}$", false)).toContain(
      "\\text{Target}",
    );
  });

  it("does not treat Figure/Eq prose numbers as packed list items", () => {
    expect(
      prepareMarkdown("See Figure 1. The results confirm the claim.", false),
    ).not.toContain("\n1. ");
    expect(
      prepareMarkdown(
        "described in 1. Introduction and 2. Methods of the paper.",
        false,
      ),
    ).not.toContain("\n1. ");
    expect(
      prepareMarkdown(
        "described in 1. Introduction and 2. Methods of the paper.",
        false,
      ),
    ).not.toContain("\n2. ");
  });

  it("splitPackedListItems is a no-op for already valid lists", () => {
    const already = "1. foo\n2. bar\n3. baz";
    expect(splitPackedListItems(already)).toBe(already);
  });
});

function mountHtml(html: string) {
  const root = document.createElement("div");
  root.innerHTML = html;
  document.body.append(root);
  return root;
}

function selectNode(node: Node) {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return selection!;
}

function selectText(node: Text, start: number, end: number) {
  const range = document.createRange();
  range.setStart(node, start);
  range.setEnd(node, end);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return selection!;
}

function firstText(from: ParentNode, includes: string) {
  const walker = document.createTreeWalker(from, NodeFilter.SHOW_TEXT);
  let node: Node | null;
  while ((node = walker.nextNode())) {
    if (node.textContent?.includes(includes)) return node as Text;
  }
  throw new Error(`text not found: ${includes}`);
}

function dispatchCopy(target: EventTarget) {
  const data: Record<string, string> = {};
  const setData = (type: string, value: string) => {
    data[type] = value;
  };
  const event = new Event("copy", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: { setData, getData: (type: string) => data[type] ?? "" },
  });
  target.dispatchEvent(event);
  return { event, data };
}

describe("clipboard markdown from a live DOM selection", () => {
  afterEach(() => {
    document.body.replaceChildren();
    window.getSelection()?.removeAllRanges();
  });

  it("emits GFM ordered list markers that CSS ::marker would hide", () => {
    const root = mountHtml(
      '<div class="markdown-body"><ol><li>甲</li><li>乙</li></ol></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const text = clipboardMarkdownFromSelection(selectNode(body), body);
    expect(text).toContain("1. 甲");
    expect(text).toContain("2. 乙");
    expect(text).not.toBe("甲乙");
  });

  it("honors ol start for copied numbers", () => {
    const root = mountHtml(
      '<div class="markdown-body"><ol start="3"><li>丙</li><li>丁</li></ol></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(clipboardMarkdownFromSelection(selectNode(body), body)).toBe(
      "3. 丙\n4. 丁",
    );
  });

  it("indents nested unordered lists by two spaces", () => {
    const root = mountHtml(
      '<div class="markdown-body"><ul><li>外<ul><li>内</li></ul></li></ul></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(clipboardMarkdownFromSelection(selectNode(body), body)).toBe(
      "- 外\n  - 内",
    );
  });

  it("keeps the list marker but only the selected slice of that item", () => {
    const root = mountHtml(
      '<div class="markdown-body"><ol><li>ABCDEF</li><li>XYZ</li></ol></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const text = clipboardMarkdownFromSelection(
      selectText(firstText(body, "ABCDEF"), 3, 6),
      body,
    );
    expect(text).toBe("1. DEF");
    expect(text).not.toContain("XYZ");
  });

  it("copies a whole inline $ formula even when only the glyph span is selected", () => {
    const root = mountHtml(
      [
        '<div class="markdown-body"><p>See ',
        '<span class="katex"><annotation encoding="application/x-tex">E=mc^2</annotation>',
        '<span class="katex-html">E=mc2</span></span>',
        " now</p></div>",
      ].join(""),
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const glyphs = firstText(body, "E=mc2");
    expect(clipboardMarkdownFromSelection(selectText(glyphs, 0, 5), body)).toBe(
      "$E=mc^2$",
    );
  });

  it("copies display math as a standalone $$ block", () => {
    const root = mountHtml(
      [
        '<div class="markdown-body">',
        '<span class="katex-display"><span class="katex">',
        '<annotation encoding="application/x-tex">\\int f</annotation>',
        "<span>integral</span></span></span>",
        "</div>",
      ].join(""),
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(clipboardMarkdownFromSelection(selectNode(body), body)).toBe(
      "$$\n\\int f\n$$",
    );
  });

  it("does not wrap KaTeX glyphs in $ when the annotation is missing", () => {
    const root = mountHtml(
      '<div class="markdown-body"><p><span class="katex"><span class="katex-html">E=mc2</span></span></p></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const text = clipboardMarkdownFromSelection(selectNode(body), body);
    expect(text ?? "").not.toContain("$E=mc2$");
    expect(text ?? "").not.toContain("$E=mc^2$");
    expect(text).toContain("E=mc2");
  });

  it("wraps only the selected slice of a strong node", () => {
    const root = mountHtml(
      '<div class="markdown-body"><p>aa<strong>监督</strong>bb</p></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(
      clipboardMarkdownFromSelection(
        selectText(firstText(body, "监督"), 0, 1),
        body,
      ),
    ).toBe("**监**");
  });

  it("keeps the header row when a GFM table body row is selected", () => {
    const root = mountHtml(
      [
        '<div class="markdown-body"><table>',
        "<thead><tr><th>A</th><th>B</th></tr></thead>",
        "<tbody><tr><td>1</td><td>2</td></tr><tr><td>3</td><td>4</td></tr></tbody>",
        "</table></div>",
      ].join(""),
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const row = [...body.querySelectorAll("tr")].find((tr) =>
      tr.textContent?.includes("3"),
    ) as HTMLElement;
    const text = clipboardMarkdownFromSelection(selectNode(row), body);
    expect(text).toContain("| A | B |");
    expect(text).toContain("| --- | --- |");
    expect(text).toContain("| 3 | 4 |");
    expect(text).not.toContain("| 1 | 2 |");
  });

  it("copies citation pills as [p.N] without a href", () => {
    const root = mountHtml(
      '<div class="markdown-body"><p>See <button class="markdown-citation" type="button">[p. 3]</button>.</p></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const text = clipboardMarkdownFromSelection(selectNode(body), body);
    expect(text).toContain("[p. 3]");
    expect(text).not.toContain("href");
    expect(text).not.toContain("#cite");
  });

  it("joins section titles and sibling markdown roots in DOM order", () => {
    const root = mountHtml(
      [
        "<div>",
        '<h3 class="artifact-section-title">📊 主要发现与结论</h3>',
        '<div class="markdown-body"><ol><li>甲</li></ol></div>',
        '<h3 class="artifact-section-title">⚖️ 审辨式评估</h3>',
        '<div class="markdown-body"><p>乙</p></div>',
        "</div>",
      ].join(""),
    );
    const text = clipboardMarkdownFromSelection(selectNode(root), root);
    expect(text).toBe(
      "### 📊 主要发现与结论\n\n1. 甲\n\n### ⚖️ 审辨式评估\n\n乙",
    );
  });

  it("does not copy chrome such as meta footers or action pills", () => {
    const root = mountHtml(
      [
        "<div>",
        '<div class="markdown-body"><p>正文</p></div>',
        '<div class="artifact-detail-meta-footer">由 Gemini 生成 · 23:39</div>',
        '<button type="button">带到讨论中</button>',
        "</div>",
      ].join(""),
    );
    const text = clipboardMarkdownFromSelection(selectNode(root), root);
    expect(text).toBe("正文");
    expect(text).not.toContain("由 Gemini 生成");
    expect(text).not.toContain("带到讨论");
  });

  it("returns null for a collapsed selection", () => {
    const root = mountHtml(
      '<div class="markdown-body"><p>正文</p></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const selection = selectText(firstText(body, "正文"), 1, 1);
    expect(selection.isCollapsed).toBe(true);
    expect(clipboardMarkdownFromSelection(selection, body)).toBeNull();
  });

  it("copies streaming markdown as the selected source text", () => {
    const root = mountHtml(
      '<div class="markdown-body markdown-streaming">1. 甲\n2. 乙</div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(clipboardMarkdownFromSelection(selectNode(body), body)).toBe(
      "1. 甲\n2. 乙",
    );
  });

  it("does not intercept copy when the selection lives in an editor", () => {
    const root = mountHtml(
      '<div><div contenteditable="true">draft</div><div class="markdown-body"><p>正文</p></div></div>',
    );
    const editor = root.querySelector("[contenteditable]") as HTMLElement;
    const { event, data } = dispatchCopy(root);
    handleMarkdownCopyEvent(
      event as unknown as ClipboardEvent,
      root,
      selectNode(editor),
    );
    expect(event.defaultPrevented).toBe(false);
    expect(data["text/plain"]).toBeUndefined();
  });

  it("writes only text/plain when intercepting a markdown selection", () => {
    const root = mountHtml(
      '<div class="markdown-body"><ol><li>甲</li></ol></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    const selection = selectNode(body);
    const { event, data } = dispatchCopy(body);
    handleMarkdownCopyEvent(
      event as unknown as ClipboardEvent,
      body,
      selection,
    );
    expect(event.defaultPrevented).toBe(true);
    expect(data["text/plain"]).toBe("1. 甲");
    expect(data["text/html"]).toBeUndefined();
  });

  it("keeps clipboardTextFromMarkdownSelection as the single-root alias", () => {
    const root = mountHtml(
      '<div class="markdown-body"><p>See <span class="katex"><annotation encoding="application/x-tex">x+y</annotation>xy</span> now</p></div>',
    );
    const body = root.querySelector(".markdown-body") as HTMLElement;
    expect(clipboardTextFromMarkdownSelection(body, selectNode(body))).toContain(
      "$x+y$",
    );
  });
});
