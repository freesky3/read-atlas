export type TrailingThrottle<T> = {
  push(value: T): void;
  dispose(flush?: boolean): void;
};

export function createTrailingThrottle<T>(
  intervalMs: number,
  task: (value: T) => void,
): TrailingThrottle<T> {
  let lastRun = Number.NEGATIVE_INFINITY;
  let pending: T | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const run = () => {
    timer = undefined;
    if (pending === undefined) return;
    const value = pending;
    pending = undefined;
    lastRun = Date.now();
    task(value);
  };

  return {
    push(value) {
      pending = value;
      const remaining = intervalMs - (Date.now() - lastRun);
      if (remaining <= 0) {
        if (timer !== undefined) clearTimeout(timer);
        run();
      } else if (timer === undefined) {
        timer = setTimeout(run, remaining);
      }
    },
    dispose(flush = false) {
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      if (flush) run();
      else pending = undefined;
    },
  };
}
