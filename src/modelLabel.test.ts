import { describe, expect, it } from "vitest";
import {
  formatModelLabel,
  isPaperProviderReady,
  paperProviderComposerHint,
  paperProviderComposerPlaceholder,
  paperProviderConnectionStatus,
  paperProviderEmptyBody,
  paperProviderEmptyHeading,
  paperProviderLabel,
  paperProviderNotConfiguredStatus,
  paperProviderSetupTitle,
} from "./modelLabel";
import type { ModelSettings, ProviderInstanceView } from "./types";

const makeProvider = (
  id: string,
  kind: "gemini" | "openai_compatible" | "grok",
  name: string,
  opts: Partial<ProviderInstanceView> = {},
): ProviderInstanceView => ({
  id,
  name,
  kind,
  baseUrl: null,
  paperModel: kind === "gemini" ? "gemini-2.5-flash" : "gpt-4.1",
  translationModel: kind === "gemini" ? "gemini-2.5-flash-lite" : "gpt-4.1-mini",
  models: [],
  modelsFetchedAt: null,
  connectionVerifiedAt: null,
  paperProbe: null,
  credentialConfigured: false,
  paperProbePassed: false,
  isCurrent: false,
  sortOrder: 1,
  ...opts,
});

function makeSettings(
  currentId: string | null = "p-gemini-1",
  providers: ProviderInstanceView[] = [
    makeProvider("p-gemini-1", "gemini", "Google Gemini", { isCurrent: true }),
  ],
): ModelSettings {
  return {
    currentProviderId: currentId,
    providers,
    credentialStore: "Windows Credential Manager",
    mistralCredentialConfigured: false,
    mistralCredentialStore: "Windows Credential Manager",
    ocrModel: "mistral-ocr-latest",
  };
}

describe("formatModelLabel", () => {
  it("prefers the catalog display name for the stored model id", () => {
    expect(
      formatModelLabel("gemini-3.6-flash", [
        {
          id: "gemini-3.6-flash",
          displayName: "Gemini 3.6 Flash",
          description: "",
          inputTokenLimit: null,
          outputTokenLimit: null,
          supportsGenerateContent: true,
          supportsNativePdf: true,
          supportsInteractions: true,
        },
      ]),
    ).toBe("Gemini 3.6 Flash");
  });

  it("pretty-prints a model id when the catalog has no match", () => {
    expect(formatModelLabel("gemini-3.6-flash")).toBe("Gemini 3.6 Flash");
    expect(formatModelLabel(undefined, [], "gemini-2.5-flash")).toBe(
      "Gemini 2.5 Flash",
    );
  });
});

describe("paper provider empty-state copy", () => {
  it("maps provider kinds to user-visible labels", () => {
    expect(paperProviderLabel("gemini")).toBe("Gemini");
    expect(paperProviderLabel("openai_compatible")).toBe("OpenAI-compatible");
    expect(paperProviderLabel("grok")).toBe("Grok");
  });

  it("is ready only when the current slot has a key and gemini verified or a compatible probe", () => {
    expect(isPaperProviderReady(makeSettings())).toBe(false);

    expect(
      isPaperProviderReady(
        makeSettings("p-gemini-1", [
          makeProvider("p-gemini-1", "gemini", "Google Gemini", {
            credentialConfigured: true,
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe(false);

    expect(
      isPaperProviderReady(
        makeSettings("p-gemini-1", [
          makeProvider("p-gemini-1", "gemini", "Google Gemini", {
            credentialConfigured: true,
            connectionVerifiedAt: "2026-08-18T00:00:00.000Z",
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe(true);

    expect(
      isPaperProviderReady(
        makeSettings("p-openai-1", [
          makeProvider("p-openai-1", "openai_compatible", "OpenAI", {
            credentialConfigured: true,
            paperProbePassed: false,
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe(false);

    expect(
      isPaperProviderReady(
        makeSettings("p-grok-1", [
          makeProvider("p-grok-1", "grok", "xAI Grok", {
            credentialConfigured: true,
            paperProbePassed: true,
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe(true);
  });

  it("builds unconfigured empty-state and composer copy from the current provider", () => {
    const kinds = ["gemini", "openai_compatible", "grok"] as const;
    const labels = ["Gemini", "OpenAI-compatible", "Grok"] as const;
    kinds.forEach((kind, index) => {
      const label = labels[index];
      expect(paperProviderNotConfiguredStatus(kind)).toBe(
        `${label} is not configured · open Settings to continue`,
      );
      expect(paperProviderComposerPlaceholder(kind)).toBe(
        `Configure ${label} to ask this paper…`,
      );
      expect(paperProviderEmptyHeading(kind)).toBe(
        `Connect ${label} to begin.`,
      );
      expect(paperProviderEmptyBody(kind)).toContain(
        `verified ${label} configuration`,
      );
      expect(paperProviderSetupTitle(kind)).toBe(
        `${label} is not configured`,
      );
      expect(paperProviderComposerHint(kind, "点击配置")).toBe(
        `${label} not configured (点击配置)`,
      );
    });
  });

  it("builds the status bar for a ready provider and a cleared credential", () => {
    expect(
      paperProviderConnectionStatus(
        makeSettings("p-openai-1", [
          makeProvider("p-openai-1", "openai_compatible", "OpenAI Custom", {
            paperModel: "gpt-4.1",
            credentialConfigured: true,
            paperProbePassed: true,
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe("OpenAI Custom ready · gpt-4.1");

    expect(
      paperProviderConnectionStatus(
        makeSettings("p-grok-1", [
          makeProvider("p-grok-1", "grok", "xAI Grok", {
            paperModel: "grok-4",
            isCurrent: true,
          }),
        ]),
      ),
    ).toBe("xAI Grok credential cleared");

    expect(
      paperProviderConnectionStatus(
        makeSettings(null, []),
      ),
    ).toBe("No active provider selected");
  });
});

