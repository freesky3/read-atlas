import React from "react";

export function LanguageGate({
  onPick,
  error,
  busy,
}: {
  onPick: (locale: "zh-CN" | "en") => Promise<void>;
  error: string | null;
  busy: boolean;
}) {
  return (
    <div
      className="scrim trash-modal-scrim open"
      role="dialog"
      aria-modal="true"
      aria-labelledby="language-gate-title"
    >
      <div
        className="trash-confirm-card"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="trash-confirm-header">
          <div>
            <h3 id="language-gate-title" className="trash-confirm-title">
              中文 / English
            </h3>
            <p className="trash-confirm-subtitle">
              界面与新生成 · Interface and new results
            </p>
          </div>
        </header>
        {error ? (
          <div className="trash-confirm-body">
            <p role="alert">{error}</p>
          </div>
        ) : null}
        <footer className="trash-confirm-footer">
          <button
            type="button"
            className="btn-liquid-pill"
            disabled={busy}
            onClick={() => void onPick("zh-CN")}
          >
            中文
          </button>
          <button
            type="button"
            className="btn-liquid-pill"
            disabled={busy}
            onClick={() => void onPick("en")}
          >
            English
          </button>
        </footer>
      </div>
    </div>
  );
}

export default LanguageGate;
