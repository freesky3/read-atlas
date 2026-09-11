import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  NoticeProvider,
  sanitizeNoticeMessage,
  useNotice,
} from "./NoticeCenter";

function NoticeHarness() {
  const { notifyError, notifyInfo } = useNotice();
  return (
    <>
      <button
        type="button"
        onClick={() =>
          notifyError(
            "Provider failed",
            "Bearer secret-token-123 api_key=plain-secret https://example.test?a=1&token=query-secret",
            { source: "provider", dedupeKey: "provider-error" },
          )
        }
      >
        error
      </button>
      <button
        type="button"
        onClick={() =>
          notifyInfo("First", "first value", {
            dedupeKey: "same-job",
            source: "job",
          })
        }
      >
        first
      </button>
      <button
        type="button"
        onClick={() =>
          notifyInfo("Updated", "updated value", {
            dedupeKey: "same-job",
            source: "job",
          })
        }
      >
        update
      </button>
      <button
        type="button"
        onClick={() =>
          notifyInfo("Action", "Open tasks", {
            source: "job",
            action: { kind: "open_operations", label: "查看任务" },
          })
        }
      >
        action
      </button>
    </>
  );
}

afterEach(() => {
  vi.useRealTimers();
});

describe("NoticeCenter", () => {
  it("redacts credentials and sensitive URL parameters", () => {
    expect(
      sanitizeNoticeMessage(
        "Bearer token-value-123 apiKey=secret-value https://x.test/?token=query-value&safe=1",
      ),
    ).toBe(
      "Bearer *** apiKey=*** https://x.test/?token=***&safe=1",
    );
  });

  it("keeps errors visible and merges duplicate notices", async () => {
    vi.useFakeTimers();
    render(
      <NoticeProvider>
        <NoticeHarness />
      </NoticeProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "error" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bearer ***");
    expect(screen.getByRole("alert")).not.toHaveTextContent("plain-secret");

    fireEvent.click(screen.getByRole("button", { name: "first" }));
    fireEvent.click(screen.getByRole("button", { name: "update" }));
    expect(screen.queryByText("first value")).not.toBeInTheDocument();
    expect(screen.getByText("updated value")).toBeInTheDocument();
    expect(screen.getAllByRole("status")).toHaveLength(1);

    act(() => vi.advanceTimersByTime(10_000));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.queryByText("updated value")).not.toBeInTheDocument();
  });

  it("dispatches only a controlled notice action payload", async () => {
    const user = userEvent.setup();
    const listener = vi.fn();
    window.addEventListener("read-desktop:notice-action", listener);
    render(
      <NoticeProvider>
        <NoticeHarness />
      </NoticeProvider>,
    );

    await user.click(screen.getByRole("button", { name: "action" }));
    await user.click(screen.getByRole("button", { name: "查看任务" }));
    expect(listener).toHaveBeenCalledTimes(1);
    expect((listener.mock.calls[0][0] as CustomEvent).detail).toEqual({
      kind: "open_operations",
      label: "查看任务",
    });
    window.removeEventListener("read-desktop:notice-action", listener);
  });
});
