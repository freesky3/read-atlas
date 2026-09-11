import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTrailingThrottle } from "./throttle";

describe("createTrailingThrottle", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("runs immediately and then publishes only the latest value per interval", () => {
    const task = vi.fn();
    const throttle = createTrailingThrottle<string>(500, task);

    throttle.push("page-1");
    throttle.push("page-2");
    throttle.push("page-3");

    expect(task).toHaveBeenCalledTimes(1);
    expect(task).toHaveBeenLastCalledWith("page-1");

    vi.advanceTimersByTime(499);
    expect(task).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);

    expect(task).toHaveBeenCalledTimes(2);
    expect(task).toHaveBeenLastCalledWith("page-3");
  });

  it("can flush a pending state when its paper is closed", () => {
    const task = vi.fn();
    const throttle = createTrailingThrottle<string>(500, task);
    throttle.push("initial");
    throttle.push("latest");

    throttle.dispose(true);

    expect(task).toHaveBeenCalledTimes(2);
    expect(task).toHaveBeenLastCalledWith("latest");
  });
});
