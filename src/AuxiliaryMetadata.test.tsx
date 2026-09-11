import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AuxiliaryMetadata, AuxiliarySources } from "./AuxiliaryMetadata";
import ArtifactPanel from "./ArtifactPanel";
import type { ArtifactProjection } from "./types";
import { renderWithLocale as render } from "./i18n/testUtils";
import sample from "../src-tauri/tests/fixtures/auxiliary-v2.metadata.json";
function artifact(
  content: Record<string, unknown>,
  kind = "metadata",
): ArtifactProjection {
  return {
    id: "artifact-v2",
    paperId: "p",
    revisionId: "r",
    ocrRevisionId: null,
    kind,
    objectKey: "",
    version: 2,
    status: "ready",
    content,
    overrides: {},
    evidence: [],
    dependencySnapshot: {},
    providerNodeId: null,
    createdAt: "2026-09-08",
  };
}
function metadata() {
  return {
    ...structuredClone(sample.metadata),
    _model: structuredClone(sample.metadata),
    _pinned: [],
    _display: {
      title: "Inference",
      titleSource: "本次提取",
      authors: ["A. Reader"],
      year: 2024,
      yearLabel: "所属出版物·出版",
      abstractIndex: 0,
    },
  };
}
describe("structured auxiliary artifacts", () => {
  it("keeps raw fields and gap markers, with page navigation only for known PDF pages", async () => {
    const user = userEvent.setup();
    const jump = vi.fn();
    render(<AuxiliaryMetadata artifact={artifact(metadata())} onJump={jump} />);
    expect(screen.getByText("〔原文缺损〕")).toBeInTheDocument();
    await user.click(screen.getByText("查看出处（1）"));
    await user.click(screen.getByRole("button", { name: "PDF 第 2 页" }));
    expect(jump).toHaveBeenCalledWith(2);
    expect(screen.queryByText("/document/title")).not.toBeInTheDocument();
  });
  it("saves an explicit clear with the displayed artifact identity and preserves original evidence", async () => {
    const user = userEvent.setup();
    const update = vi.fn().mockResolvedValue(undefined);
    const content = metadata();
    render(
      <AuxiliaryMetadata artifact={artifact(content)} onUpdate={update} />,
    );
    await user.click(screen.getAllByRole("button", { name: "编辑标题" })[0]);
    const editor = screen.getByRole("textbox", { name: "编辑标题" });
    await user.clear(editor);
    await user.click(screen.getByRole("button", { name: "保存并保留" }));
    expect(update).toHaveBeenCalledWith(
      "r",
      expect.objectContaining({
        _editBaseArtifactId: "artifact-v2",
        document: expect.objectContaining({ title: null }),
        _model: sample.metadata,
        sources: sample.metadata.sources,
      }),
      ["/document/title"],
    );
  });
  it("keeps original text even when it is also an enum word, and marks invalidated source associations", () => {
    const content = metadata();
    content.document.title = "book";
    content._display.title = "book";
    render(
      <AuxiliaryMetadata
        artifact={artifact({
          ...content,
          _sourceLinks: [[null]],
          _issueLinks: [false],
          _effectiveIssues: [],
        })}
      />,
    );
    expect(screen.getByRole("heading", { name: "book" })).toBeInTheDocument();
    expect(
      screen.getByText("部分出处仅对应模型原稿，人工修改后的关联尚未核对。"),
    ).toBeInTheDocument();
    expect(screen.queryByText("〔原文缺损〕")).not.toBeInTheDocument();
  });
  it("edits and deletes homonymous rows independently while retaining usage and sources", async () => {
    const user = userEvent.setup();
    const update = vi.fn().mockResolvedValue(undefined);
    const source = {
      pageNumber: 2,
      locator: "Definition",
      excerpt: "X",
      supports: "本处定义。",
    };
    const rows = [
      {
        _id: "concept-a",
        term: "X",
        definition: "第一含义",
        aliases: ["Shared"],
        usage: "方法章节",
        sources: [source],
      },
      {
        _id: "concept-b",
        term: "X",
        definition: "第二含义",
        aliases: ["Shared"],
        usage: "附录",
        sources: [],
      },
    ];
    const current = artifact(
      { _format: "auxiliary-v2", entries: rows, _pinned: [] },
      "glossary",
    );
    render(
      <ArtifactPanel
        artifacts={[current]}
        activeArtifactId={current.id}
        activeBlockId={null}
        displayCropSrc=""
        lensQa={[]}
        lensQaBusy={false}
        onSelect={vi.fn()}
        onJump={vi.fn()}
        onAskLens={vi.fn()}
        onTransferLens={vi.fn()}
        onSetOverride={vi.fn()}
        onGenerateBrief={vi.fn()}
        onUpdateOrientationTable={update}
      />,
    );
    const first = screen.getAllByRole("button", { name: "编辑 X" })[0];
    await user.click(first);
    const usage = screen.getByRole("textbox", { name: "编辑 X 的文中用法" });
    await user.clear(usage);
    await user.type(usage, "人工核对的方法用法");
    await user.click(screen.getByRole("button", { name: "保存词条" }));
    expect(update).toHaveBeenCalledWith(
      "r",
      "glossary",
      [
        expect.objectContaining({
          _id: "concept-a",
          usage: "人工核对的方法用法",
          sources: [source],
        }),
        rows[1],
      ],
      ["concept-a"],
      current.id,
    );
    await user.click(screen.getAllByRole("button", { name: "删除 X" })[0]);
    expect(update).toHaveBeenLastCalledWith(
      "r",
      "glossary",
      [rows[1]],
      [],
      current.id,
    );
  });
  it("does not fabricate page jumps for an unlocated source", async () => {
    const user = userEvent.setup();
    render(
      <AuxiliarySources
        sources={[
          {
            pageNumber: null,
            locator: "Definitions",
            excerpt: "X",
            supports: "原文定义。",
          },
        ]}
        onJump={vi.fn()}
      />,
    );
    await user.click(screen.getByText("查看出处（1）"));
    expect(screen.getByText("页序未知")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
