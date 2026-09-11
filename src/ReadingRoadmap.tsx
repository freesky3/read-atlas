import React, { useEffect, useRef, useState } from "react";
import type {
  ReadingRoadmapProjection,
  RoadmapProgressEntry,
  RoadmapPass,
  RoadmapTask,
  RoadmapEvidencePill,
} from "./types";
import MarkdownBody from "./MarkdownBody";
import RegenerateConfirmModal from "./components/RegenerateConfirmModal";
import { useLocale } from "./i18n/LocaleContext";

export interface ReadingRoadmapPanelProps {
  open: boolean;
  pinned?: boolean;
  projection: ReadingRoadmapProjection | null;
  progress: RoadmapProgressEntry[];
  activeJobId: string | null;
  isOrientationMissing?: boolean;
  onClose: () => void;
  onTogglePin?: (pinned: boolean) => void;
  onToggleTask: (taskId: string, completed: boolean) => void;
  onGenerate: () => void;
  onJumpToPage: (page: number, blockId?: string, label?: string) => void;
}

export const ReadingRoadmapPanel: React.FC<ReadingRoadmapPanelProps> = ({
  open,
  pinned = false,
  projection,
  progress,
  activeJobId,
  isOrientationMissing = false,
  onClose,
  onTogglePin,
  onToggleTask,
  onGenerate,
  onJumpToPage,
}) => {
  const { t } = useLocale();
  const [showRegenerateConfirm, setShowRegenerateConfirm] = useState(false);
  const leaveTimerRef = useRef<number | null>(null);

  const handleMouseEnter = () => {
    if (leaveTimerRef.current) {
      window.clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
  };

  const handleMouseLeave = () => {
    if (pinned) return;
    if (leaveTimerRef.current) {
      window.clearTimeout(leaveTimerRef.current);
    }
    leaveTimerRef.current = window.setTimeout(() => {
      onClose();
    }, 280);
  };

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current) {
        window.clearTimeout(leaveTimerRef.current);
      }
    };
  }, []);

  const content = projection?.content;
  const isGenerating = Boolean(activeJobId);
  const priorityObject = content?.oneChart;

  // Compute progress stats
  const completedMap = new Map<string, boolean>();
  for (const entry of progress) {
    completedMap.set(entry.taskId, entry.completed);
  }

  let totalTasks = 0;
  let completedTasks = 0;

  if (content?.passes) {
    for (const pass of content.passes) {
      for (const task of pass.tasks) {
        totalTasks += 1;
        if (completedMap.get(task.id)) {
          completedTasks += 1;
        }
      }
    }
  }

  const completionPercent =
    totalTasks > 0 ? Math.round((completedTasks / totalTasks) * 100) : 0;

  return (
    <aside
      className={`reading-roadmap-overlay ${open ? "open" : ""}`}
      aria-label={t("roadmap.panelAria")}
      aria-hidden={!open}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="roadmap-panel">
        {/* Header */}
        <header className="roadmap-panel-header">
          <div className="roadmap-header-left">
            <div className="roadmap-title-row">
              <span className="roadmap-title-icon">📋</span>
              <h2 className="roadmap-title">{t("roadmap.title")}</h2>
            </div>
            {content && totalTasks > 0 && (
              <div className="roadmap-progress-summary">
                <span className="roadmap-progress-text">
                  {t("roadmap.progress", {
                    completed: completedTasks,
                    total: totalTasks,
                    percent: completionPercent,
                  })}
                </span>
                <div className="roadmap-progress-bar-bg">
                  <div
                    className="roadmap-progress-bar-fill"
                    style={{ width: `${completionPercent}%` }}
                  />
                </div>
              </div>
            )}
          </div>
          <div className="roadmap-header-actions">
            {content && !isGenerating && (
              <button
                type="button"
                className="roadmap-regenerate-btn"
                onClick={() => setShowRegenerateConfirm(true)}
                title={t("roadmap.regenerate")}
                aria-label={t("roadmap.regenerate")}
              >
                <span>🔄</span>
                <small>{t("roadmap.regenerateShort")}</small>
              </button>
            )}
            <button
              type="button"
              className={`roadmap-pin-btn ${pinned ? "is-pinned" : ""}`}
              onClick={() => onTogglePin?.(!pinned)}
              title={
                pinned ? t("roadmap.pinnedTitle") : t("roadmap.unpinnedTitle")
              }
              aria-label={pinned ? t("roadmap.unpin") : t("roadmap.pin")}
            >
              <span>{pinned ? "📌" : "📍"}</span>
              <small>{pinned ? t("roadmap.pinned") : t("roadmap.pinShort")}</small>
            </button>
            <button
              type="button"
              className="roadmap-close-btn"
              onClick={onClose}
              aria-label={t("roadmap.closeAria")}
              title={t("roadmap.close")}
            >
              ×
            </button>
          </div>
        </header>

        {/* Body Content */}
        <div className="roadmap-panel-body">
          {/* State 1: Generating */}
          {isGenerating && (
            <div className="roadmap-state-card is-busy">
              <div className="roadmap-spinner" />
              <h3>{t("roadmap.generatingTitle")}</h3>
              <p>{t("roadmap.generatingBody")}</p>
            </div>
          )}

          {/* State 2: No Roadmap yet */}
          {!isGenerating && !content && (
            <div className="roadmap-state-card empty">
              <div className="roadmap-empty-icon">🎯</div>
              <h3>{t("roadmap.emptyTitle")}</h3>
              <p>
                {t("roadmap.emptyIntroBefore")}
                <strong>{t("roadmap.threePass")}</strong>
                {t("roadmap.emptyIntroAfter")}
              </p>
              {isOrientationMissing && (
                <div className="roadmap-warning-badge">
                  {t("roadmap.briefHint", { brief: "Brief" })}
                </div>
              )}
              <button
                type="button"
                className="btn-liquid-pill primary roadmap-generate-btn"
                onClick={onGenerate}
              >
                {t("roadmap.generate")}
              </button>
            </div>
          )}

          {/* State 3: Render Passes */}
          {!isGenerating && content && (
            <>
              <div className="roadmap-paper-badge">
                <span className="badge-tag">{t("roadmap.target")}</span>
                <span className="badge-title">{content.paperTitle}</span>
              </div>

              {content.prerequisites && content.prerequisites.length > 0 && (
                <div className="roadmap-teaching-block">
                  <span className="roadmap-teaching-label">{t("roadmap.prerequisites")}</span>
                  <div className="roadmap-chip-list">
                    {content.prerequisites.map((item) => (
                      <span key={item} className="roadmap-chip">
                        <MarkdownBody>{item}</MarkdownBody>
                      </span>
                    ))}
                  </div>
                </div>
              )}

              {content.learningObjectives && content.learningObjectives.length > 0 && (
                <div className="roadmap-teaching-block">
                  <span className="roadmap-teaching-label">{t("roadmap.objectives")}</span>
                  <ul className="roadmap-objective-list">
                    {content.learningObjectives.map((item) => (
                      <li key={item}>
                        <MarkdownBody>{item}</MarkdownBody>
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              <div className="roadmap-passes-container">
                {content.passes.map((pass: RoadmapPass) => {
                  let passTotal = pass.tasks.length;
                  let passCompleted = 0;
                  for (const passTask of pass.tasks) {
                    if (completedMap.get(passTask.id)) passCompleted += 1;
                  }

                  return (
                    <details
                      key={pass.passNumber}
                      className="roadmap-pass-section"
                      open
                    >
                      <summary className="roadmap-pass-summary">
                        <div className="pass-summary-header">
                          <span className="pass-badge">{t("roadmap.pass", { number: pass.passNumber })}</span>
                          <span className="pass-title">{pass.title}</span>
                          <span className="pass-time-pill">{pass.timeBudget}</span>
                        </div>
                        <div className="pass-stats">
                          {passCompleted}/{passTotal}
                        </div>
                      </summary>

                      <div className="pass-content">
                        <div className="pass-subtitle">{pass.subtitle}</div>

                        {pass.exitCriteria && (
                          <div className="pass-exit-criteria">
                            <span className="exit-icon">🎯</span>
                            <strong>{t("roadmap.nextStep")}</strong>
                            <span className="pass-exit-criteria-text">
                              <MarkdownBody>{pass.exitCriteria}</MarkdownBody>
                            </span>
                          </div>
                        )}

                        <div className="pass-tasks-list">
                          {pass.tasks.map((task: RoadmapTask) => {
                            const isDone = Boolean(completedMap.get(task.id));

                            return (
                              <div
                                key={task.id}
                                className={`roadmap-task-card ${isDone ? "is-done" : ""}`}
                              >
                                <div className="task-main-row">
                                  <label className="task-checkbox-label">
                                    <input
                                      type="checkbox"
                                      className="roadmap-task-checkbox"
                                      checked={isDone}
                                      onChange={(e) =>
                                        onToggleTask(task.id, e.target.checked)
                                      }
                                    />
                                    <span className="task-checkbox-custom" />
                                  </label>

                                  <div className="task-text-content">
                                    <div className="task-title-line">
                                      <div className="task-text-markdown">
                                        <MarkdownBody>{task.text}</MarkdownBody>
                                      </div>
                                      <div className="task-meta-tags">
                                        <span className="task-time-tag">
                                          ⏱️ {task.timeMinutes}{t("completion.min")}</span>
                                        {task.required ? (
                                          <span className="task-required-tag required">
                                            {t("roadmap.required")}
                                          </span>
                                        ) : (
                                          <span className="task-required-tag optional">
                                            {t("roadmap.optional")}
                                          </span>
                                        )}
                                      </div>
                                    </div>

                                    {/* Completion criteria */}
                                    {task.completionCriteria && (
                                      <div className="task-criteria-box">
                                        <span className="criteria-label">{t("roadmap.criteria")}</span>
                                        <span className="criteria-text">
                                          <MarkdownBody>{task.completionCriteria}</MarkdownBody>
                                        </span>
                                      </div>
                                    )}

                                    {/* Self-check questions */}
                                    {task.selfCheckQuestions &&
                                      task.selfCheckQuestions.length > 0 && (
                                        <div className="task-self-check-box">
                                          <span className="self-check-label">
                                            {t("roadmap.selfCheck")}
                                          </span>
                                          <ul className="self-check-list">
                                            {task.selfCheckQuestions.map(
                                              (q: string, qIdx: number) => (
                                                <li key={qIdx}>
                                                  <MarkdownBody>{q}</MarkdownBody>
                                                </li>
                                              ),
                                            )}
                                          </ul>
                                        </div>
                                      )}

                                    {/* Evidence pills */}
                                    {task.evidence && task.evidence.length > 0 && (
                                      <div className="task-evidence-row">
                                        {task.evidence.map(
                                          (ev: RoadmapEvidencePill, evIdx: number) => (
                                            <button
                                              key={evIdx}
                                              type="button"
                                              className="evidence-pill-roadmap"
                                              onClick={() =>
                                                onJumpToPage(ev.page, ev.blockId, ev.label)
                                              }
                                              title={t("roadmap.jumpToPage", { page: ev.page })}
                                            >
                                              📄 {ev.label}
                                            </button>
                                          ),
                                        )}
                                      </div>
                                    )}
                                  </div>
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    </details>
                  );
                })}
              </div>

              {/* Bottom Cards: Elevator pitch & One chart */}
              <div className="roadmap-bottom-cards">
                {content.elevatorPitch && (
                  <div className="roadmap-card-box pitch">
                    <div className="card-box-header">
                      <span>🎤</span>
                      <h4>{t("roadmap.elevatorPitch")}</h4>
                    </div>
                    <div className="card-box-body">
                      <MarkdownBody>{content.elevatorPitch}</MarkdownBody>
                    </div>
                  </div>
                )}

                {priorityObject && (
                  <div className="roadmap-card-box one-chart">
                    <div className="card-box-header">
                      <span>📊</span>
                      <h4>{t("roadmap.priorityObject")}</h4>
                    </div>
                    <div className="card-box-body">
                      <div className="one-chart-target-row">
                        <button
                          type="button"
                          className="evidence-pill-roadmap primary"
                          onClick={() =>
                            onJumpToPage(
                              priorityObject.page,
                              priorityObject.blockId,
                              priorityObject.label,
                            )
                          }
                          title={t("roadmap.jumpToPageLabel", {
                            page: priorityObject.page,
                            label: priorityObject.label,
                          })}
                        >
                          📄 {priorityObject.label} (p.{priorityObject.page})
                        </button>
                      </div>
                      <div className="one-chart-reason">
                        <MarkdownBody>{priorityObject.reason}</MarkdownBody>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
        {content && (
          <RegenerateConfirmModal
            open={showRegenerateConfirm}
            artifactTitle={content.paperTitle || t("roadmap.currentDoc")}
            kindLabel={t("roadmap.kindLabel")}
            busy={isGenerating}
            onConfirm={() => {
              setShowRegenerateConfirm(false);
              onGenerate();
            }}
            onCancel={() => setShowRegenerateConfirm(false)}
          />
        )}
      </div>
    </aside>
  );
};
