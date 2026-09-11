const GREEK_NAMES =
  "alpha|beta|gamma|delta|epsilon|zeta|eta|theta|iota|kappa|lambda|mu|nu|xi|omicron|pi|rho|sigma|tau|upsilon|phi|chi|psi|omega|Gamma|Delta|Theta|Lambda|Xi|Pi|Sigma|Upsilon|Phi|Psi|Omega";

export function preprocessLaTeX(text: string): string {
  if (!text) return "";
  const greekPattern = `(?:${GREEK_NAMES})`;
  return text
    .replace(/\x08([a-zA-Z]+)/g, "\\b$1")
    .replace(/\x0c([a-zA-Z]+)/g, "\\f$1")
    .replace(/\x08/g, "\\b")
    .replace(/\x0c/g, "\\f")
    .replace(/\t([a-zA-Z]+)/g, "\\t$1")
    .replace(/\r(ho\b|ight\b)/g, "\\r$1")
    .replace(/(?<=\b(?:distribution|density|measure|parameter|variable|field)\s+)ho\b/gi, "$\\rho$")
    .replace(/\bho\(([^)]+)\)/g, "$\\rho($1)$")
    .replace(new RegExp(`\\\\textbf\\{\\\\?(${greekPattern})\\}`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\textbf\\s+\\\\?(${greekPattern})`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\mathbf\\{\\\\?(${greekPattern})\\}`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\mathbf\\s+\\\\?(${greekPattern})`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bold\\{\\\\?(${greekPattern})\\}`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bold\\s+\\\\?(${greekPattern})`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bf\\{\\\\?(${greekPattern})\\}`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bf\\s+\\\\?(${greekPattern})`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bm\\{\\\\?(${greekPattern})\\}`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\bm\\s+\\\\?(${greekPattern})`, "g"), "\\boldsymbol{\\$1}")
    .replace(new RegExp(`\\\\text\\{\\\\?(${greekPattern})\\}`, "g"), "\\$1")
    .replace(new RegExp(`\\\\mathrm\\{\\\\?(${greekPattern})\\}`, "g"), "\\$1")
    .replace(new RegExp(`\\\\textit\\{\\\\?(${greekPattern})\\}`, "g"), "\\$1")
    .replace(/\\bm\{/g, "\\boldsymbol{")
    .replace(/\\\(([\s\S]*?)\\\)/g, "$$$1$$")
    .replace(/\\\[([\s\S]*?)\\\]/g, "$$$$$1$$$$");
}

export function hasIncompleteMath(text: string): boolean {
  const dollars = (text.match(/\$/g) ?? []).length;
  if (dollars % 2 !== 0) return true;
  const openParen = (text.match(/\\\(/g) ?? []).length;
  const closeParen = (text.match(/\\\)/g) ?? []).length;
  const openBracket = (text.match(/\\\[/g) ?? []).length;
  const closeBracket = (text.match(/\\\]/g) ?? []).length;
  return openParen !== closeParen || openBracket !== closeBracket;
}

export type PaperCitationLink =
  { kind: "page"; page: number } | { kind: "block"; blockId: string };

export function parsePaperCitationHref(
  href: string | undefined,
): PaperCitationLink | null {
  if (!href) return null;
  const page = href.match(/^#cite-page-(\d+)$/);
  if (page) return { kind: "page", page: Number(page[1]) };
  const block = href.match(/^#cite-block-(.+)$/);
  if (block?.[1]) return { kind: "block", blockId: block[1].trim() };
  return null;
}

export function linkPaperCitations(text: string): string {
  return text
    .replace(/\[p\.\s*(\d+)\]/gi, "[$&](#cite-page-$1)")
    .replace(/\[block:\s*([^\]]+?)\]/gi, "[$&](#cite-block-$1)");
}

const PROTECTED_SEGMENT =
  /(\$\$[\s\S]*?\$\$|\$[^$\n]+\$|\\\[[\s\S]*?\\\]|\\\([\s\S]*?\\\)|```[\s\S]*?```|`[^`]+`)/g;

const BARE_LATEX_TOKEN =
  /\\[A-Za-z]+(?:\s*\{[^}]*\})*(?:[_^](?:\{[^}]+\}|[A-Za-z0-9])+)*(?:\([^)]*\))?|[A-Za-z](?:[_^](?:\{[^}]+\}|[A-Za-z0-9])+)+(?:\([^)]*\))?/g;

