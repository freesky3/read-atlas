import { fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import MarkdownBody from "./MarkdownBody";
import { renderWithLocale as render } from "./i18n/testUtils";

function dispatchCopy(node: EventTarget) {
  const data: Record<string, string> = {};
  const event = new Event("copy", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", {
    value: {
      setData: (type: string, value: string) => {
        data[type] = value;
      },
      getData: (type: string) => data[type] ?? "",
    },
  });
  node.dispatchEvent(event);
  return { event, data };
}

function selectContents(node: Node) {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return selection;
}

describe("MarkdownBody rendering of repaired artifacts", () => {
  it("renders CJK bold adjacent to punctuation as strong", () => {
    const { container } = render(
      <MarkdownBody>
        {"执行**监督学习**，其核心目标是最小化误差。"}
      </MarkdownBody>,
    );
    const strong = [...container.querySelectorAll("strong")].map(
      (node) => node.textContent,
    );
    expect(strong).toContain("监督学习");
    expect(container.textContent).not.toContain("**");
  });

  it("renders packed ordered findings as separate list items", () => {
    const { container } = render(
      <MarkdownBody>
        {
          "1. 监督学习中反向传播利用链式法则计算梯度；2. 强化学习中多巴胺神经元表征奖赏预测误差；3. TD学习通过Bellman方程解释时间回溯。"
        }
      </MarkdownBody>,
    );
    const items = [...container.querySelectorAll("li")].map((node) =>
      (node.textContent ?? "").trim(),
    );
    expect(items).toHaveLength(3);
    expect(items[0]).toContain("监督学习");
    expect(items[1]).toContain("强化学习");
    expect(items[2]).toContain("TD学习");
  });

  it("renders already-correct lens bold without leftover asterisks", () => {
    const { container } = render(
      <MarkdownBody>
        {
          "1. **无监督学习**：无外部教学信号。\n2. **强化学习**：教学信号为**标量（Scalar）评价**，仅告知优劣。\n3. **监督学习**：教学信号为**多维误差向量（Vector error）**。"
        }
      </MarkdownBody>,
    );
    const strong = [...container.querySelectorAll("strong")].map(
      (node) => node.textContent,
    );
    expect(strong).toEqual(
      expect.arrayContaining([
        "无监督学习",
        "强化学习",
        "标量（Scalar）评价",
        "监督学习",
        "多维误差向量（Vector error）",
      ]),
    );
    expect(container.textContent).not.toContain("**");
    expect(container.querySelectorAll("li")).toHaveLength(3);
  });

  it("copies packed ordered findings as GFM list markers", () => {
    const { container } = render(
      <MarkdownBody>
        {
          "1. 监督学习中反向传播利用链式法则计算梯度；2. 强化学习中多巴胺神经元表征奖赏预测误差；3. TD学习通过Bellman方程解释时间回溯。"
        }
      </MarkdownBody>,
    );
    const root = container.querySelector(".markdown-body") as HTMLElement;
    selectContents(root);
    const { event, data } = dispatchCopy(root);
    expect(event.defaultPrevented).toBe(true);
    expect(data["text/plain"]).toContain("1. ");
    expect(data["text/plain"]).toContain("2. ");
    expect(data["text/plain"]).toContain("3. ");
    expect(data["text/html"]).toBeUndefined();
  });

  it("copies inline and display math with $ delimiters", () => {
    const { container } = render(
      <MarkdownBody>{"Energy $E=mc^2$\n\n$$\n\\int f\n$$"}</MarkdownBody>,
    );
    const root = container.querySelector(".markdown-body") as HTMLElement;
    selectContents(root);
    const { data } = dispatchCopy(root);
    expect(data["text/plain"]).toContain("$E=mc^2$");
    expect(data["text/plain"]).toContain("$$\n\\int f\n$$");
  });

  it("copies a formula on click when the selection is collapsed", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const { container } = render(
      <MarkdownBody>{"$E=mc^2$"}</MarkdownBody>,
    );
    window.getSelection()?.removeAllRanges();
    const hit = container.querySelector(".latex-hit") as HTMLElement;
    expect(hit).toBeTruthy();
    fireEvent.click(hit);
    expect(writeText).toHaveBeenCalledWith("$E=mc^2$");
  });

  it("does not copy a formula on click when text is already selected", () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const { container } = render(
      <MarkdownBody>{"See $E=mc^2$ now"}</MarkdownBody>,
    );
    const root = container.querySelector(".markdown-body") as HTMLElement;
    selectContents(root);
    const hit = container.querySelector(".latex-hit") as HTMLElement;
    fireEvent.click(hit);
    expect(writeText).not.toHaveBeenCalled();
  });
});
