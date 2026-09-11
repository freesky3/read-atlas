import { describe, expect, it } from "vitest";
import config from "../src-tauri/tauri.conf.json";

const directives = new Map(config.app.security.csp.split(";").filter(Boolean).map(item => {
  const [name, ...sources] = item.trim().split(/\s+/);
  return [name, sources];
}));
describe("Packaged desktop resource policy", () => {
  it("allows Windows local PDF fetches and the Tauri IPC transport", () => {
    expect(directives.get("connect-src")).toEqual(expect.arrayContaining(["asset:", "http://asset.localhost", "ipc:", "http://ipc.localhost"]));
    expect(directives.get("img-src")).toContain("http://asset.localhost");
  });
  it("keeps arbitrary web origins and disk paths outside the default allowlist", () => {
    expect(directives.get("connect-src")).not.toContain("*");
    expect(directives.get("connect-src")).not.toContain("http:");
    expect(config.app.security.assetProtocol.scope).toEqual([]);
    expect(config.app.security.csp).not.toContain("'unsafe-eval'");
  });
});
