import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const python = process.env.READ_DESKTOP_PYTHON || "python";
const result = spawnSync(python, ["tools/generate-pdf-fixtures.py"], { cwd: root, stdio: "inherit" });
if (result.error) console.error(result.error.message);
if (result.status !== 0) console.error("Install Python with reportlab and Pillow; or set READ_DESKTOP_PYTHON to that Python executable.");
process.exitCode = result.status ?? 1;
