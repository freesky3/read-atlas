import React, { useMemo } from "react";
import { useLocale } from "../i18n/LocaleContext";


export interface MessageItem {
  id: string;
  role: string;
  text?: string;
  content?: string;
  createdAt?: string;
}

export interface ChatTimelineNavigatorProps {
  messages: MessageItem[];
  activeMessageId?: string | null;
  onJumpToMessage: (messageId: string) => void;
}

export function ChatTimelineNavigator({
  messages,
  activeMessageId,
  onJumpToMessage,
}: ChatTimelineNavigatorProps) {
  const { t } = useLocale();
  const turns = useMemo(() => {
    const result: Array<{
      userMessageId: string;
      question: string;
      timeStr: string;
      answerSnippet: string;
      turnIndex: number;
    }> = [];

    let turnCount = 0;
    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role === "user") {
        turnCount++;
        const nextMsg = messages[i + 1];
        const answerText =
          nextMsg && nextMsg.role === "assistant"
            ? nextMsg.text || nextMsg.content || ""
            : "";

        // Format the timestamp
        let timeStr = "";
        if (msg.createdAt) {
          try {
            const d = new Date(msg.createdAt);
            timeStr = `${String(d.getHours()).padStart(2, "0")}:${String(
              d.getMinutes()
            ).padStart(2, "0")}`;
          } catch {
            timeStr = "";
          }
        }

        // Strip Markdown markers for a plain-text snippet
        const cleanSnippet = answerText
          .replace(/[#*`$\\]/g, "")
          .replace(/\[\^?\d+\]/g, "")
          .replace(/\s+/g, " ")
          .trim()
          .slice(0, 110);

        result.push({
          userMessageId: msg.id,
          question: (msg.text || msg.content || t("reader.timeline.userQuestion")).trim(),
          timeStr,
          answerSnippet: cleanSnippet ? `${cleanSnippet}...` : t("reader.timeline.waiting"),
          turnIndex: turnCount,
        });
      }
    }
    return result;
  }, [messages, t]);

  if (turns.length === 0) {
    return null;
  }

  return (
    <aside className="chat-timeline-minimap" aria-label={t("reader.timeline.nav")}>
      {turns.map((turn) => {
        const isActive = activeMessageId === turn.userMessageId;
        return (
          <button
            key={turn.userMessageId}
            type="button"
            className={`timeline-tick ${isActive ? "active" : ""}`}
            onClick={() => onJumpToMessage(turn.userMessageId)}
            aria-label={t("reader.timeline.jump", { index: turn.turnIndex, question: turn.question })}
            title={`#${turn.turnIndex} ${turn.question}`}
          >
            <div
              className="timeline-preview-card"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="preview-header">
                <span className="preview-index">{t("reader.timeline.questionIndex", { index: turn.turnIndex })}</span>
                {turn.timeStr && <span>{turn.timeStr}</span>}
              </div>
              <div className="preview-question">{turn.question}</div>
              <div className="preview-snippet">{turn.answerSnippet}</div>
            </div>
          </button>
        );
      })}
    </aside>
  );
}
