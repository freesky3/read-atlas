import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { ReadingRoadmapPanel } from "./ReadingRoadmap";
import type { ReadingRoadmapProjection, RoadmapProgressEntry } from "./types";
import { renderWithLocale as render } from "./i18n/testUtils";

const mockProjection: ReadingRoadmapProjection = {
  id: "roadmap-1",
  paperId: "paper-1",
  revisionId: "rev-1",
  status: "published",
  activeJobId: null,
  lastError: null,
  createdAt: "2026-08-20T00:00:00Z",
  content: {
    version: 1,
    generatedAt: "2026-08-20T00:00:00Z",
    paperTitle: "Attention Is All You Need",
    passes: [
      {
        passNumber: 0,
        title: "前置对齐",
        subtitle: "让读者不在正文里迷路",
        timeBudget: "10–20 分钟",
        exitCriteria: "能用自己的话回答论文在解决什么问题",
        tasks: [
          {
            id: "p0-t1",
            text: "了解 Self-Attention 的基本概念与前置背景",
            timeMinutes: 5,
            required: true,
            completionCriteria: "能说出 self-attention 的核心作用",
            selfCheckQuestions: ["什么是 self-attention？", "它与 RNN 的差异在哪里？"],
            evidence: [{ label: "p.1 · Abstract", page: 1, blockId: "b-1" }],
          },
        ],
      },
      {
        passNumber: 1,
        title: "鸟瞰 + 决策",
        subtitle: "五分钟决定是否深入",
        timeBudget: "5–15 分钟",
        exitCriteria: "能在 30 秒内向别人介绍这篇论文",
        tasks: [
          {
            id: "p1-t1",
            text: "阅读 5C 中的 Category 与 Contributions",
            timeMinutes: 10,
            required: true,
            completionCriteria: "明确架构创新的核心卖点",
            selfCheckQuestions: ["作者声称的贡献哪一条最硬？"],
            evidence: [{ label: "p.2 · Section 1", page: 2 }],
          },
        ],
      },
    ],
    elevatorPitch: "这篇论文提出了完全基于自注意力机制的 Transformer 架构...",
    oneChart: { label: "Figure 1", page: 3, blockId: "b-fig1", reason: "展示了完整的 Encoder-Decoder 结构" },
  },
};