const INNER_EMPHASIS_WS = /[ \t\u00a0\u3000]/;
const CJK_LETTER = /\p{Script=Han}|\p{Script=Hiragana}|\p{Script=Katakana}/u;

function mapUnprotected(text: string, fn: (segment: string) => string): string {
  return text
    .split(PROTECTED_SEGMENT)
    .map((segment, index) => {
      if (index % 2 === 1 || !segment) return segment;
      return fn(segment);
    })
    .join("");
}

export function wrapBareInlineMath(text: string) {
  return mapUnprotected(text, (segment) =>
    segment.replace(BARE_LATEX_TOKEN, (token) => `$${token}$`),
  );
}

function isCjkLetter(ch: string | undefined): boolean {
  return !!ch && CJK_LETTER.test(ch);
}

function lastEmittedChar(out: string[]): string | undefined {
  for (let i = out.length - 1; i >= 0; i -= 1) {
    if (out[i]) return out[i];
  }
  return undefined;
}

function findMarkerPositions(
  chars: string[],
  marker: string,
  skip: (index: number) => boolean,
): number[] {
  const positions: number[] = [];
  const first = marker[0];
  let i = 0;
  while (i < chars.length) {
    if (chars[i] !== first) {
      i += 1;
      continue;
    }
    let matched = true;
    for (let k = 1; k < marker.length; k += 1) {
      if (chars[i + k] !== marker[k]) {
        matched = false;
        break;
      }
    }
    if (matched && !skip(i)) {
      positions.push(i);
      i += marker.length;
    } else {
      i += 1;
    }
  }
  return positions;
}

function repairMarkerPairs(
  text: string,
  marker: string,
  streaming: boolean,
  skip: (chars: string[], index: number) => boolean = () => false,
): string {
  const chars = Array.from(text);
  if (chars.length === 0) return text;
  const positions = findMarkerPositions(chars, marker, (index) =>
    skip(chars, index),
  );
  if (positions.length === 0) return text;
  if (positions.length % 2 === 1) {
    if (streaming || marker !== "**") return text;
    const stray = positions[positions.length - 1];
    let splitAt = chars.length;
    for (let k = stray + marker.length; k < chars.length; k += 1) {
      if (chars[k] === "\n") {
        splitAt = k;
        break;
      }
    }
    return repairMarkerPairs(
      chars.slice(0, splitAt).join("") + marker + chars.slice(splitAt).join(""),
      marker,
      streaming,
      skip,
    );
  }

  const out: string[] = [];
  let idx = 0;
  for (let p = 0; p < positions.length; p += 2) {
    const open = positions[p];
    const close = positions[p + 1];
    out.push(...chars.slice(idx, open));
    let innerStart = open + marker.length;
    let innerEnd = close;
    while (innerStart < innerEnd && INNER_EMPHASIS_WS.test(chars[innerStart])) {
      innerStart += 1;
    }
    while (innerEnd > innerStart && INNER_EMPHASIS_WS.test(chars[innerEnd - 1])) {
      innerEnd -= 1;
    }
    const before = lastEmittedChar(out);
    if (isCjkLetter(before)) {
      out.push(" ");
    }
    out.push(...Array.from(marker));
    out.push(...chars.slice(innerStart, innerEnd));
    out.push(...Array.from(marker));
    const after = chars[close + marker.length];
    if (isCjkLetter(after)) {
      out.push(" ");
    }
    idx = close + marker.length;
  }
  out.push(...chars.slice(idx));
  return out.join("");
}

export function repairInlineEmphasis(
  text: string,
  streaming = false,
): string {
  if (!text) return "";
  let out = text.replace(/\\\*\*/g, "**");
  out = repairMarkerPairs(out, "**", streaming);
  out = repairMarkerPairs(out, "__", streaming);
  out = repairMarkerPairs(out, "*", streaming, (chars, index) => {
    if (chars[index + 1] === "*" || (index > 0 && chars[index - 1] === "*")) {
      return true;
    }
    const atLineStart = index === 0 || chars[index - 1] === "\n";
    return atLineStart && chars[index + 1] === " ";
  });
  return out;
}

