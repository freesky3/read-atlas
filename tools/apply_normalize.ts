// Re-runs the production normalizer against the live lens database and prints
// each artifact's resulting markdown fields. Does NOT write back to the DB.
//
// Run with: npx tsx tools/apply_normalize.ts

// We re-implement the same Rust normalizer here in TypeScript so the test
// reflects what the backend will write on the next generate. The Rust source
// is in src-tauri/src/reading_artifact_module.rs and is the source of truth.

function charsToString(chars: string[]): string {
  return chars.join("");
}

function detectListMarker(input: string, i: number): number {
  let j = i;
  let digitCount = 0;
  while (j < input.length && /[0-9]/.test(input[j]) && digitCount < 3) {
    j += 1;
    digitCount += 1;
  }
  if (digitCount === 0) {
    if (input[i] === "-" && input[i + 1] === " ") return 2;
    if (input[i] === "·") return 1;
    return 0;
  }
  if (j >= input.length) return 0;
  const sep = input[j];
  if (sep === "." || sep === ")" || sep === "．") {
    if (j + 1 < input.length && input[j + 1] === " ") return j + 1 - i + 1;
    if (j + 1 === input.length) return j - i + 1;
    return 0;
  }
  if (sep === "、") return j - i + 1;
  return 0;
}

function isListTerminator(c: string): boolean {
  return /[。！!,;?：:]/.test(c);
}

function splitPackedListItems(input: string): string {
  const chars = Array.from(input);
  const out: string[] = [];
  let i = 0;
  let lineStart = true;
  let prevWasTerminator = true;
  while (i < chars.length) {
    const c = chars[i];
    if (c === "\n") {
      out.push("\n");
      i += 1;
      lineStart = true;
      prevWasTerminator = true;
      continue;
    }
    if (!lineStart && c === " ") {
      out.push(" ");
      i += 1;
      continue;
    }
    const remaining = charsToString(chars.slice(i));
    const markerLen = detectListMarker(remaining, 0);
    if (markerLen > 0 && (lineStart || prevWasTerminator)) {
      if (!lineStart) out.push("\n");
      out.push(...chars.slice(i, i + markerLen));
      i += markerLen;
      lineStart = false;
      prevWasTerminator = false;
      continue;
    }
    out.push(c);
    if (c.trim() !== "") lineStart = false;
    prevWasTerminator = isListTerminator(c);
    i += 1;
  }
  return out.join("");
}

function repairInlineBold(input: string): string {
  if (input.includes("\\**")) {
    return repairInlineBold(input.replace(/\\\*\*/g, "**"));
  }
  const chars = Array.from(input);
  if (chars.length === 0) return input;
  const positions: number[] = [];
  for (let i = 0; i + 1 < chars.length; i++) {
    if (chars[i] === "*" && chars[i + 1] === "*") {
      positions.push(i);
      i += 1;
    }
  }
  if (positions.length === 0) return input;
  if (positions.length % 2 === 1) {
    const stray = positions[positions.length - 1];
    let splitAt = chars.length;
    for (let k = stray + 2; k < chars.length; k++) {
      if (chars[k] === "\n") {
        splitAt = k;
        break;
      }
    }
    const next = [...chars.slice(0, splitAt), "*", "*", ...chars.slice(splitAt)];
    return repairInlineBold(next.join(""));
  }
  const result: string[] = [];
  let idx = 0;
  for (let p = 0; p < positions.length; p += 2) {
    const open = positions[p];
    const close = positions[p + 1];
    result.push(...chars.slice(idx, open));
    result.push("*", "*");
    let cs = open + 2;
    while (cs < close && (chars[cs] === " " || chars[cs] === "\t")) cs += 1;
    let ce = close;
    while (ce > cs && (chars[ce - 1] === " " || chars[ce - 1] === "\t")) ce -= 1;
    result.push(...chars.slice(cs, ce));
    result.push("*", "*");
    idx = close + 2;
  }
  result.push(...chars.slice(idx));
  return result.join("");
}

function normalize(input: string): string {
  let out = input.replace(/\\r/g, "").replace(/\\t/g, "\t");
  if (out.includes("\\n")) out = out.replace(/\\n/g, "\n");
  out = out.replace(/[ \t]{2,}/g, " ");
  out = repairInlineBold(out);
  out = splitPackedListItems(out);
  return out;
}

import Database from "better-sqlite3";
const path = process.env.READ_DESKTOP_DB;
if (!path) throw new Error("Set READ_DESKTOP_DB to the database you want to inspect (read-only).");
const db = new Database(path, { readonly: true });
const rows = db.prepare("SELECT id, kind, content_json FROM artifacts WHERE kind LIKE 'lens_%' ORDER BY created_at DESC LIMIT 2").all();
for (const r of rows) {
  const v = JSON.parse(r.content_json);
  console.log(`\n=== ${r.id} ${r.kind} ===`);
  if (v.quickTakeaway?.markdown) {
    const normalized = normalize(v.quickTakeaway.markdown);
    console.log("-- quickTakeaway --");
    console.log("BEFORE:", JSON.stringify(v.quickTakeaway.markdown));
    console.log("AFTER :", JSON.stringify(normalized));
  }
  if (v.overallMarkdown?.markdown) {
    console.log("-- overallMarkdown --");
    console.log("BEFORE:", JSON.stringify(v.overallMarkdown.markdown));
    console.log("AFTER :", JSON.stringify(normalize(v.overallMarkdown.markdown)));
  }
  for (const s of (v.sections ?? []).slice(0, 1)) {
    if (s.markdown) {
      console.log(`-- section ${s.sectionId} --`);
      console.log("BEFORE:", JSON.stringify(s.markdown));
      console.log("AFTER :", JSON.stringify(normalize(s.markdown)));
    }
  }
}
db.close();
