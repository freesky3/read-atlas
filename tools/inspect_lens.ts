// Read-only inspector for Lens artifacts in the workspace database.
// Run with: npx tsx tools/inspect_lens.ts [N]
import Database from "better-sqlite3";

const dbPath = process.env.READ_DESKTOP_DB;
if (!dbPath) throw new Error("Set READ_DESKTOP_DB to the database you want to inspect (read-only).");
const db = new Database(dbPath, { readonly: true });
const limit = Number(process.argv[2] ?? 6);

const rows = db
  .prepare(
    `SELECT id, kind, version, status, length(content_json) AS len, content_json
     FROM artifacts
     WHERE kind LIKE 'lens_%'
     ORDER BY created_at DESC
     LIMIT ?`,
  )
  .all(limit);

for (const r of rows) {
  console.log(`=== ${r.id} ${r.kind} v${r.version} ${r.status} (${r.len} bytes) ===`);
  let parsed: any = null;
  try {
    parsed = JSON.parse(r.content_json);
  } catch (e) {
    console.log("JSON parse error", e);
    continue;
  }
  const qt = parsed?.quickTakeaway;
  if (qt) {
    console.log("-- quickTakeaway.title --");
    console.log(qt.title ?? "");
    console.log("-- quickTakeaway.markdown --");
    console.log(qt.markdown ?? "");
  }
  const overall = parsed?.overallMarkdown ?? parsed?.explanationMarkdown ?? parsed?.summaryMarkdown;
  if (overall) {
    console.log("-- overall/explanation.markdown --");
    console.log(overall.markdown ?? "");
  }
  const sections = parsed?.sections ?? [];
  for (const s of sections.slice(0, 4)) {
    console.log(`-- section ${s.sectionId} ${s.title} --`);
    console.log(s.markdown ?? "");
  }
}

db.close();
