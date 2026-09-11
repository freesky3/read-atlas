import { useEffect, useRef } from "react";
import type { ClipboardEvent, MouseEvent } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import {
  formattedLatexFromKatexNode,
  handleMarkdownCopyEvent,
  hasIncompleteMath,
  parsePaperCitationHref,
  prepareMarkdown,
} from "./markdown";
import { useLocale } from "./i18n/LocaleContext";


async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export default function MarkdownBody({
  children,
  streaming = false,
  onCitation,
}: {
  children: string;
  streaming?: boolean;
  onCitation?: (href: string) => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const { t } = useLocale();


  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    root.querySelectorAll(".katex-display, .katex").forEach((node) => {
      if (
        node.classList.contains("katex") &&
        node.closest(".katex-display")
      ) {
        return;
      }
      const element = node as HTMLElement;
      element.classList.add("latex-hit");
      element.title = t("markdown.copyLatex");
    });
  }, [children, t]);

  const onCopy = (event: ClipboardEvent<HTMLDivElement>) => {
    const root = rootRef.current;
    if (!root) return;
    handleMarkdownCopyEvent(event, root);
  };

  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    if (target.closest("button, a")) return;
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed) return;
    const latex = formattedLatexFromKatexNode(target);
    if (!latex) return;
    event.preventDefault();
    void copyText(latex);
  };

  if (!children) return null;
  if (streaming && hasIncompleteMath(children)) {
    return (
      <div
        className="markdown-body markdown-streaming"
        ref={rootRef}
        onCopy={onCopy}
        onClick={onClick}
      >
        {children}
      </div>
    );
  }
  return (
    <div
      className="markdown-body"
      ref={rootRef}
      onCopy={onCopy}
      onClick={onClick}
    >
      <ReactMarkdown
        remarkPlugins={[[remarkGfm, { singleTilde: false }], remarkMath]}
        rehypePlugins={[
          [
            rehypeKatex,
            { strict: false, trust: true, output: "htmlAndMathml" },
          ],
        ]}
        components={{
          a({ href, children: label }) {
            const citation = parsePaperCitationHref(href);
            if (citation && onCitation) {
              return (
                <button
                  type="button"
                  className="markdown-citation"
                  onClick={() => onCitation(href ?? "")}
                >
                  {label}
                </button>
              );
            }
            return (
              <a href={href} target="_blank" rel="noreferrer">
                {label}
              </a>
            );
          },
        }}
      >
        {prepareMarkdown(children, streaming)}
      </ReactMarkdown>
    </div>
  );
}
