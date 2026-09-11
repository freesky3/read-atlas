import { describe, expect, it } from "vitest";
import { buildSendChatInvokeArgs } from "./sendChat";

describe("send_chat IPC contract", () => {
  it("wraps the payload in the Tauri `request` key", () => {
    expect(
      buildSendChatInvokeArgs({
        revisionId: "rev-1",
        threadId: "thread-1",
        parentId: "msg-parent",
        question: "What is the claim?",
        page: 3,
        blockIds: ["block-a", "block-b"],
      }),
    ).toEqual({
      request: {
        revisionId: "rev-1",
        threadId: "thread-1",
        parentId: "msg-parent",
        question: "What is the claim?",
        page: 3,
        blockIds: ["block-a", "block-b"],
        regenerateFromId: null,
      },
    });
  });

  it("keeps regenerate on the same request object", () => {
    const args = buildSendChatInvokeArgs({
      revisionId: "rev-1",
      threadId: "thread-1",
      parentId: null,
      question: "",
      page: 1,
      blockIds: [],
      regenerateFromId: "assistant-9",
    });
    expect(args.request.regenerateFromId).toBe("assistant-9");
    expect(Object.keys(args)).toEqual(["request"]);
  });
});
