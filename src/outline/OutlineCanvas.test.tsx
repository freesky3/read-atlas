import { useState } from "react";
import { screen, within } from "@testing-library/react";
import { renderWithLocale as render } from "../i18n/testUtils";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import OutlineCanvas from "./OutlineCanvas";
import type { OutlineGraph, OutlineNode } from "../types";

const graph: OutlineGraph = {
  title: "Map",
  summary: "A",
  nodes: [
    {
      nodeId: "n1",
      roleClass: "method_design",
      title: "RNN 模拟",
      takeaway: "用 RNN 做模拟",
      importance: "core",
      sourceUnitIds: [],
      evidenceIds: ["fig-1"],
      confidence: 1,
    },
    {
      nodeId: "n2",
      roleClass: "result_finding",
      title: "误差下降",
      takeaway: "验证有效",
      importance: "supporting",
      sourceUnitIds: [],
      evidenceIds: ["p-2"],
      confidence: 1,
    },
  ],
  edges: [
    {
      edgeId: "n1-n2",
      sourceNodeId: "n1",
      targetNodeId: "n2",
      tier: "narrative",
      relationClass: "dependency",
      label: "then",
      rationale: "",
      evidenceIds: [],
    },
    {
      edgeId: "n2-n1",
      sourceNodeId: "n2",
      targetNodeId: "n1",
      tier: "cross_link",
      relationClass: "comparison",
      label: "vs",
      rationale: "",
      evidenceIds: [],
    },
  ],
};

const catalog = [
  {
    id: "fig-1",
    page: 3,
    type: "figure",
    blockIndex: 2,
    bbox: [10, 10, 100, 100] as [number, number, number, number],
  },
];

describe("OutlineCanvas", () => {
  it("renders a compact draggable map without takeaways or an inspector", async () => {
    render(
      <OutlineCanvas
        graph={graph}
        catalog={catalog}
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
      />,
    );
    expect(await screen.findByTestId("rf__wrapper")).toHaveAttribute(
      "role",
      "application",
    );
    expect(screen.getByTestId("rf__node-n1")).toHaveClass("draggable");
    expect(screen.getByLabelText("节点编号 1")).toHaveClass("outline-reading-number");
    expect(screen.getByRole("button", { name: "放大" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "缩小" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看整张地图" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "回到可读起点" })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: "Mini Map" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("rf__edge-n2-n1")).not.toBeInTheDocument();
    expect(screen.queryByText("用 RNN 做模拟")).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "论证地图" })).toHaveStyle({
      flexGrow: "1",
    });
  });

  it("opens a resizable inspector on select and jumps PDF only from the detail action", async () => {
    const user = userEvent.setup();
    const onJump = vi.fn();
    const onClear = vi.fn();
    render(
      <OutlineCanvas
        graph={graph}
        catalog={catalog}
        selectedNodeId="n1"
        inspectorWidth={300}
        onSelectNode={vi.fn()}
        onJumpEvidence={onJump}
        onClearSelection={onClear}
      />,
    );
    expect(await screen.findByRole("complementary")).toHaveTextContent("用 RNN 做模拟");
    expect(screen.getByRole("separator", { name: "调整详情宽度" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "跳到原文" }));
    expect(onJump).toHaveBeenCalledWith({
      blockId: "fig-1",
      purpose: "内容出处",
    });
    await user.click(screen.getByRole("button", { name: "收起详情" }));
    expect(onClear).toHaveBeenCalled();
  });

  it("remounts React Flow when the canvas identity changes", async () => {
    const { rerender } = render(
      <OutlineCanvas
        canvasKey="head-1"
        graph={graph}
        catalog={catalog}
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
      />,
    );
    const first = await screen.findByTestId("rf__wrapper");
    expect(document.querySelector("[data-outline-canvas]")).toHaveAttribute(
      "data-outline-canvas",
      "head-1",
    );

    const deepNode: OutlineNode = {
      nodeId: "d1",
      roleClass: "method_design",
      title: "局部节点",
      takeaway: "只看这一支",
      importance: "core",
      sourceUnitIds: [],
      evidenceIds: [],
      confidence: 1,
    };
    rerender(
      <OutlineCanvas
        canvasKey="deep-dive"
        graph={{ title: "Local", summary: "", nodes: [deepNode], edges: [] }}
        catalog={catalog}
        selectedNodeId={null}
        onSelectNode={vi.fn()}
        onJumpEvidence={vi.fn()}
      />,
    );
    const second = await screen.findByTestId("rf__wrapper");
    expect(second).not.toBe(first);
    expect(document.querySelector("[data-outline-canvas]")).toHaveAttribute(
      "data-outline-canvas",
      "deep-dive",
    );
    expect(screen.getByText("局部节点")).toBeInTheDocument();
  });

  it("selects a node, jumps to evidence, and can reveal cross_link edges", async () => {
    const user = userEvent.setup();
    const onSelectNode = vi.fn();
    const onJumpEvidence = vi.fn();
    render(
      <OutlineCanvas
        graph={graph}
        catalog={catalog}
        selectedNodeId="n1"
        onSelectNode={onSelectNode}
        onJumpEvidence={onJumpEvidence}
      />,
    );
    expect(screen.queryByRole("button", { name: "生成局部图" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /p\.3/ }));
    expect(onJumpEvidence).toHaveBeenCalledWith({
      blockId: "fig-1",
      purpose: "内容出处",
    });
    await user.click(screen.getByRole("checkbox", { name: "显示交叉引用" }));
    expect(screen.getByTestId("rf__edge-n2-n1")).toBeInTheDocument();
  });
});

