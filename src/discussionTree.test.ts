import { describe, expect, it } from "vitest";
import {
  countTurnSubtree,
  layoutDiscussionTree,
  layoutDiscussionTurns,
  visibleDiscussionNodes,
} from "./discussionTree";
import type { Message } from "./types";

function message(
  id: string,
  parentId: string | null,
  createdAt: string,
  role: Message["role"] = "user",
): Message {
  return {
    id,
    threadId: "thread-1",
    parentId,
    role,
    content: id,
    citations: [],
    blockQuotes: [],
    status: "complete",
    createdAt,
  };
}

describe("layoutDiscussionTree", () => {
  it("indents children by parentId instead of list order", () => {
    const laid = layoutDiscussionTree(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a1", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("u2", "a1", "2026-08-17T00:00:02Z"),
        message("a2", "u2", "2026-08-17T00:00:03Z", "assistant"),
        message("u-branch", "a1", "2026-08-17T00:00:04Z"),
        message("a-branch", "u-branch", "2026-08-17T00:00:05Z", "assistant"),
      ],
      "a2",
    );

    expect(laid.map((node) => [node.message.id, node.depth])).toEqual([
      ["u1", 0],
      ["a1", 1],
      ["u2", 2],
      ["a2", 3],
      ["u-branch", 2],
      ["a-branch", 3],
    ]);
    expect(laid.find((node) => node.message.id === "a1")?.isBranchPoint).toBe(
      true,
    );
    expect(
      laid.filter((node) => node.onActivePath).map((node) => node.message.id),
    ).toEqual(["u1", "a1", "u2", "a2"]);
    expect(laid.find((node) => node.message.id === "a2")?.isHead).toBe(true);
    expect(
      laid.find((node) => node.message.id === "a-branch")?.onActivePath,
    ).toBe(false);
  });

  it("keeps the linear transcript on the active path plus the latest unfinished child", () => {
    const laid = layoutDiscussionTree(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a-old", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("a-new", "u1", "2026-08-17T00:00:02Z", "assistant"),
      ],
      "u1",
    );
    laid[1]!.message.status = "failed";
    laid[2]!.message.status = "streaming";
    expect(
      visibleDiscussionNodes(laid, "u1").map((node) => node.message.id),
    ).toEqual(["u1", "a-new"]);
  });

  it("drops failed replies from the tree entirely", () => {
    const laid = layoutDiscussionTree(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        {
          ...message("a-fail", "u1", "2026-08-17T00:00:01Z", "assistant"),
          status: "failed",
        },
        message("a-ok", "u1", "2026-08-17T00:00:02Z", "assistant"),
      ],
      "a-ok",
    );
    expect(laid.map((node) => node.message.id)).toEqual(["u1", "a-ok"]);
  });

  it("treats messages with a missing parent as roots", () => {
    const laid = layoutDiscussionTree(
      [message("orphan", "gone", "2026-08-17T00:00:00Z")],
      null,
    );
    expect(laid).toHaveLength(1);
    expect(laid[0]?.depth).toBe(0);
  });
});

describe("layoutDiscussionTurns", () => {
  it("binds a question and its answer as one node", () => {
    const turns = layoutDiscussionTurns(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a1", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("u2", "a1", "2026-08-17T00:00:02Z"),
        message("a2", "u2", "2026-08-17T00:00:03Z", "assistant"),
        message("u-branch", "a1", "2026-08-17T00:00:04Z"),
        message("a-branch", "u-branch", "2026-08-17T00:00:05Z", "assistant"),
      ],
      "a2",
    );

    expect(
      turns.map((turn) => [turn.user.id, turn.assistant?.id, turn.depth]),
    ).toEqual([
      ["u1", "a1", 0],
      ["u2", "a2", 1],
      ["u-branch", "a-branch", 1],
    ]);
    expect(turns.find((turn) => turn.user.id === "u1")?.isBranchPoint).toBe(
      true,
    );
    expect(turns.find((turn) => turn.user.id === "u2")?.isHead).toBe(true);
    expect(turns.find((turn) => turn.user.id === "u2")?.onActivePath).toBe(true);
    expect(turns.find((turn) => turn.user.id === "u-branch")?.onActivePath).toBe(
      false,
    );
    expect(countTurnSubtree(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a1", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("u2", "a1", "2026-08-17T00:00:02Z"),
        message("a2", "u2", "2026-08-17T00:00:03Z", "assistant"),
        message("u-branch", "a1", "2026-08-17T00:00:04Z"),
        message("a-branch", "u-branch", "2026-08-17T00:00:05Z", "assistant"),
      ],
      "u1",
    )).toBe(3);
    expect(countTurnSubtree(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a1", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("u2", "a1", "2026-08-17T00:00:02Z"),
        message("a2", "u2", "2026-08-17T00:00:03Z", "assistant"),
        message("u-branch", "a1", "2026-08-17T00:00:04Z"),
        message("a-branch", "u-branch", "2026-08-17T00:00:05Z", "assistant"),
      ],
      "u2",
    )).toBe(1);
  });

  it("keeps a regenerate sibling as its own turn", () => {
    const turns = layoutDiscussionTurns(
      [
        message("u1", null, "2026-08-17T00:00:00Z"),
        message("a1", "u1", "2026-08-17T00:00:01Z", "assistant"),
        message("u1b", null, "2026-08-17T00:00:02Z"),
        message("a1b", "u1b", "2026-08-17T00:00:03Z", "assistant"),
      ],
      "a1b",
    );
    expect(turns).toHaveLength(2);
    expect(turns[0]?.user.id).toBe("u1");
    expect(turns[1]?.user.id).toBe("u1b");
    expect(turns[1]?.isHead).toBe(true);
  });
});
