import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, statSync } from "node:fs";
import { dirname, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const files = [...new Set(execFileSync("git", [
  "ls-files", "--cached", "--others", "--exclude-standard", "-z",
], { cwd: root, encoding: "utf8" }).split("\0").filter(Boolean))];
const errors = [];
const textExtensions = new Set([".md", ".ts", ".tsx", ".mjs", ".json", ".toml", ".yml", ".yaml", ".css", ".html", ".ps1"]);
for (const file of files) {
  const path = resolve(root, file);
  if (!existsSync(path)) continue;
  if (/^(node_modules|dist|pet-upgrades|Papers|Textbooks|Characters|export|tmp|\.superpowers)\//.test(file)
    || /(^|\/)\.read-desktop\//.test(file)
    || /\.(sqlite3?|db|pem|key|pfx|p12|pdf|exe|msi|log)$/i.test(file)
    || /(^|\/)\.env(?:\.|$)/.test(file) && !file.endsWith(".env.example")) {
    errors.push(file + ": private/generated data in publication set");
  }
  if (statSync(path).size > 10 * 1024 * 1024) errors.push(file + ": exceeds 10 MiB; review before committing");
  if (!textExtensions.has(extname(file))) continue;
  const text = readFileSync(path, "utf8");
  if (/file:\/\/\//i.test(text) || /[A-Z]:[\\/]Users[\\/](?!<|Public|Default|example|test)/.test(text)) {
    errors.push(file + ": machine-specific file URL or user path");
  }
  if (file.endsWith(".md")) {
    // Historical source line numbers are prose; links must resolve as repository files.
    const prose = text.replace(/```[\s\S]*?```/g, "").replace(/`[^`]*`/g, "");
    for (const match of prose.matchAll(/\]\(([^\s)]+)(?:\s+"[^"]*")?\)/g)) {
      const target = match[1].replace(/^<|>$/g, "");
      if (/^(https?:|mailto:|#)/i.test(target)) continue;
      const local = decodeURIComponent(target.split("#")[0]);
      if (!local) continue;
      if (/^[A-Za-z]:[\\/]/.test(local)) {
        errors.push(file + ": absolute local link");
      } else if (!existsSync(resolve(dirname(path), local))) {
        errors.push(file + ": broken link " + target);
      }
    }
  }
}
const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf8"));
const lock = JSON.parse(readFileSync(resolve(root, "package-lock.json"), "utf8"));
const tauri = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"));
const cargo = readFileSync(resolve(root, "src-tauri/Cargo.toml"), "utf8");
if (pkg.license !== "MIT" || lock.packages[""].license !== "MIT" || !/^license = "MIT"$/m.test(cargo)) {
  errors.push("Project MIT metadata is incomplete");
}
if (pkg.version !== tauri.version || !cargo.includes('version = "' + pkg.version + '"')) {
  errors.push("Application versions disagree");
}
if (/(@import\s+(url\()?\s*["']?https?:|url\(["']?https?:)/i.test(readFileSync(resolve(root, "src/styles.css"), "utf8"))) {
  errors.push("UI stylesheet fetches a remote asset");
}
for (const file of ["LICENSE", "README.zh-CN.md", "PRIVACY.md", "SECURITY.md", "THIRD_PARTY_NOTICES.txt"]) {
  if (!existsSync(resolve(root, file))) errors.push("Missing " + file);
}
if (errors.length) {
  console.error(errors.join("\n"));
  process.exitCode = 1;
} else {
  console.log("Repository checks passed for " + files.length + " publication candidate files.");
}
