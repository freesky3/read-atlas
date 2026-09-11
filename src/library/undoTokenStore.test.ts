import { afterEach, describe, expect, it } from "vitest";
import {
  clearUndoTokens,
  consumeUndoToken,
  peekUndoToken,
  rememberUndoToken,
} from "./undoTokenStore";

describe("undoTokenStore", () => {
  afterEach(() => clearUndoTokens());

  it("remembers a token until expiry and consumes it once", () => {
    rememberUndoToken("batch-1", "plain-token", new Date(Date.now() + 60_000).toISOString());
    expect(peekUndoToken("batch-1")).toBe("plain-token");
    expect(consumeUndoToken("batch-1")).toBe("plain-token");
    expect(peekUndoToken("batch-1")).toBeNull();
    expect(consumeUndoToken("batch-1")).toBeNull();
  });

  it("drops expired tokens without returning them", () => {
    rememberUndoToken("batch-2", "late", new Date(Date.now() - 1_000).toISOString());
    expect(peekUndoToken("batch-2")).toBeNull();
  });

  it("ignores empty tokens", () => {
    rememberUndoToken("batch-3", null);
    rememberUndoToken("batch-3", "");
    expect(peekUndoToken("batch-3")).toBeNull();
  });
});
