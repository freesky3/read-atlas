import ts from "typescript";
import { describe, expect, it } from "vitest";
import { flatten } from "./t";
import { en } from "./messages/en";
import { zhCN } from "./messages/zh-CN";
import { completionEntries } from "./messages/completion";
const sources = import.meta.glob("../**/*.{ts,tsx}", { eager: true, query: "?raw", import: "default" }) as Record<string, string>;
describe("UI catalog coverage", () => {
  it("resolves every static UI key and helper label used by the application", () => {
    const english = new Set(flatten(en)), chinese = new Set(flatten(zhCN));
    const labels = new Set(Object.values(completionEntries).flat());
    const failures: string[] = [];
    let checked = 0;
    for (const [file, content] of Object.entries(sources).filter(([file]) => file.startsWith("../") && !file.includes(".test.") && !file.includes("/i18n/"))) {
      const source = ts.createSourceFile(file, content, ts.ScriptTarget.Latest, true);
      const visit = (node: ts.Node) => {
        if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
          const name = node.expression.text, arg = node.arguments[name === "uiText" ? 1 : 0];
          if ((name === "t" || name === "uiText") && arg && ts.isStringLiteral(arg)) {
            checked += 1;
            const valid = name === "t" ? english.has(arg.text) && chinese.has(arg.text) : labels.has(arg.text);
            if (!valid) failures.push(file + ": " + arg.text);
          }
        }
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(checked).toBeGreaterThan(500);
    expect(failures).toEqual([]);
  });
  it("keeps interpolation variables identical across languages", () => {
    const variables = (tree: Record<string, unknown>, prefix = "", result: Record<string, string[]> = {}) => {
      for (const [key, value] of Object.entries(tree)) {
        const full = prefix ? prefix + "." + key : key;
        if (typeof value === "string") result[full] = (value.match(/\{\w+\}/g) ?? []).sort();
        else variables(value as Record<string, unknown>, full, result);
      }
      return result;
    };
    expect(variables(zhCN)).toEqual(variables(en));
  });
});