export function fixCjkEmphasis(text: string): string {
  return mapUnprotected(text, (segment) => repairInlineEmphasis(segment, false));
}

export function fixEmphasisSpaces(text: string): string {
  return mapUnprotected(text, (segment) => repairInlineEmphasis(segment, false));
}

function isListTerminator(ch: string | undefined): boolean {
  if (!ch) return false;
  return "。！？!?；;：:，,".includes(ch) || ch === "\uFFFD";
}

function precededByLatinWord(chars: string[], index: number): boolean {
  let k = index;
  while (k > 0 && INNER_EMPHASIS_WS.test(chars[k - 1])) {
    k -= 1;
  }
  return k > 0 && /[A-Za-z]/.test(chars[k - 1]);
}

function detectListMarker(
  chars: string[],
  index: number,
): { len: number; n: number | null } | null {
  if (chars[index] === "-" && chars[index + 1] === " ") {
    return { len: 2, n: null };
  }
  let j = index;
  let digits = "";
  while (j < chars.length && /[0-9]/.test(chars[j]) && digits.length < 3) {
    digits += chars[j];
    j += 1;
  }
  if (!digits) return null;
  const sep = chars[j];
  if (sep === "." || sep === ")" || sep === "．") {
    if (j + 1 < chars.length && INNER_EMPHASIS_WS.test(chars[j + 1])) {
      return { len: j + 2 - index, n: Number(digits) };
    }
    if (j + 1 === chars.length) {
      return { len: j + 1 - index, n: Number(digits) };
    }
    return null;
  }
  if (sep === "、") {
    return { len: j + 1 - index, n: Number(digits) };
  }
  return null;
}

export function splitPackedListItems(text: string): string {
  const chars = Array.from(text);
  const out: string[] = [];
  let i = 0;
  let lineStart = true;
  let lastNonSpace: string | undefined;
  let lastNumber = 0;
  let inBold = false;
  while (i < chars.length) {
    const ch = chars[i];
    if (ch === "\n") {
      out.push("\n");
      i += 1;
      lineStart = true;
      lastNonSpace = undefined;
      lastNumber = 0;
      continue;
    }
    if (ch === "*" && chars[i + 1] === "*") {
      inBold = !inBold;
      out.push("*", "*");
      i += 2;
      lineStart = false;
      lastNonSpace = "*";
      continue;
    }
    if (INNER_EMPHASIS_WS.test(ch) && !lineStart) {
      out.push(ch);
      i += 1;
      continue;
    }
    const marker = inBold ? null : detectListMarker(chars, i);
    if (marker && !precededByLatinWord(chars, i)) {
      const shouldSplit =
        lineStart ||
        isListTerminator(lastNonSpace) ||
        (marker.n !== null && lastNumber > 0 && marker.n === lastNumber + 1);
      if (shouldSplit) {
        if (!lineStart) {
          while (out.length > 0 && INNER_EMPHASIS_WS.test(out[out.length - 1])) {
            out.pop();
          }
          out.push("\n");
        }
        out.push(...chars.slice(i, i + marker.len));
        i += marker.len;
        lineStart = false;
        lastNonSpace = undefined;
        if (marker.n !== null) lastNumber = marker.n;
        continue;
      }
    }
    out.push(ch);
    if (!INNER_EMPHASIS_WS.test(ch)) {
      lastNonSpace = ch;
      lineStart = false;
    }
    i += 1;
  }
  return out.join("");
}

export function decodeLiteralBreaks(text: string): string {
  const chars = Array.from(text);
  const out: string[] = [];
  for (let i = 0; i < chars.length; i += 1) {
    if (chars[i] === "\\" && i + 1 < chars.length) {
      const next = chars[i + 1];
      const afterLetter = i + 2 < chars.length && /[A-Za-z]/.test(chars[i + 2]);
      if (!afterLetter && (next === "n" || next === "r")) {
        out.push("\n");
        i += 1;
        continue;
      }
    }
    out.push(chars[i]);
  }
  return out.join("");
}

