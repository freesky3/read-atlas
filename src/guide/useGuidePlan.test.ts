import { act, renderHook } from "@testing-library/react";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { desktopClient } from "../desktopClient";
import type { GuideCharacterSettings, GuidePlan, GuideProjection } from "../types";
import { useGuidePlan } from "./useGuidePlan";

const settings: GuideCharacterSettings = { characters: ["a", "b", "c"].map((id) => ({
  id, revision: 1, displayName: id, workTitle: "", characterVersion: "", description: "",
  avatarAssetId: null, inkColor: "#123456", personality: "", readingHabits: "", expressionStyle: "",
  avoidances: "", exampleNotes: [], presetId: null, presetVersion: null, createdAt: "", updatedAt: "",
})),
  defaultCharacterIds: ["c"], presetCasts: [], storeRevision: 1, schemaVersion: 1 };
const projection = (revisionId: string): GuideProjection => ({ revisionId, status: "ready_to_plan",
  ocrRevisionId: "ocr", head: null, activeJobId: null, hasPaperRoot: false,
  preferredCharacterIds: revisionId.startsWith("A") ? ["a"] : ["b"] });
const plan = (revisionId: string, ids: string[]): GuidePlan => ({ revisionId, ocrRevisionId: "ocr",
  catalogDigest: "catalog", model: "fake", pageCount: 1, batchCount: 1, understandCalls: 1,
  annotationCalls: 1, repairCalls: 2, reusedOutline: false, hasPaperRoot: false, supportsNativePdf: true,
  estimatedCost: null, planId: ids.join("-"), planDigest: ids.join("-"),
  guideProtocol: "reading-guide-desktop-v2", characterIds: ids });

beforeEach(() => {
  vi.spyOn(desktopClient, "open").mockImplementation(async (name, args: any) =>
    (name === "get_guide_character_settings" ? settings : projection(args.revisionId)) as any);
  vi.spyOn(desktopClient, "command").mockImplementation(async (_name, args: any) =>
    plan(args.request.revisionId, args.request.characterIds) as any);
});
afterEach(() => vi.restoreAllMocks());

describe("guide generation panel", () => {
  it("uses per-document persisted preferences after cancel, document change and remount", async () => {
    const onProjection = vi.fn();
    const { result, rerender, unmount } = renderHook(({ doc, rev }) => useGuidePlan(doc, rev, onProjection, vi.fn()),
      { initialProps: { doc: "paperA", rev: "A1" } });
    await act(async () => result.current.open());
    expect(result.current.castIds).toEqual(["a"]);
    await act(async () => result.current.changeCast(["c"]));
    expect(result.current.plan?.planDigest).toBe("c");
    act(() => result.current.cancel());
    rerender({ doc: "paperB", rev: "B1" });
    await act(async () => result.current.open());
    expect(result.current.castIds).toEqual(["b"]);
    rerender({ doc: "paperA", rev: "A2" });
    await act(async () => result.current.open());
    expect(result.current.castIds).toEqual(["a"]);
    unmount();
    const reopened = renderHook(() => useGuidePlan("paperA", "A2", vi.fn(), vi.fn()));
    await act(async () => reopened.result.current.open());
    expect(reopened.result.current.castIds).toEqual(["a"]);
    expect(desktopClient.command).not.toHaveBeenCalledWith("save_guide_default_cast", expect.anything());
  });

  it("ignores outdated plan responses and never permits an empty or deleted cast", async () => {
    const { result } = renderHook(() => useGuidePlan("paperA", "A1", vi.fn(), vi.fn()));
    await act(async () => result.current.open());
    let resolveOld!: (value: any) => void;
    vi.mocked(desktopClient.command).mockImplementationOnce(() => new Promise((resolve) => { resolveOld = resolve; }));
    let old!: Promise<void>;
    act(() => { old = result.current.changeCast(["b"]); });
    expect(result.current.canStart).toBe(false);
    await act(async () => result.current.changeCast(["c"]));
    await act(async () => { resolveOld(plan("A1", ["b"])); await old; });
    expect(result.current.plan?.characterIds).toEqual(["c"]);
    await act(async () => result.current.changeCast([]));
    expect(result.current.castIds).toEqual([]);
    expect(result.current.canStart).toBe(false);
    await act(async () => result.current.changeCast(["deleted"]));
    expect(result.current.castIds).toEqual(["deleted"]);
    expect(result.current.canStart).toBe(false);
  });

  it("does not reopen a cancelled panel when a pending local plan finishes", async () => {
    let resolvePlan!: (value: any) => void;
    vi.mocked(desktopClient.command).mockImplementationOnce(() => new Promise((resolve) => { resolvePlan = resolve; }));
    const { result } = renderHook(() => useGuidePlan("paperA", "A1", vi.fn(), vi.fn()));
    let opening!: Promise<void>;
    await act(async () => { opening = result.current.open(); });
    act(() => result.current.cancel());
    await act(async () => { resolvePlan(plan("A1", ["a"])); await opening; });
    expect(result.current.confirming).toBe(false);
    expect(result.current.plan).toBe(null);
  });
});