const openGraph: OutlineGraph = {
  title: "采样与边界", summary: "可核对的关系",
  nodes: [
    { nodeId: "a", title: "采样设计", takeaway: "根据变化采样", references: [{ pageNumber: 3, purpose: "定义" }] },
    { nodeId: "b", title: "噪声条件", takeaway: "收益依赖估计质量", references: [{ blockId: "fig-1", pageNumber: 3, purpose: "观察" }] },
  ],
  edges: [
    { edgeId: "limit", sourceNodeId: "b", targetNodeId: "a", direction: "directed", label: "限定有效条件", rationale: "噪声实验限定收益范围", references: [{ pageNumber: 7, purpose: "边界依据" }], uncertainty: "机制未直接检验" },
    { edgeId: "compare", sourceNodeId: "a", targetNodeId: "b", direction: "undirected", label: "对照两种设置", rationale: "在共同采样预算下比较", references: [{ pageNumber: 4, purpose: "比较设置" }] },
  ],
};

describe("OutlineCanvas v4 interactions", () => {
  it("keeps a relation selected when clearing a previously selected node, including after reset", async () => {
    const user = userEvent.setup();
    const jump = vi.fn();
    function Harness() {
      const [selected, setSelected] = useState<string | null>("a");
      return <OutlineCanvas graph={openGraph} catalog={catalog} selectedNodeId={selected}
        onSelectNode={node => setSelected(node.nodeId)} onClearSelection={() => setSelected(null)} onJumpEvidence={jump} />;
    }
    render(<Harness />);
    await user.click(await screen.findByRole("button", { name: "关系：限定有效条件" }));
    expect(screen.getByRole("complementary")).toHaveTextContent("噪声实验限定收益范围");
    expect(screen.getByRole("complementary")).toHaveTextContent("机制未直接检验");
    expect(screen.getByRole("button", { name: "关系：限定有效条件" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "回到可读起点" }));
    await user.click(screen.getByRole("button", { name: "关系：对照两种设置" }));
    expect(screen.getByRole("complementary")).toHaveTextContent("无向 · 在共同采样预算下比较");
    await user.click(screen.getByRole("button", { name: /页级定位 · p.4/ }));
    expect(jump).toHaveBeenCalledWith({ blockId: undefined, pageNumber: 4 });
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("opens relation details from the keyboard and provides a searchable evidence list", async () => {
    const user = userEvent.setup(); const jump = vi.fn();
    render(<OutlineCanvas graph={openGraph} catalog={catalog} selectedNodeId={null} onSelectNode={vi.fn()} onJumpEvidence={jump} />);
    const relation = await screen.findByRole("button", { name: "关系：限定有效条件" });
    relation.focus(); await user.keyboard("{Enter}");
    expect(screen.getByRole("complementary")).toHaveTextContent("限定有效条件");
    await user.click(screen.getByRole("button", { name: "节点与关系列表" }));
    const list = screen.getByRole("region", { name: "地图列表" });
    await user.type(within(list).getByRole("textbox", { name: "搜索节点与关系" }), "噪声实验");
    expect(within(list).getByText("噪声实验限定收益范围")).toBeInTheDocument();
    expect(within(list).queryByText("在共同采样预算下比较")).not.toBeInTheDocument();
    await user.click(within(list).getByRole("button", { name: /页级定位 · p.7/ }));
    expect(jump).toHaveBeenCalledWith({ pageNumber: 7, purpose: "边界依据" });
  });
});
