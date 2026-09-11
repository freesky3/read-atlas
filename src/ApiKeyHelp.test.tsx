import { fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderWithLocale } from "./i18n/testUtils";
import { desktopClient } from "./desktopClient";
import ApiKeyHelp, { keyResourcesFor } from "./ApiKeyHelp";

const clientMock = vi.hoisted(() => ({ runtime: "memory", command: vi.fn() }));
vi.mock("./desktopClient", () => ({ desktopClient: clientMock }));
afterEach(() => { clientMock.runtime = "memory"; vi.restoreAllMocks(); clientMock.command.mockReset(); });

describe("API key help", () => {
  it("matches the endpoint host and avoids sending custom/proxy users to the wrong provider", () => {
    expect(keyResourcesFor("openai_compatible", "https://api.deepseek.com/v1")).toEqual(["deepseek"]);
    expect(keyResourcesFor("openai_compatible", "https://api.openai.com/v1")).toEqual(["openai"]);
    expect(keyResourcesFor("openai_compatible", "https://api.openai.com.evil.example/v1")).toEqual([]);
    expect(keyResourcesFor("openai_compatible", "http://localhost:11434/v1")).toEqual([]);
    expect(keyResourcesFor("gemini_proxy", "http://localhost:8045/v1")).toEqual([]);
  });

  it.each(["zh-CN", "en"] as const)("renders official key links in %s without submitting credentials", (locale) => {
    const command = vi.spyOn(desktopClient, "command");
    renderWithLocale(<ApiKeyHelp kind="mistral" onError={vi.fn()} />, locale);
    const link = screen.getByRole("link", { name: /Mistral Console/ });
    expect(link).toHaveAttribute("href", "https://console.mistral.ai");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(command).not.toHaveBeenCalled();
    command.mockRestore();
  });

  it("passes only a resource identifier to the native opener", async () => {
    clientMock.runtime = "desktop";
    const command = vi.spyOn(desktopClient, "command").mockResolvedValue(undefined);
    try {
      renderWithLocale(<ApiKeyHelp kind="gemini" onError={vi.fn()} />, "en");
      fireEvent.click(screen.getByRole("link", { name: /Google AI Studio/ }));
      expect(command).toHaveBeenCalledWith("open_api_key_page", { request: { resource: "gemini" } });
    } finally { command.mockRestore(); clientMock.runtime = "memory"; }
  });
});
