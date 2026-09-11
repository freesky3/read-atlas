import MarkdownBody from "../MarkdownBody";
import type { GuideCastSnapshot, GuideNote, GuideReply } from "../types";
import { guidePersona } from "./personas";
import GuideAvatar from "./GuideAvatar";

export default function GuideCard({
  note,
  replies,
  active,
  onActivate,
  snapshot,
  workspaceRoot,
}: {
  note: GuideNote;
  replies: GuideReply[];
  active: boolean;
  onActivate: () => void;
  snapshot?: GuideCastSnapshot | null;
  workspaceRoot?: string | null;
}) {
  const persona = guidePersona(note.speakerId, snapshot, workspaceRoot);
  return (
    <article
      className={`guide-card ${active ? "is-active" : ""}`}
      data-guide-card={note.id}
      style={{ borderLeftColor: persona?.color }}
    >
      <button type="button" className="guide-card-head" onClick={onActivate}>
        <span className="guide-card-who">
          <GuideAvatar persona={persona} />
          <span style={{ color: persona?.color }}>
            {persona?.displayName ?? note.speakerId}
          </span>
        </span>
        <span>p.{note.anchor.pageNumber}</span>
      </button>
      <div className="guide-card-body">
        <MarkdownBody>{note.body}</MarkdownBody>
      </div>
      {replies.map((reply) => {
        const who = guidePersona(reply.speakerId, snapshot, workspaceRoot);
        return (
          <div key={reply.id} className="guide-reply" data-guide-reply={reply.id}>
            <span className="guide-card-who">
              <GuideAvatar persona={who} />
              <span style={{ color: who?.color }}>
                {who?.displayName ?? reply.speakerId}
              </span>
            </span>
            <MarkdownBody>{reply.body}</MarkdownBody>
          </div>
        );
      })}
    </article>
  );
}
