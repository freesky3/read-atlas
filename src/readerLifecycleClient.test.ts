import { describe, expect, it } from "vitest";
import { createMemoryDesktopClient } from "./desktopClient";

describe("memory reader lifecycle client", () => {
  it("records activity, exposes revision-scoped progress, and transitions status", async () => {
    const client = createMemoryDesktopClient();
    const initial = await client.open<{
      lifecycle: { status: string; version: number };
      engagement: unknown;
      session: unknown;
    }>("get_reading_context", { paperId: "p1", revisionId: "r1" });
    expect(initial.lifecycle).toMatchObject({ status: "unread", version: 0 });
    expect(initial.engagement).toBeNull();

    const activity = await client.command<{
      lifecycle: { status: string; version: number };
      engagement: { revisionId: string; furthestPage: number };
    }>("record_reading_activity", {
      request: {
        paperId: "p1",
        revisionId: "r1",
        pageNumber: 7,
        pageCount: 20,
        occurredAt: "2026-08-31T00:00:00.000Z",
      },
    });
    expect(activity.lifecycle).toMatchObject({ status: "reading", version: 1 });
    expect(activity.engagement).toMatchObject({ revisionId: "r1", furthestPage: 7 });

    const done = await client.command<{ status: string; version: number }>(
      "update_reading_lifecycle",
      {
        request: {
          paperId: "p1",
          expectedVersion: 1,
          patch: { status: "read", favorite: true },
          occurredAt: "2026-08-31T01:00:00.000Z",
        },
      },
    );
    expect(done).toMatchObject({ status: "read", version: 2 });

    await expect(
      client.command("update_reading_lifecycle", {
        request: { paperId: "p1", expectedVersion: 1, patch: { status: "abandoned" } },
      }),
    ).rejects.toThrow("stale_lifecycle_version");
  });

  it("keeps engagement isolated when a new revision is opened", async () => {
    const client = createMemoryDesktopClient();
    await client.command("record_reading_activity", {
      request: { paperId: "p1", revisionId: "r1", pageNumber: 9, pageCount: 10 },
    });
    const next = await client.command<{
      engagement: { revisionId: string; furthestPage: number };
    }>("record_reading_activity", {
      request: { paperId: "p1", revisionId: "r2", pageNumber: 2, pageCount: 12 },
    });
    expect(next.engagement).toMatchObject({ revisionId: "r2", furthestPage: 2 });
    const r1 = await client.open<{ engagement: { revisionId: string; furthestPage: number } }>(
      "get_reading_context",
      { paperId: "p1", revisionId: "r1" },
    );
    expect(r1.engagement).toMatchObject({ revisionId: "r1", furthestPage: 9 });
  });
});
