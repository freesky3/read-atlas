// Undo Token 明文只在 Start 时出现一次，库里只存 hash。
// 8 秒 Toast 是快捷入口；任务中心要在默认 10 分钟窗口内继续能撤销。

const DEFAULT_TTL_MS = 10 * 60 * 1000;

export type StoredUndoToken = Readonly<{
  token: string;
  expiresAt: number;
}>;

const tokens = new Map<string, StoredUndoToken>();
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function rememberUndoToken(
  batchId: string,
  token: string | null | undefined,
  expiresAt?: string | null,
): void {
  if (!batchId || !token) return;
  const parsed = expiresAt ? Date.parse(expiresAt) : Date.now() + DEFAULT_TTL_MS;
  const expires = Number.isFinite(parsed) ? parsed : Date.now() + DEFAULT_TTL_MS;
  tokens.set(batchId, { token, expiresAt: expires });
  emit();
}

export function peekUndoToken(batchId: string, now = Date.now()): string | null {
  const entry = tokens.get(batchId);
  if (!entry) return null;
  if (now >= entry.expiresAt) {
    tokens.delete(batchId);
    emit();
    return null;
  }
  return entry.token;
}

export function consumeUndoToken(batchId: string, now = Date.now()): string | null {
  const token = peekUndoToken(batchId, now);
  if (!token) return null;
  tokens.delete(batchId);
  emit();
  return token;
}

export function subscribeUndoTokens(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function clearUndoTokens(): void {
  if (tokens.size === 0) return;
  tokens.clear();
  emit();
}
