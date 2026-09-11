import React, { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { t as translate } from "./i18n/t";
import type { UiLocale } from "./i18n/types";
export type NoticeSeverity = "info" | "success" | "warning" | "error";
export type NoticeSource = "workspace" | "provider" | "import" | "reader" | "chat" | "artifact" | "job" | "export" | "system";
export type NoticeActionKind =
  | "open_settings"
  | "open_operations"
  | "open_source"
  | "copy_details"
  | "dismiss";

export type AppNotice = {
  id: string;
  severity: NoticeSeverity;
  title: string;
  message: string;
  source: NoticeSource;
  entityId?: string;
  createdAt: string;
  persistent: boolean;
  dedupeKey?: string;
  action?: {
    kind: NoticeActionKind;
    label: string;
    payload?: Record<string, string>;
  };
};

type NotifyOpts = Partial<Pick<AppNotice, "source" | "entityId" | "dedupeKey" | "action">> & { persistent?: boolean };

type NoticeContextValue = {
  notices: AppNotice[];
  notify: (severity: NoticeSeverity, title: string, message: string, opts?: NotifyOpts) => string;
  notifyInfo: (title: string, message: string, opts?: NotifyOpts) => string;
  notifySuccess: (title: string, message: string, opts?: NotifyOpts) => string;
  notifyWarning: (title: string, message: string, opts?: NotifyOpts) => string;
  notifyError: (title: string, message: string, opts?: NotifyOpts) => string;
  dismiss: (id: string) => void;
  clearAll: () => void;
};

const NoticeContext = createContext<NoticeContextValue | null>(null);

export function sanitizeNoticeMessage(msg: string): string {
  return msg
    .replace(/\bBearer\s+[A-Za-z0-9._~+/=-]{8,}/gi, "Bearer ***")
    .replace(/sk-[A-Za-z0-9_-]{10,}/g, "sk-***")
    .replace(
      /([?&](?:api[_-]?key|key|token|access[_-]?token|authorization|signature|sig)=)[^&#\s]+/gi,
      "$1***",
    )
    .replace(
      /(["']?(?:api[_-]?key|token|access[_-]?token|authorization|secret)["']?\s*[:=]\s*["']?)[^"',}\s&]+/gi,
      "$1***",
    );
}

export function NoticeProvider({ children }: { children: React.ReactNode }) {
  const [notices, setNotices] = useState<AppNotice[]>([]);
  const noticesRef = useRef<AppNotice[]>([]);
  const timers = useRef<Map<string, number>>(new Map());

  const dismiss = useCallback((id: string) => {
    const next = noticesRef.current.filter((notice) => notice.id !== id);
    noticesRef.current = next;
    setNotices(next);
    const t = timers.current.get(id);
    if (t) {
      window.clearTimeout(t);
      timers.current.delete(id);
    }
  }, []);

  const scheduleAutoDismiss = useCallback((notice: AppNotice) => {
    if (notice.persistent) return;
    let ms = 4000;
    if (notice.severity === "warning") ms = 8000;
    if (notice.severity === "error") return; // error stays
    const previous = timers.current.get(notice.id);
    if (previous) window.clearTimeout(previous);
    const tid = window.setTimeout(() => dismiss(notice.id), ms);
    timers.current.set(notice.id, tid);
  }, [dismiss]);

  const notify = useCallback((severity: NoticeSeverity, title: string, message: string, opts: NotifyOpts = {}): string => {
    const sanitized = sanitizeNoticeMessage(message);
    const dedupeKey = opts.dedupeKey;
    const existing = dedupeKey
      ? noticesRef.current.find(
          (notice) =>
            notice.dedupeKey === dedupeKey && notice.severity === severity,
        )
      : undefined;
    const id =
      existing?.id ??
      `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const persistent = opts.persistent ?? (severity === "error");
    const notice: AppNotice = {
      id,
      severity,
      title,
      message: sanitized,
      source: opts.source ?? "system",
      entityId: opts.entityId,
      createdAt: new Date().toISOString(),
      persistent,
      dedupeKey,
      action: opts.action,
    };
    const next = existing
      ? noticesRef.current.map((current) =>
          current.id === existing.id ? notice : current,
        )
      : [...noticesRef.current.slice(-4), notice];
    noticesRef.current = next;
    setNotices(next);
    scheduleAutoDismiss(notice);
    return id;
  }, [scheduleAutoDismiss]);

  const notifyInfo = useCallback((t: string, m: string, o?: NotifyOpts) => notify("info", t, m, o), [notify]);
  const notifySuccess = useCallback((t: string, m: string, o?: NotifyOpts) => notify("success", t, m, o), [notify]);
  const notifyWarning = useCallback((t: string, m: string, o?: NotifyOpts) => notify("warning", t, m, o), [notify]);
  const notifyError = useCallback((t: string, m: string, o?: NotifyOpts) => notify("error", t, m, o), [notify]);

  const clearAll = useCallback(() => {
    timers.current.forEach((tid) => window.clearTimeout(tid));
    timers.current.clear();
    noticesRef.current = [];
    setNotices([]);
  }, []);

  useEffect(
    () => () => {
      timers.current.forEach((timer) => window.clearTimeout(timer));
      timers.current.clear();
    },
    [],
  );

  return (
    <NoticeContext.Provider value={{ notices, notify, notifyInfo, notifySuccess, notifyWarning, notifyError, dismiss, clearAll }}>
      {children}
      <NoticeStack notices={notices} onDismiss={dismiss} />
    </NoticeContext.Provider>
  );
}

export function useNotice() {
  const ctx = useContext(NoticeContext);
  if (!ctx) throw new Error("useNotice must be used within NoticeProvider");
  return ctx;
}

function NoticeStack({ notices, onDismiss }: { notices: AppNotice[]; onDismiss: (id: string) => void }) {
  const [locale, setLocale] = useState<UiLocale>(() => document.documentElement.lang === "en" ? "en" : "zh-CN");
  useEffect(() => {
    const observer = new MutationObserver(() => setLocale(document.documentElement.lang === "en" ? "en" : "zh-CN"));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
    return () => observer.disconnect();
  }, []);
  if (notices.length === 0) return null;
  return (
    <div
      className="notice-stack"
      role="region"
      aria-label={translate(locale, "notice.region")}
      aria-live="polite"
      style={{
        position: "fixed",
        top: 16,
        right: 16,
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: 420,
        pointerEvents: "none",
      }}
    >
      {notices.map((n) => (
        <div
          key={n.id}
          role={n.severity === "error" ? "alert" : "status"}
          aria-live={n.severity === "error" ? "assertive" : "polite"}
          className={`notice-card severity-${n.severity}`}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              onDismiss(n.id);
            }
          }}
          style={{
            pointerEvents: "auto",
            background: "var(--glass-card)",
            border: "1px solid var(--glass-border)",
            borderLeft: `4px solid ${n.severity === "error" ? "#dc2626" : n.severity === "warning" ? "#d97706" : n.severity === "success" ? "#16a34a" : "#2563eb"}`,
            borderRadius: 12,
            padding: "10px 12px",
            boxShadow: "var(--glass-specular)",
            backdropFilter: "var(--glass-blur)",
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          <div style={{ display: "flex", justifyContent: "space-between", gap: 8, alignItems: "flex-start" }}>
            <strong style={{ fontSize: 13 }}>{n.title}</strong>
            <button
              type="button"
              aria-label={translate(locale, "notice.close")}
              onClick={() => onDismiss(n.id)}
              style={{ border: 0, background: "transparent", cursor: "pointer", color: "var(--muted)", fontSize: 16, lineHeight: 1 }}
            >
              ×
            </button>
          </div>
          <span style={{ fontSize: 12, color: "var(--ink)", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{n.message}</span>
          {n.action && (
            <button
              type="button"
              className="btn-sm btn-primary"
              style={{ alignSelf: "flex-start", marginTop: 4 }}
              onClick={() => {
                window.dispatchEvent(
                  new CustomEvent("read-desktop:notice-action", {
                    detail: n.action,
                  }),
                );
                onDismiss(n.id);
              }}
            >
              {n.action.label}
            </button>
          )}
          <small style={{ fontSize: 10, color: "var(--muted)" }}>{new Date(n.createdAt).toLocaleTimeString(locale === "en" ? "en-US" : "zh-CN")} · {n.source}</small>
        </div>
      ))}
    </div>
  );
}
