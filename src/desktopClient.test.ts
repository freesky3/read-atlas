import { describe, expect, it, vi } from "vitest";
import { createMemoryDesktopClient } from "./desktopClient";
import type { PromptSettings } from "./types";

describe("memory DesktopClient", () => {
  it("upgrades one outline bundle while preserving its previous text and other kind", async () => {
    const client = createMemoryDesktopClient();
    for (const slot of ["outline_extract", "outline_compose", "outline_deep_dive"]) {
      await client.command("save_prompt_slot", { request: { slot, kind: "paper", text: `${slot} paper custom` } });
      await client.command("save_prompt_slot", { request: { slot, kind: "textbook", text: `${slot} textbook custom` } });
    }
    const upgraded = await client.command<PromptSettings>("restore_outline_prompt_bundle", { request: { kind: "paper" } });
    for (const slot of ["outline_extract", "outline_compose", "outline_deep_dive"]) {
      expect(upgraded.slots[slot].paper).toMatchObject({ isDefault: true, outputProtocol: "v4", previousText: `${slot} paper custom` });
      expect(upgraded.slots[slot].textbook.text).toBe(`${slot} textbook custom`);
    }
    await client.command("restore_outline_prompt_bundle", { request: { kind: "paper" } });
    await client.command("restore_prompt_default", { request: { slot: "outline_extract", kind: "paper" } });
    const restored = await client.command<PromptSettings>("restore_prompt_previous", { request: { slot: "outline_extract", kind: "paper" } });
    expect(restored.slots.outline_extract.paper.text).toBe("outline_extract paper custom");
    expect(restored.slots.outline_extract.paper.outputProtocol).toBe("v4");
  });

  it("opens honest empty projections without manufacturing paper results", async () => {
    const client = createMemoryDesktopClient();

    await expect(client.open("get_workspace")).resolves.toBeNull();
    await expect(client.open("list_documents")).resolves.toEqual([]);
    await expect(client.open("get_brief")).resolves.toBeNull();
    await expect(client.open("latest_ocr")).resolves.toBeNull();
    await expect(client.open("get_outline")).resolves.toMatchObject({
      status: "missing_ocr",
      hasPaperRoot: false,
      catalog: null,
      head: null,
    });
    await expect(client.open("get_reading_guide")).resolves.toMatchObject({
      status: "missing_ocr",
      hasPaperRoot: false,
      head: null,
    });
    await expect(client.open("get_prompt_settings")).resolves.toMatchObject({
      schemaVersion: 3,
    });
    await expect(client.open("preview_diagnostics")).resolves.toMatchObject({
      includedSections: [],
      excludedData: [],
    });
  });

  it("rejects capabilities that require the desktop runtime", async () => {
    const client = createMemoryDesktopClient();

    await expect(client.command("generate_brief")).rejects.toThrow("requires the Read Atlas runtime");
    await expect(client.command("generate_document_artifact", {
      request: { revisionId: "r", kind: "glossary", useBrief: false },
    })).rejects.toThrow("requires the Read Atlas runtime");
    await expect(client.command("send_chat")).rejects.toThrow(
      "send_chat requires the Read Atlas runtime",
    );
    await expect(client.command("plan_outline")).rejects.toThrow(
      "plan_outline requires the Read Atlas runtime",
    );
    await expect(client.command("delete_reading_guide")).rejects.toThrow(
      "delete_reading_guide requires the Read Atlas runtime",
    );
    await expect(client.command("plan_reading_guide")).rejects.toThrow(
      "plan_reading_guide requires the Read Atlas runtime",
    );
    await expect(client.command("start_reading_guide")).rejects.toThrow(
      "start_reading_guide requires the Read Atlas runtime",
    );
  });

  it("stores reader context in memory and deletes on empty save", async () => {
    const client = createMemoryDesktopClient();
    await client.command("save_reader_context", {
      request: { scope: "workspace", text: "  I know linear algebra  " },
    });
    await expect(
      client.command("get_reader_context", { request: { scope: "workspace" } }),
    ).resolves.toMatchObject({
      scope: "workspace",
      text: "I know linear algebra",
      warn: false,
    });
    await client.command("save_reader_context", {
      request: { scope: "workspace", text: "   " },
    });
    await expect(
      client.command("get_reader_context", { request: { scope: "workspace" } }),
    ).resolves.toMatchObject({ text: "" });
  });

  it("provides a no-op event subscription for browser tests", async () => {
    const listener = vi.fn();
    const unlisten = await createMemoryDesktopClient().watch(listener);
    expect(() => unlisten()).not.toThrow();
    expect(listener).not.toHaveBeenCalled();
  });

  it("accepts dynamic provider commands without throwing", async () => {
    const client = createMemoryDesktopClient();

    await expect(
      client.command("add_provider", {
        request: {
          name: "DeepSeek",
          kind: "openai_compatible",
          baseUrl: "https://api.deepseek.com/v1",
          paperModel: "deepseek-chat",
          translationModel: "deepseek-chat",
        },
      }),
    ).resolves.toMatchObject({
      providers: expect.arrayContaining([
        expect.objectContaining({ name: "DeepSeek", kind: "openai_compatible" }),
      ]),
    });

    const settingsAfterAdd = await client.open<any>("get_model_settings");
    const newProvider = settingsAfterAdd.providers.find(
      (p: any) => p.name === "DeepSeek",
    );
    expect(newProvider).toBeDefined();

    await expect(
      client.command("test_provider_connection", {
        request: {
          id: newProvider.id,
          apiKey: "test-key",
          baseUrl: "https://api.deepseek.com/v1",
          paperModel: "deepseek-chat",
        },
      }),
    ).resolves.toMatchObject({
      paperProbePassed: null,
      modelsFetchError: expect.stringContaining("no provider connection"),
    });

    await expect(
      client.command("save_provider_settings", {
        request: {
          id: newProvider.id,
          apiKey: "test-key",
          baseUrl: "https://api.deepseek.com/v1",
          paperModel: "deepseek-chat",
          translationModel: "deepseek-chat",
        },
      }),
    ).resolves.toMatchObject({
      providers: expect.arrayContaining([
        expect.objectContaining({
          id: newProvider.id,
          credentialConfigured: false,
          paperProbePassed: false,
        }),
      ]),
    });

    await expect(
      client.command("set_current_provider", {
        request: { id: newProvider.id },
      }),
    ).resolves.toMatchObject({ currentProviderId: newProvider.id });

    await expect(
      client.command("clear_provider_credential", {
        request: { id: newProvider.id },
      }),
    ).resolves.toMatchObject({
      providers: expect.arrayContaining([
        expect.objectContaining({ id: newProvider.id, credentialConfigured: false }),
      ]),
    });
  });
});

