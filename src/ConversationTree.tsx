import { useEffect, useRef } from "react";
import { Trash2 } from "lucide-react";
import { countTurnSubtree, layoutDiscussionTurns } from "./discussionTree";
import { useLocale } from "./i18n/LocaleContext";
import type { Message } from "./types";

function formatTime(value: string, dateLocale: string, fallback: string) {
  try {
    return new Intl.DateTimeFormat(dateLocale, {
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(value));
  } catch {
    return fallback;
  }
}

function preview(text: string, emptyLabel: string, length = 90) {
  const compact = text.replace(/\s+/g, " ").trim();
  if (compact.length <= length) return compact || emptyLabel;
  return `${compact.slice(0, length)}…`;
}

export default function ConversationTree({
  title,
  messages,
  activeMessageId,
  onClose,
  onSelect,
  onDelete,
}: {
  title: string;
  messages: Message[];
  activeMessageId: string | null;
  onClose: () => void;
  onSelect: (message: Message) => void;
  onDelete: (user: Message) => void;
}) {
  const { t, dateLocale } = useLocale();
  const emptyPreview = t("discussion.previewEmpty");
  const canvasRef = useRef<HTMLDivElement>(null);
  const turns = layoutDiscussionTurns(messages, activeMessageId);

  const fitToView = () => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const target =
      canvas.querySelector<HTMLElement>(".tree-node.head") ??
      canvas.querySelector<HTMLElement>(".tree-node.on-path") ??
      canvas.querySelector<HTMLElement>(".tree-node");
    target?.scrollIntoView({
      block: "center",
      inline: "nearest",
      behavior: "smooth",
    });
  };

  useEffect(() => {
    fitToView();
  }, [activeMessageId, messages.length]);

  return (
    <div className="scrim tree-scrim" onClick={onClose}>
      <section
        className="tree-workbench"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="tree-head">
          <div>
            <span className="eyebrow">{t("discussion.eyebrow")}</span>
            <h2>{title}</h2>
            <p>{t("discussion.subtitle", { count: turns.length })}</p>
          </div>
          <div className="tree-actions">
            <button
              className="outline-button"
              type="button"
              onClick={fitToView}
            >
              {t("discussion.fitCurrent")}
            </button>
            <button className="icon-button" type="button" onClick={onClose}>
              ×
            </button>
          </div>
        </div>
        <div className="tree-canvas" ref={canvasRef}>
          {turns.length === 0 ? (
            <div className="tree-empty">
              <span>✦</span>
              <strong>{t("discussion.emptyTitle")}</strong>
              <small>{t("discussion.emptyHint")}</small>
            </div>
          ) : (
            <div className="tree-nodes">
              {turns.map((turn) => {
                const streaming =
                  turn.user.status === "streaming" ||
                  turn.assistant?.status === "streaming";
                const subtree = countTurnSubtree(messages, turn.user.id);
                return (
                  <div
                    className={[
                      "tree-node",
                      turn.onActivePath ? "on-path" : "",
                      turn.isHead ? "head" : "",
                      turn.isBranchPoint ? "branch-point" : "",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                    key={turn.user.id}
                    style={{ ["--tree-depth" as string]: String(turn.depth) }}
                  >
                    <button
                      type="button"
                      className="tree-node-main"
                      onClick={() =>
                        onSelect(turn.assistant ?? turn.user)
                      }
                    >
                      <span className="node-index">
                        {String(turn.index + 1).padStart(2, "0")}
                      </span>
                      <div>
                        <strong>
                          {t("discussion.ask")} ·{" "}
                          {formatTime(
                            turn.user.createdAt,
                            dateLocale,
                            t("discussion.now"),
                          )}
                        </strong>
                        <p>{preview(turn.user.content, emptyPreview)}</p>
                        <small>
                          {turn.assistant
                            ? `${t("discussion.answer")} · ${preview(turn.assistant.content, emptyPreview, 70)}`
                            : streaming
                              ? t("discussion.answering")
                              : t("discussion.noAnswer")}
                          {turn.isHead
                            ? t("discussion.headSuffix")
                            : turn.onActivePath
                              ? t("discussion.pathSuffix")
                              : ""}
                          {turn.isBranchPoint ? t("discussion.branchSuffix") : ""}
                        </small>
                      </div>
                    </button>
                    <button
                      type="button"
                      className="tree-node-delete"
                      aria-label={t("discussion.deleteTurn")}
                      title={t("discussion.deleteTurn")}
                      disabled={streaming}
                      onClick={() => {
                        const ok = window.confirm(
                          subtree > 1
                            ? t("discussion.confirmDeleteSubtree", {
                                count: subtree,
                              })
                            : t("discussion.confirmDeleteTurn"),
                        );
                        if (ok) onDelete(turn.user);
                      }}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <div className="tree-legend">
          <span>
            <i className="legend-dot main" />
            {t("discussion.legendOther")}
          </span>
          <span>
            <i className="legend-dot branch" />
            {t("discussion.legendBranch")}
          </span>
          <span>
            <i className="legend-dot current" />
            {t("discussion.legendCurrent")}
          </span>
        </div>
      </section>
    </div>
  );
}
