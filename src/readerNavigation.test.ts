import { describe, expect, it } from "vitest";
import {
  canGoBackReader,
  clearReaderHistory,
  jumpWithHistory,
  popReaderLocation,
  pushReaderLocation,
  sameReaderLocation,
  type ReaderLocation,
} from "./readerNavigation";

const location = (overrides: Partial<ReaderLocation> = {}): ReaderLocation => ({
  page: 4,
  pageOffset: 123,
  zoom: 110,
  rotation: 0,
  focusedBlockId: "block-4",
  rightTab: "discussion",
  activeArtifactId: null,
  workspaceLayout: "pdf_discussion",
  ...overrides,
});

describe("reader navigation history", () => {
  it("pushes a pre-jump location and pops it as the return target", () => {
    const before = location();
    const history = pushReaderLocation([], before);

    expect(canGoBackReader(history)).toBe(true);
    const popped = popReaderLocation(history);
    expect(popped.location).toEqual(before);
    expect(popped.history).toEqual([]);
  });

  it("does not add consecutive duplicate locations", () => {
    const before = location();
    const once = pushReaderLocation([], before);
    const twice = pushReaderLocation(once, { ...before });

    expect(twice).toBe(once);
    expect(twice).toHaveLength(1);
  });

  it("records the current location only for a real destination change", () => {
    const current = location({ page: 2 });
    const same = jumpWithHistory([], current, { ...current });
    expect(same).toEqual({ history: [], changed: false });

    const changed = jumpWithHistory([], current, location({ page: 9 }));
    expect(changed.changed).toBe(true);
    expect(changed.history).toEqual([current]);
  });

  it("keeps the newest entries within the configured limit", () => {
    const history = [
      location({ page: 1 }),
      location({ page: 2 }),
      location({ page: 3 }),
    ];

    const next = pushReaderLocation(history, location({ page: 4 }), 2);
    expect(next.map((entry) => entry.page)).toEqual([3, 4]);
    expect(history.map((entry) => entry.page)).toEqual([1, 2, 3]);
  });

  it("supports a multi-hop back chain", () => {
    let history = [] as readonly ReaderLocation[];
    history = pushReaderLocation(history, location({ page: 1 }));
    history = pushReaderLocation(history, location({ page: 2 }));
    history = pushReaderLocation(history, location({ page: 3 }));

    const first = popReaderLocation(history);
    const second = popReaderLocation(first.history);
    expect(first.location?.page).toBe(3);
    expect(second.location?.page).toBe(2);
    expect(second.history.map((entry) => entry.page)).toEqual([1]);
  });

  it("treats an empty history as a no-op", () => {
    const empty: readonly ReaderLocation[] = [];
    expect(canGoBackReader(empty)).toBe(false);
    expect(popReaderLocation(empty)).toEqual({ history: empty, location: null });
    expect(clearReaderHistory()).toEqual([]);
  });

  it("compares tracked reader context, including focus and panel state", () => {
    expect(sameReaderLocation(location(), location())).toBe(true);
    expect(
      sameReaderLocation(
        location({ focusedBlockId: null }),
        location({ focusedBlockId: "block-4" }),
      ),
    ).toBe(false);
    expect(
      sameReaderLocation(
        location({ rightTab: "discussion" }),
        location({ rightTab: "artifacts" }),
      ),
    ).toBe(false);
    expect(
      sameReaderLocation(
        location({ selectedAnnotationId: null }),
        location({ selectedAnnotationId: "annotation-1" }),
      ),
    ).toBe(false);
    expect(sameReaderLocation(null, undefined)).toBe(false);
  });
});