export function prepareMarkdown(text: string, streaming = false): string {
  if (!text) return "";
  if (streaming && hasIncompleteMath(text)) return text;
  const repaired = mapUnprotected(preprocessLaTeX(text), (segment) =>
    splitPackedListItems(
      repairInlineEmphasis(decodeLiteralBreaks(segment), streaming),
    ),
  );
  return linkPaperCitations(wrapBareInlineMath(repaired));
}

export function hasMathDelimiters(text: string) {
  return /\$|\\\(|\\\[/.test(text);
}

export function looksLikeLatex(text: string) {
  return /[_^\\{}]/.test(text);
}

export function ensureInlineMath(text: string) {
  const value = text.trim();
  if (!value || hasMathDelimiters(value)) return value;
  if (!looksLikeLatex(value)) return value;
  return `$${value.replace(/\s+or\s+/gi, "\\text{ or }")}$`;
}

export function ensureDisplayMath(text: string) {
  const value = text.trim();
  if (!value || hasMathDelimiters(value)) return value;
  return `$$${value}$$`;
}

export function latexFromKatexElement(element: Element): string {
  return (
    element
      .querySelector('annotation[encoding="application/x-tex"]')
      ?.textContent ?? ""
  ).trim();
}

export function formatInlineLatex(tex: string) {
  return `$${tex}$`;
}

export function formatDisplayLatex(tex: string) {
  return `$$\n${tex}\n$$`;
}

export function replaceKatexWithDelimiters(root: ParentNode) {
  root.querySelectorAll(".katex-display").forEach((node) => {
    const tex = latexFromKatexElement(node);
    node.replaceWith(tex ? formatDisplayLatex(tex) : "");
  });
  root.querySelectorAll(".katex").forEach((node) => {
    const tex = latexFromKatexElement(node);
    node.replaceWith(tex ? formatInlineLatex(tex) : "");
  });
}

export function formattedLatexFromKatexNode(node: Element): string | null {
  const root =
    node.closest(".katex-display") ?? node.closest(".katex");
  if (!root) return null;
  const tex = latexFromKatexElement(root);
  if (!tex) return null;
  return root.classList.contains("katex-display")
    ? formatDisplayLatex(tex)
    : formatInlineLatex(tex);
}

const EDITABLE_SELECTOR =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"])';
const MARKDOWN_ROOT_SELECTOR = ".markdown-body";
const SECTION_TITLE_SELECTOR = "h3.artifact-section-title";
const BLOCK_TAGS = new Set([
  "P",
  "H1",
  "H2",
  "H3",
  "H4",
  "H5",
  "H6",
  "UL",
  "OL",
  "PRE",
  "TABLE",
  "BLOCKQUOTE",
  "HR",
  "DIV",
  "SECTION",
  "ARTICLE",
  "FIGURE",
  "LI",
]);

type CopyEventLike = {
  preventDefault: () => void;
  defaultPrevented: boolean;
  clipboardData: DataTransfer | null;
};

function isElement(node: Node): node is Element {
  return node.nodeType === Node.ELEMENT_NODE;
}

function isEditableNode(node: Node | null | undefined): boolean {
  if (!node) return false;
  if (
    node instanceof HTMLInputElement ||
    node instanceof HTMLTextAreaElement ||
    node instanceof HTMLSelectElement
  ) {
    return true;
  }
  const element = isElement(node) ? node : node.parentElement;
  return Boolean(element?.closest(EDITABLE_SELECTOR));
}

function rangeTouchesEditable(range: Range): boolean {
  return (
    isEditableNode(range.startContainer) || isEditableNode(range.endContainer)
  );
}

function textNodeSlice(node: Text, range: Range): string {
  if (!range.intersectsNode(node)) return "";
  const from = range.startContainer === node ? range.startOffset : 0;
  const to = range.endContainer === node ? range.endOffset : node.data.length;
  if (to <= from) return "";
  return node.data.slice(from, to);
}

function selectedTextUnder(root: Node, range: Range): string {
  if (root.nodeType === Node.TEXT_NODE) {
    return textNodeSlice(root as Text, range);
  }
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let out = "";
  let current: Node | null;
  while ((current = walker.nextNode())) {
    out += textNodeSlice(current as Text, range);
  }
  return out;
}

function collectWhitelist(container: ParentNode, range: Range): Element[] {
  const matches: Element[] = [];
  if (
    container instanceof Element &&
    (container.matches(MARKDOWN_ROOT_SELECTOR) ||
      container.matches(SECTION_TITLE_SELECTOR)) &&
    range.intersectsNode(container)
  ) {
    matches.push(container);
  }
  const scoped = container.querySelectorAll(
    `${MARKDOWN_ROOT_SELECTOR}, ${SECTION_TITLE_SELECTOR}`,
  );
  scoped.forEach((element) => {
    if (!range.intersectsNode(element)) return;
    if (
      element.matches(MARKDOWN_ROOT_SELECTOR) &&
      element.parentElement?.closest(MARKDOWN_ROOT_SELECTOR)
    ) {
      return;
    }
    matches.push(element);
  });
  return matches;
}

function wrapInlineCode(value: string) {
  if (!value) return "";
  if (value.includes("`")) return "`` " + value + " ``";
  return `\`${value}\``;
}

function coveringRange(node: Node): Range {
  const range = document.createRange();
  range.selectNodeContents(node);
  return range;
}

function fenceLanguage(code: Element) {
  const match = /(?:^|\s)language-([^\s]+)/.exec(code.className);
  return match?.[1] ?? "";
}

function katexRoot(element: Element): Element | null {
  const display = element.closest(".katex-display");
  if (display) return display;
  const inline = element.closest(".katex");
  if (inline?.closest(".katex-display")) {
    return inline.closest(".katex-display");
  }
  return inline;
}

function serializeKatex(element: Element, range: Range): string {
  const root = katexRoot(element);
  if (!root) return "";
  const tex = latexFromKatexElement(root);
  if (tex) {
    return root.classList.contains("katex-display")
      ? formatDisplayLatex(tex)
      : formatInlineLatex(tex);
  }
  const glyphs = root.querySelector(".katex-html") ?? root;
  return selectedTextUnder(glyphs, range);
}

function isDisplayMathBlock(value: string) {
  return value.startsWith("$$\n") || value.startsWith("$$\r");
}

function joinInlineParts(parts: string[]) {
  let result = "";
  for (const part of parts) {
    if (!part) continue;
    if (!result) {
      result = part;
      continue;
    }
    if (isDisplayMathBlock(result) || isDisplayMathBlock(part)) {
      result = `${result.replace(/\n+$/, "")}\n\n${part.replace(/^\n+/, "")}`;
    } else {
      result += part;
    }
  }
  return result;
}

function serializeInlines(root: Node, range: Range): string {
  if (!range.intersectsNode(root)) return "";
  if (root.nodeType === Node.TEXT_NODE) {
    return textNodeSlice(root as Text, range);
  }
  if (!isElement(root)) return "";

  if (root.classList.contains("katex-display") || root.classList.contains("katex")) {
    return serializeKatex(root, range);
  }
  if (root.classList.contains("markdown-citation")) {
    return selectedTextUnder(root, range);
  }

  const tag = root.tagName;
  if (tag === "BR") return "\n";
  if (tag === "IMG" || tag === "SCRIPT" || tag === "STYLE" || tag === "SVG") {
    return "";
  }
  if (tag === "STRONG" || tag === "B") {
    const inner = serializeChildrenInlines(root, range);
    return inner ? `**${inner}**` : "";
  }
  if (tag === "EM" || tag === "I") {
    const inner = serializeChildrenInlines(root, range);
    return inner ? `*${inner}*` : "";
  }
  if (tag === "CODE" && root.parentElement?.tagName !== "PRE") {
    const inner = selectedTextUnder(root, range);
    return inner ? wrapInlineCode(inner) : "";
  }
  if (tag === "A") {
    const inner = serializeChildrenInlines(root, range);
    if (!inner) return "";
    const href = root.getAttribute("href") ?? "";
    if (/^(https?:|mailto:)/i.test(href)) return `[${inner}](${href})`;
    return inner;
  }
  if (tag === "BUTTON") {
    return selectedTextUnder(root, range);
  }
  return serializeChildrenInlines(root, range);
}

function serializeChildrenInlines(element: Element, range: Range): string {
  const parts: string[] = [];
  element.childNodes.forEach((child) => {
    parts.push(serializeInlines(child, range));
  });
  return joinInlineParts(parts);
}

function serializeListItem(
  item: Element,
  range: Range,
  marker: string,
  indent: string,
): string {
  const inlineParts: string[] = [];
  const nested: string[] = [];
  item.childNodes.forEach((child) => {
    if (!range.intersectsNode(child) && !range.intersectsNode(item)) return;
    if (isElement(child) && (child.tagName === "UL" || child.tagName === "OL")) {
      const nestedList = serializeList(child, range, `${indent}  `);
      if (nestedList) nested.push(nestedList);
      return;
    }
    if (isElement(child) && child.tagName === "P") {
      inlineParts.push(serializeChildrenInlines(child, range));
      return;
    }
    inlineParts.push(serializeInlines(child, range));
  });
  const joined = joinInlineParts(inlineParts);
  const head = isDisplayMathBlock(joined)
    ? joined.trim()
    : joined.replace(/\s+/g, " ").trim();
  const line = `${indent}${marker}${head}`;
  return nested.length ? `${line}\n${nested.join("\n")}` : line;
}

function serializeList(
  list: Element,
  range: Range,
  indent: string,
): string {
  const ordered = list.tagName === "OL";
  const start =
    ordered && list instanceof HTMLOListElement && list.start
      ? list.start
      : 1;
  const lines: string[] = [];
  let index = 0;
  list.childNodes.forEach((child) => {
    if (!isElement(child) || child.tagName !== "LI") return;
    const marker = ordered ? `${start + index}. ` : "- ";
    index += 1;
    if (!range.intersectsNode(child)) return;
    lines.push(serializeListItem(child, range, marker, indent));
  });
  return lines.join("\n");
}

function serializeTableCell(cell: Element, range: Range, clip: boolean) {
  const raw = serializeChildrenInlines(
    cell,
    clip ? range : coveringRange(cell),
  );
  return raw.replace(/\|/g, "\\|").replace(/\s*\n\s*/g, " ").trim();
}

function tableRows(table: Element) {
  const head = table.querySelector("thead");
  const headerRows = head
    ? [...head.querySelectorAll("tr")]
    : [];
  const body = table.querySelector("tbody");
  const bodyRows = body
    ? [...body.querySelectorAll("tr")]
    : [...table.querySelectorAll("tr")].filter(
        (row) => !head?.contains(row),
      );
  return { headerRows, bodyRows };
}

function serializeTable(table: Element, range: Range): string {
  const { headerRows, bodyRows } = tableRows(table);
  const selectedBody = bodyRows.filter((row) => range.intersectsNode(row));
  const selectedHeader = headerRows.filter((row) => range.intersectsNode(row));
  if (selectedBody.length === 0 && selectedHeader.length === 0) return "";

  const emitHeader =
    headerRows.length > 0
      ? headerRows
      : selectedBody.length
        ? [selectedBody[0]]
        : selectedHeader;
  const emitBody =
    headerRows.length > 0
      ? selectedBody
      : selectedBody.slice(headerRows.length === 0 && bodyRows[0] === selectedBody[0] ? 1 : 0);

  const headerCells = [...emitHeader[0].children].filter(
    (cell) => cell.tagName === "TH" || cell.tagName === "TD",
  );
  const headerLine = `| ${headerCells
    .map((cell) => serializeTableCell(cell, range, false))
    .join(" | ")} |`;
  const rule = `| ${headerCells.map(() => "---").join(" | ")} |`;
  const bodyLines = emitBody.map((row) => {
    const cells = [...row.children].filter(
      (cell) => cell.tagName === "TH" || cell.tagName === "TD",
    );
    return `| ${cells.map((cell) => serializeTableCell(cell, range, true)).join(" | ")} |`;
  });
  return [headerLine, rule, ...bodyLines].join("\n");
}

function headingPrefix(tag: string) {
  const level = Number(tag.slice(1));
  if (!Number.isFinite(level) || level < 1) return "";
  return `${"#".repeat(Math.min(level, 6))} `;
}

function serializePre(pre: Element): string {
  const code = pre.querySelector("code") ?? pre;
  const lang = fenceLanguage(code);
  const body = (code.textContent ?? "").replace(/\n$/, "");
  return `\`\`\`${lang}\n${body}\n\`\`\``;
}

function serializeBlock(node: Node, range: Range): string {
  if (!range.intersectsNode(node)) return "";
  if (node.nodeType === Node.TEXT_NODE) {
    const text = textNodeSlice(node as Text, range);
    return text.trim() ? text : "";
  }
  if (!isElement(node)) return "";

  if (node.classList.contains("katex-display") || node.classList.contains("katex")) {
    return serializeKatex(node, range);
  }

  const tag = node.tagName;
  if (tag === "UL" || tag === "OL") return serializeList(node, range, "");
  if (tag === "PRE") return serializePre(node);
  if (tag === "TABLE") return serializeTable(node, range);
  if (tag === "HR") return "";
  if (/^H[1-6]$/.test(tag)) {
    const inner = serializeChildrenInlines(node, range).trim();
    return inner ? `${headingPrefix(tag)}${inner}` : "";
  }
  if (tag === "P" || tag === "BLOCKQUOTE" || tag === "FIGCAPTION") {
    return serializeChildrenInlines(node, range).trim();
  }
  if (tag === "LI") {
    return serializeListItem(node, range, "- ", "");
  }
  if (BLOCK_TAGS.has(tag) || node.classList.contains("markdown-body")) {
    return serializeBlockChildren(node, range);
  }
  return serializeInlines(node, range).trim();
}

function serializeBlockChildren(element: Element, range: Range): string {
  const blocks: string[] = [];
  element.childNodes.forEach((child) => {
    const serialized = serializeBlock(child, range);
    if (serialized) blocks.push(serialized);
  });
  return blocks.join("\n\n");
}

function serializeMarkdownRoot(root: Element, range: Range): string {
  if (root.classList.contains("markdown-streaming")) {
    return selectedTextUnder(root, range).replace(/\n+$/, "");
  }
  return serializeBlockChildren(root, range);
}

function serializeSectionTitle(title: Element, range: Range): string {
  const text = selectedTextUnder(title, range).replace(/\s+/g, " ").trim();
  return text ? `### ${text}` : "";
}

export function clipboardMarkdownFromSelection(
  selection: Selection | null,
  container: ParentNode,
): string | null {
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
    return null;
  }
  const range = selection.getRangeAt(0);
  if (
    container instanceof Node &&
    !container.contains(range.commonAncestorContainer) &&
    !(container instanceof Element && range.intersectsNode(container))
  ) {
    return null;
  }
  const whitelist = collectWhitelist(container, range);
  const targets =
    whitelist.length > 0
      ? whitelist
      : container instanceof Element && range.intersectsNode(container)
        ? [container]
        : [];
  const pieces = targets
    .map((element) =>
      element.matches(SECTION_TITLE_SELECTOR)
        ? serializeSectionTitle(element, range)
        : serializeMarkdownRoot(element, range),
    )
    .filter(Boolean);
  const text = pieces.join("\n\n").trim();
  return text || null;
}

export function clipboardTextFromMarkdownSelection(
  root: HTMLElement,
  selection: Selection | null = window.getSelection(),
): string | null {
  return clipboardMarkdownFromSelection(selection, root);
}

export function handleMarkdownCopyEvent(
  event: CopyEventLike,
  container: HTMLElement,
  selection: Selection | null = window.getSelection(),
): void {
  if (event.defaultPrevented) return;
  try {
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
      return;
    }
    const range = selection.getRangeAt(0);
    if (rangeTouchesEditable(range)) return;
    if (collectWhitelist(container, range).length === 0) return;
    const text = clipboardMarkdownFromSelection(selection, container);
    if (!text) return;
    event.preventDefault();
    event.clipboardData?.setData("text/plain", text);
  } catch {
    // Leave the native copy path intact if serialization fails.
  }
}
