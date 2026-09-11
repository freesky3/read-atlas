import { uiText } from "./i18n/uiText";
import { languageCode, metadataAbstractIndex } from "./i18n/language";
import { createContext, useContext, useState } from "react";
import type { ArtifactProjection } from "./types";
import Markdown from "./MarkdownBody";
import { useLocale, type TranslateFn } from "./i18n/LocaleContext";
type Row = Record<string, any>;
const IdentityContext = createContext<Record<string, { id: string }>>({});
const FIELD_KEYS: Record<string, true> = {
  document: true,
  container: true,
  relatedVersions: true,
  sources: true,
  issues: true,
  type: true,
  coverage: true,
  title: true,
  alternateTitles: true,
  versionInfo: true,
  contributors: true,
  dates: true,
  identifiers: true,
  publishingEntities: true,
  abstracts: true,
  placement: true,
  name: true,
  role: true,
  roleLabel: true,
  order: true,
  kind: true,
  label: true,
  event: true,
  eventLabel: true,
  dateText: true,
  value: true,
  identifierText: true,
  places: true,
  scope: true,
  languages: true,
  completeness: true,
  segments: true,
  heading: true,
  text: true,
  part: true,
  volume: true,
  issue: true,
  chapterNumber: true,
  sectionNumber: true,
  pages: true,
  articleNumber: true,
  relativeTo: true,
  relations: true,
  relationText: true,
  target: true,
};
const WORD_KEYS: Record<string, true> = {
  article: true,
  book: true,
  chapter: true,
  section: true,
  report: true,
  thesis: true,
  other: true,
  unknown: true,
  complete: true,
  partial: true,
  author: true,
  editor: true,
  translator: true,
  publisher: true,
  imprint: true,
  distributor: true,
  issuing_body: true,
  publication: true,
  online_publication: true,
  print_publication: true,
  release: true,
  submission: true,
  receipt: true,
  acceptance: true,
  revision: true,
  copyright: true,
  printing: true,
  manuscript_label: true,
  edition: true,
  journal: true,
  proceedings: true,
  series: true,
  document: true,
  container: true,
  earlier_version: true,
  later_version: true,
  translation: true,
  translation_source: true,
};
function metaField(name: string, t: TranslateFn): string {
  return FIELD_KEYS[name] ? t(`artifacts.metadata.fields.${name}`) : name;
}
function metaWord(name: string, t: TranslateFn): string {
  return WORD_KEYS[name] ? t(`artifacts.metadata.words.${name}`) : name;
}
const enums: Record<string, string[]> = {
  coverage: ["complete", "partial", "unknown"],
  completeness: ["complete", "partial", "unknown"],
  relativeTo: ["document", "container"],
  event: [
    "publication",
    "online_publication",
    "print_publication",
    "release",
    "submission",
    "receipt",
    "acceptance",
    "revision",
    "copyright",
    "printing",
    "other",
    "unknown",
  ],
};
const list = (value: any): any[] => (Array.isArray(value) ? value : []);
const common = () => ({
  title: null,
  alternateTitles: [],
  contributors: [],
  versionInfo: [],
  dates: [],
  identifiers: [],
  publishingEntities: [],
});
const container = () => ({
  ...common(),
  type: "unknown",
  placement: {
    part: null,
    volume: null,
    issue: null,
    chapterNumber: null,
    sectionNumber: null,
    pages: null,
    articleNumber: null,
  },
});
function templates(t: TranslateFn): Record<string, () => any> {
  const pending = t("artifacts.metadata.templates.pending");
  return {
    contributors: () => ({
      name: t("artifacts.metadata.templates.newContributor"),
      role: "unknown",
      roleLabel: null,
      order: null,
    }),
    versionInfo: () => ({
      kind: "other",
      label: t("artifacts.metadata.templates.newVersion"),
    }),
    dates: () => ({
      event: "unknown",
      eventLabel: null,
      dateText: pending,
      value: null,
    }),
    identifiers: () => ({
      type: "unknown",
      label: null,
      identifierText: pending,
      value: null,
    }),
    publishingEntities: () => ({
      name: pending,
      role: "unknown",
      roleLabel: null,
      places: [],
      scope: null,
    }),
    abstracts: () => ({
      label: null,
      languages: [],
      completeness: "unknown",
      segments: [],
    }),
    segments: () => ({ heading: null, text: pending }),
    relatedVersions: () => ({
      relativeTo: "document",
      relations: [{ type: "unknown", relationText: null }],
      target: { ...common(), container: null },
    }),
    relations: () => ({ type: "unknown", relationText: null }),
  };
}
export function AuxiliarySources({
  sources,
  onJump,
}: {
  sources: unknown;
  onJump?: (page: number) => void;
}) {
  const { t } = useLocale();
  const rows = list(sources);
  if (!rows.length) return null;
  return (
    <details className="auxiliary-sources">
      <summary>{t("artifacts.metadata.viewSources", { count: rows.length })}</summary>
      {rows.map((s, i) => (
        <article key={i}>
          {Number.isInteger(s.pageNumber) && s.pageNumber > 0 ? (
            <button
              type="button"
              onClick={() => onJump?.(s.pageNumber)}
              disabled={!onJump}
            >
              {t("artifacts.metadata.pdfPage", { page: s.pageNumber })}
            </button>
          ) : (
            <span>{t("artifacts.metadata.pageUnknown")}</span>
          )}
          {s.locator && <p>{s.locator}</p>}
          {s.excerpt && (
            <blockquote>
              <Markdown>{s.excerpt}</Markdown>
            </blockquote>
          )}
          {typeof s.supports === "string" ? (
            <Markdown>{s.supports}</Markdown>
          ) : (
            list(s.supports).map((support, j) => (
              <p key={j}>{support.explanation}</p>
            ))
          )}
        </article>
      ))}
    </details>
  );
}
function Field({
  value,
  name,
  path,
  onChange,
  locked,
  onUnlock,
}: {
  value: any;
  name: string;
  path: string;
  onChange?: (path: string, value: any) => void;
  locked: Set<string>;
  onUnlock: (path: string) => void;
}) {
  const { t } = useLocale();
  const identities = useContext(IdentityContext);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const label = metaField(name, t);
  const options =
    enums[name] ??
    (name === "role"
      ? path.includes("/contributors/")
        ? ["author", "editor", "translator", "other", "unknown"]
        : [
            "publisher",
            "imprint",
            "distributor",
            "issuing_body",
            "other",
            "unknown",
          ]
      : name === "kind"
        ? ["manuscript_label", "revision", "edition", "printing", "other"]
        : name === "type"
          ? path.includes("/identifiers/")
            ? ["doi", "isbn", "issn", "arxiv", "other", "unknown"]
            : path.includes("/relations/")
              ? [
                  "earlier_version",
                  "later_version",
                  "translation",
                  "translation_source",
                  "other",
                  "unknown",
                ]
              : path === "/document/type"
                ? [
                    "article",
                    "book",
                    "chapter",
                    "section",
                    "report",
                    "thesis",
                    "other",
                    "unknown",
                  ]
                : [
                    "book",
                    "journal",
                    "proceedings",
                    "series",
                    "other",
                    "unknown",
                  ]
          : undefined);
  if (Array.isArray(value))
    return (
      <details className="metadata-group">
        <summary>
          {t("artifacts.metadata.groupCount", { label, count: value.length })}
          {locked.has(path) && t("artifacts.metadata.manualKept")}
        </summary>
        {value.map((item, i) => (
          <div
            className="metadata-array-item"
            key={identities[`${path}/${i}`]?.id ?? i}
          >
            <Field
              value={item}
              name={typeof item === "object" ? `${label} ${i + 1}` : label}
              path={`${path}/${i}`}
              onChange={onChange}
              locked={locked}
              onUnlock={onUnlock}
            />
            {onChange && (
              <button
                type="button"
                onClick={() =>
                  onChange(
                    path,
                    value.filter((_, j) => j !== i),
                  )
                }
              >
                {t("artifacts.metadata.removeItem")}
              </button>
            )}
          </div>
        ))}
        {onChange && (
          <button
            type="button"
            onClick={() =>
              onChange(
                path,
                [
                  ...value,
                  templates(t)[name]?.() ??
                    t("artifacts.metadata.templates.pending"),
                ],
              )
            }
          >
            {t("artifacts.metadata.addLabeled", { label })}
          </button>
        )}
        {locked.has(path) && (
          <button type="button" onClick={() => onUnlock(path)}>
            {t("artifacts.metadata.restoreOriginal")}
          </button>
        )}
      </details>
    );
  if (value && typeof value === "object")
    return (
      <details className="metadata-group" open={name === "document"}>
        <summary>{label}</summary>
        {Object.entries(value).map(([k, v]) => (
          <Field
            key={k}
            value={v}
            name={k}
            path={`${path}/${k}`}
            onChange={onChange}
            locked={locked}
            onUnlock={onUnlock}
          />
        ))}
      </details>
    );
  if (value === null && name === "container")
    return (
      <div className="metadata-field">
        <span>{t("artifacts.metadata.notProvidedLabeled", { label })}</span>
        {onChange && (
          <button type="button" onClick={() => onChange(path, container())}>
            {t("artifacts.metadata.addContainer")}
          </button>
        )}
      </div>
    );
  return (
    <div className="metadata-field">
      <strong>{label}</strong>
      {editing ? (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onChange?.(
              path,
              ["order"].includes(name)
                ? draft.trim()
                  ? Number(draft)
                  : null
                : draft.trim()
                  ? draft
                  : null,
            );
            setEditing(false);
          }}
        >
          {options ? (
            <select
              aria-label={t("artifacts.metadata.editLabeled", { label })}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            >
              {options.map((v) => (
                <option key={v} value={v}>
                  {metaWord(v, t)}
                </option>
              ))}
            </select>
          ) : (
            <textarea
              aria-label={t("artifacts.metadata.editLabeled", { label })}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              rows={name === "text" ? 6 : 2}
            />
          )}
          <button type="submit">{t("artifacts.metadata.saveKeep")}</button>
          <button type="button" onClick={() => setEditing(false)}>
            {t("artifacts.metadata.cancel")}
          </button>
        </form>
      ) : (
        <>
          <div className="metadata-field-value">
            {value == null ? (
              <span>{t("artifacts.metadata.notProvided")}</span>
            ) : (
              <Markdown>
                {options
                  ? metaWord(String(value), t)
                  : String(value)}
              </Markdown>
            )}
          </div>
          {onChange && (
            <button
              type="button"
              aria-label={t("artifacts.metadata.editLabeled", { label })}
              onClick={() => {
                setDraft(value == null ? "" : String(value));
                setEditing(true);
              }}
            >
              {t("artifacts.metadata.edit")}
            </button>
          )}
          {[...locked].some((p) => p === path || path.startsWith(`${p}/`)) && (
            <>
              <small>{t("artifacts.metadata.manualNote")}</small>
              <button type="button" onClick={() => onUnlock(path)}>
                {t("artifacts.metadata.restoreOriginal")}
              </button>
            </>
          )}
        </>
      )}
    </div>
  );
}
export function AuxiliaryMetadata({
  artifact,
  onUpdate,
  onJump,
}: {
  artifact: ArtifactProjection;
  onUpdate?: (
    revisionId: string,
    metadata: Row,
    pins: string[],
  ) => Promise<void>;
  onJump?: (page: number) => void;
}) {
  const { t } = useLocale();
  const { locale } = useLocale();
  const prefZh = "zh-CN";
  const prefEn = "en";
  const content = artifact.content as Row;
  const view = { ...(content._display ?? {}), abstractIndex: metadataAbstractIndex(content, locale) } as Row;
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const pins = new Set<string>(list(content._pinned));
  const save = async (next: Row, nextPins = [...pins]) => {
    if (!onUpdate) return;
    setSaving(true);
    setError("");
    try {
      await onUpdate(
        artifact.revisionId,
        { ...next, _editBaseArtifactId: artifact.id },
        nextPins,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };
  const change = (path: string, value: any) => {
    const next = structuredClone(content);
    const keys = path.slice(1).split("/");
    let parent = next;
    for (const key of keys.slice(0, -1)) parent = parent[key];
    parent[keys.at(-1)!] = value;
    void save(next, [
      ...new Set(
        [...pins].filter((p) => !p.startsWith(`${path}/`)).concat(path),
      ),
    ]);
  };
  const unlock = (path: string) => {
    void save(
      content,
      [...pins].filter(
        (p) =>
          p !== path && !p.startsWith(`${path}/`) && !path.startsWith(`${p}/`),
      ),
    );
  };
  const abstracts = list(content.document?.abstracts);
  const selected =
    typeof view.abstractIndex === "number"
      ? abstracts[view.abstractIndex]
      : null;
  const dateChoices = ["document", "container"].flatMap((owner) =>
    list(content[owner]?.dates).map((date, i) => ({
      owner,
      date,
      id: `${owner}-${i}`,
    })),
  );
  const dateChoice = content._yearSelection?.date
    ? dateChoices.find(
        (d) =>
          d.owner === content._yearSelection.owner &&
          JSON.stringify(d.date) ===
            JSON.stringify(content._yearSelection.date),
      )?.id
    : "";
  return (
    <IdentityContext.Provider value={content._identities ?? {}}>
      <div className="artifact-metadata-container" aria-busy={saving}>
        <div className="artifact-meta-header-card">
          <h2>{view.title || t("artifacts.metadata.titleUnknown")}</h2>
          <small>{uiText(t, view.titleSource ?? "")}</small>
          <p>{list(view.authors).join(" · ")}</p>
          <p>
            {view.year ?? t("artifacts.metadata.yearUnknown")} {uiText(t, view.yearLabel ?? "")}
          </p>
          {view.venue && (
            <p>
              {t("artifacts.metadata.containerPrefix", { venue: view.venue })}
            </p>
          )}
        </div>
        {content._compatProtocol === "v1" && (
          <p>
            {t("artifacts.metadata.legacyCompat")}
          </p>
        )}
        {error && <p role="alert">{error}</p>}
        {onUpdate && (
          <fieldset disabled={saving}>
            <legend>{t("artifacts.metadata.displayChoices")}</legend>
            <label>
              {t("artifacts.metadata.yearBasis")}{" "}
              <select
                value={dateChoice ?? ""}
                onChange={(e) => {
                  const choice = dateChoices.find(
                    (d) => d.id === e.target.value,
                  );
                  void save({
                    ...content,
                    _yearSelection: choice
                      ? { owner: choice.owner, date: choice.date }
                      : null,
                  });
                }}
              >
                <option value="">{t("artifacts.metadata.autoPubDate")}</option>
                {dateChoices.map((d) => (
                  <option key={d.id} value={d.id}>
                    {metaWord(d.owner, t)} · {metaWord(d.date.event, t)} ·{" "}
                    {d.date.dateText}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("artifacts.metadata.manualYear")}{" "}
              <input
                type="number"
                aria-label={t("artifacts.metadata.manualYear")}
                key={JSON.stringify(content._yearSelection)}
                defaultValue={content._yearSelection?.value ?? ""}
                onBlur={(e) => {
                  if (e.target.value !== "")
                    void save({
                      ...content,
                      _yearSelection: { value: Number(e.target.value) },
                    });
                }}
              />
            </label>
            <button
              type="button"
              onClick={() =>
                void save({ ...content, _yearSelection: { value: null } })
              }
            >
              {t("artifacts.metadata.hideYear")}
            </button>
            <label>
              {t("artifacts.metadata.abstractLangPref")}{" "}
              <select
                value={languageCode(content._preferredLanguage ?? locale)}
                onChange={(e) =>
                  void save({ ...content, _preferredLanguage: e.target.value })
                }
              >
                {[
                  ...new Set([
                    prefZh,
                    prefEn,
                    ...abstracts.flatMap((a) => list(a.languages).map(languageCode)),
                  ]),
                ].map((language) => (
                  <option key={language} value={language}>
                    {language === prefZh
                      ? t("artifacts.metadata.prefZhLabel")
                      : language === prefEn
                        ? t("artifacts.metadata.prefEnLabel")
                        : language}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("artifacts.metadata.abstractChoice")}{" "}
              <select
                value={
                  content._abstractSelection
                    ? String(
                        abstracts.findIndex(
                          (a) =>
                            JSON.stringify(a) ===
                            JSON.stringify(content._abstractSelection),
                        ),
                      )
                    : "auto"
                }
                onChange={(e) =>
                  void save({
                    ...content,
                    _abstractSelection:
                      e.target.value === "auto"
                        ? null
                        : abstracts[Number(e.target.value)],
                  })
                }
              >
                <option value="auto">{t("artifacts.metadata.autoAbstract")}</option>
                {abstracts.map((a, i) => (
                  <option key={i} value={i}>
                    {a.label || t("artifacts.metadata.abstractN", { n: i + 1 })} ·{" "}
                    {list(a.languages).join(t("artifacts.metadata.listSep"))} ·{" "}
                    {metaWord(a.completeness, t)}
                  </option>
                ))}
              </select>
            </label>
          </fieldset>
        )}
        {!selected && content._abstractSelection && (
          <p>{t("artifacts.metadata.abstractUnmatched")}</p>
        )}
        {selected && (
          <section className="metadata-abstract">
            <h3>
              {selected.label || t("artifacts.metadata.fields.abstracts")} ·{" "}
              {metaWord(selected.completeness, t)}
            </h3>
            {list(selected.segments).map((segment, i) => (
              <div key={i}>
                {list(content._effectiveIssues ?? content.issues).some(
                  (issue) =>
                    issue.textGap?.segmentsPath ===
                      `/document/abstracts/${view.abstractIndex}/segments` &&
                    issue.textGap.beforeIndex === i,
                ) && <p className="metadata-gap">{t("artifacts.metadata.textGap")}</p>}
                {segment.heading && <h4>{segment.heading}</h4>}
                <Markdown>{segment.text}</Markdown>
                {i === selected.segments.length - 1 &&
                  list(content._effectiveIssues ?? content.issues).some(
                    (issue) =>
                      issue.textGap?.segmentsPath ===
                        `/document/abstracts/${view.abstractIndex}/segments` &&
                      issue.textGap.afterIndex === i &&
                      issue.textGap.beforeIndex === null,
                  ) && <p className="metadata-gap">{t("artifacts.metadata.textGapAfter")}</p>}
              </div>
            ))}
            {list(content._effectiveIssues ?? content.issues)
              .filter(
                (issue) =>
                  issue.textGap?.segmentsPath ===
                    `/document/abstracts/${view.abstractIndex}/segments` &&
                  issue.textGap.beforeIndex === null &&
                  issue.textGap.afterIndex === null,
              )
              .map((issue, i) => (
                <p key={i} className="metadata-gap">
                  {t("artifacts.metadata.gapUnknown", { explanation: issue.explanation })}
                </p>
              ))}
          </section>
        )}
        {(["document", "container", "relatedVersions"] as const).map((key) => (
          <Field
            key={key}
            name={key}
            path={`/${key}`}
            value={content[key]}
            onChange={onUpdate && !saving ? change : undefined}
            locked={pins}
            onUnlock={unlock}
          />
        ))}
        {!!list(content._sourceLinks).some((links) =>
          list(links).some((link) => link === null),
        ) && <p>{t("artifacts.metadata.sourceUnverified")}</p>}
        <AuxiliarySources sources={content.sources} onJump={onJump} />
        {!!list(content.issues).length && (
          <details open>
            <summary>
              {t("artifacts.metadata.issuesCount", {
                count: content.issues.length,
              })}
            </summary>
            {content.issues.map((issue: Row, i: number) => (
              <article key={i}>
                {content._issueLinks?.[i] === false && (
                  <small>{t("artifacts.metadata.issueUnlinked")}</small>
                )}
                <Markdown>{issue.explanation}</Markdown>
                {list(issue.candidates).map((c, j) => (
                  <blockquote key={j}>
                    <p>{c.text}</p>
                    <p>{c.explanation}</p>
                  </blockquote>
                ))}
              </article>
            ))}
          </details>
        )}
        {!!list(content._unmatchedManual).length && (
          <details open>
            <summary>{t("artifacts.metadata.unmatchedManual")}</summary>
            {content._unmatchedManual.map((entry: Row, i: number) => (
              <p key={i}>
                {typeof entry.setting?.value === "string"
                  ? entry.setting.value
                  : t("artifacts.metadata.manualKeptCheck")}
              </p>
            ))}
          </details>
        )}
        {content._legacy && Object.keys(content._legacy).length > 0 && (
          <details>
            <summary>{t("artifacts.metadata.legacyRecords")}</summary>
            {Object.entries(content._legacy)
              .filter(([k]) => !k.startsWith("_"))
              .map(([k, v]) => (
                <p key={k}>
                  <strong>{metaField(k, t)}：</strong>
                  {typeof v === "string" || typeof v === "number"
                    ? String(v)
                    : Array.isArray(v)
                      ? v
                          .filter((x) => typeof x === "string")
                          .join(t("artifacts.metadata.listSep"))
                      : t("artifacts.metadata.keptInHistory")}
                </p>
              ))}
          </details>
        )}
        <details>
          <summary>{t("artifacts.metadata.modelDraft")}</summary>
          {(["document", "container", "relatedVersions"] as const).map(
            (key) => (
              <Field
                key={key}
                name={key}
                path={`/${key}`}
                value={content._model?.[key]}
                locked={new Set()}
                onUnlock={() => {}}
              />
            ),
          )}
        </details>
      </div>
    </IdentityContext.Provider>
  );
}