describe("ReadingRoadmapPanel", () => {
  it("renders a route without optional extras and preserves task order", () => {
    const content = { ...mockProjection.content! };
    delete content.elevatorPitch;
    delete content.oneChart;
    render(<ReadingRoadmapPanel open={true} projection={{...mockProjection, content}}
      progress={[]} activeJobId={null} onClose={vi.fn()} onToggleTask={vi.fn()}
      onGenerate={vi.fn()} onJumpToPage={vi.fn()} />);
    expect(screen.queryByText("复述自检提示")).toBeNull();
    expect(screen.queryByText("优先精读对象")).toBeNull();
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    const text = screen.getByLabelText("精读路线面板").textContent!;
    expect(text.indexOf("前置对齐")).toBeLessThan(text.indexOf("鸟瞰 + 决策"));
  });

  it("supports a theorem as the priority object with page-only navigation", () => {
    const onJump = vi.fn();
    const content = {...mockProjection.content!, oneChart: {
      label:"定理 2", page:4, reason:"先辨认条件，再返回核对证明。"
    }};
    render(<ReadingRoadmapPanel open={true} projection={{...mockProjection, content}}
      progress={[]} activeJobId={null} onClose={vi.fn()} onToggleTask={vi.fn()}
      onGenerate={vi.fn()} onJumpToPage={onJump} />);
    fireEvent.click(screen.getByRole("button", {name:/定理 2/}));
    expect(onJump).toHaveBeenCalledWith(4, undefined, "定理 2");
  });

  it("renders textbook learning objectives and prerequisites", () => {
    const textbookProjection: ReadingRoadmapProjection = {
      ...mockProjection,
      content: {
        ...mockProjection.content!,
        learningObjectives: ["能推导梯度下降更新规则", "能解释过拟合的成因"],
        prerequisites: ["线性代数基础", "微积分基础"],
      },
    };
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={textbookProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );

    expect(screen.getByText("📚 前置知识")).toBeInTheDocument();
    expect(screen.getByText("线性代数基础")).toBeInTheDocument();
    expect(screen.getByText("🎯 学习目标")).toBeInTheDocument();
    expect(screen.getByText("能推导梯度下降更新规则")).toBeInTheDocument();
  });

  it("renders inactive state when closed", () => {
    const { container } = render(
      <ReadingRoadmapPanel
        open={false}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(container.querySelector(".reading-roadmap-overlay.open")).toBeNull();
    expect(
      container
        .querySelector(".reading-roadmap-overlay")
        ?.getAttribute("aria-hidden"),
    ).toBe("true");
  });

  it("renders pass titles, tasks, and elevator pitch when open", () => {
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(screen.getByText(/前置对齐/)).toBeTruthy();
    expect(screen.getByText(/鸟瞰 \+ 决策/)).toBeTruthy();
    expect(screen.getByText(/Self-Attention/)).toBeTruthy();
    expect(screen.getByText("复述自检提示")).toBeTruthy();
    expect(screen.getByText("优先精读对象")).toBeTruthy();
  });

  it("calls onToggleTask when task checkbox is clicked", () => {
    const onToggle = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={onToggle}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    const checkboxes = screen.getAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    expect(onToggle).toHaveBeenCalledWith("p0-t1", true);
  });

  it("calls onJumpToPage when evidence pill is clicked", () => {
    const onJump = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={onJump}
      />,
    );
    const pill = screen.getByText(/p\.1 · Abstract/);
    fireEvent.click(pill);
    expect(onJump).toHaveBeenCalledWith(1, "b-1", "p.1 · Abstract");
  });

  it("shows generate button when no projection is available", () => {
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={null}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: /生成精读路线/i })).toBeTruthy();
  });

  it("marks tasks as completed correctly from progress prop", () => {
    const progress: RoadmapProgressEntry[] = [
      { taskId: "p0-t1", completed: true, completedAt: "2026-08-20T01:00:00Z" },
    ];
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={progress}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    const checkboxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(checkboxes[0].checked).toBe(true);
    expect(checkboxes[1].checked).toBe(false);
  });

  it("shows active job generation state when job is running", () => {
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={null}
        progress={[]}
        activeJobId="job-123"
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    expect(screen.getByText(/精读导师正在安排阅读顺序/i)).toBeTruthy();
  });

  it("handles pin toggle correctly", () => {
    const onTogglePin = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        pinned={false}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onTogglePin={onTogglePin}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );
    const pinBtn = screen.getByRole("button", { name: /固定面板/ });
    fireEvent.click(pinBtn);
    expect(onTogglePin).toHaveBeenCalledWith(true);
  });

  it("renders math formulas and does not mistake single tilde ranges for strikethrough", () => {
    const mathProjection: ReadingRoadmapProjection = {
      ...mockProjection,
      content: {
        ...mockProjection.content!,
        passes: [
          {
            passNumber: 0,
            title: "公式推导",
            subtitle: "推导细节",
            timeBudget: "20 min",
            exitCriteria: "掌握 $E=mc^2$ 的物理意义",
            tasks: [
              {
                id: "math-t1",
                text: "推导容量 $p_{\\max} \\sim N_{\\text{syn}}$（式 4.33~4.40，图 4.18a），随后推导崩溃（式 4.46~4.47，图 4.18b）",
                timeMinutes: 25,
                required: true,
                completionCriteria: "对比二值突触 $p$ 与上限 $p_{\\text{max}} \\sim \\frac{\\ln(q\\sqrt{N})}{q}$",
                selfCheckQuestions: ["为什么学习率 $q$ 较大时容量极小？"],
                evidence: [],
              },
            ],
          },
        ],
      },
    };

    const { container } = render(
      <ReadingRoadmapPanel
        open={true}
        projection={mathProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={vi.fn()}
        onJumpToPage={vi.fn()}
      />,
    );

    // Verify no <del> tags are created from 4.33~4.40 and 4.46~4.47
    expect(container.querySelectorAll("del").length).toBe(0);

    // Verify formulas in task text, completionCriteria, and selfCheckQuestions render KaTeX elements
    const katexNodes = container.querySelectorAll(".katex");
    expect(katexNodes.length).toBeGreaterThanOrEqual(4);

    // Verify the text content includes the range ~ intact
    expect(container.textContent).toContain("4.33~4.40");
    expect(container.textContent).toContain("4.46~4.47");
  });

  it("renders regenerate button and requires secondary confirmation before calling onGenerate", () => {
    const onGenerate = vi.fn();
    render(
      <ReadingRoadmapPanel
        open={true}
        projection={mockProjection}
        progress={[]}
        activeJobId={null}
        onClose={vi.fn()}
        onToggleTask={vi.fn()}
        onGenerate={onGenerate}
        onJumpToPage={vi.fn()}
      />,
    );

    // Regenerate button is displayed in the header
    const regenBtn = screen.getByRole("button", { name: "重新生成精读路线" });
    expect(regenBtn).toBeInTheDocument();

    // Clicking it opens the confirmation modal
    fireEvent.click(regenBtn);
    const modal = screen.getByRole("dialog");
    expect(modal).toBeInTheDocument();
    expect(
      screen.getByText("重新生成 精读路线 (Three-Pass Roadmap)"),
    ).toBeInTheDocument();
    expect(modal).toHaveTextContent("Attention Is All You Need");

    // Clicking cancel closes the modal without generating
    const cancelBtn = screen.getByRole("button", { name: "取消" });
    fireEvent.click(cancelBtn);
    expect(
      screen.queryByText("重新生成 精读路线 (Three-Pass Roadmap)"),
    ).toBeNull();
    expect(onGenerate).not.toHaveBeenCalled();

    // Clicking confirm calls onGenerate
    fireEvent.click(regenBtn);
    const confirmBtn = screen.getByRole("button", { name: "确认重新生成" });
    fireEvent.click(confirmBtn);
    expect(onGenerate).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByText("重新生成 精读路线 (Three-Pass Roadmap)"),
    ).toBeNull();
  });
});
