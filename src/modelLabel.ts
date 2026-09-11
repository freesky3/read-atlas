import { uiText, enT, type TranslateFn } from "./i18n/uiText";
import type { GeminiModelOption, ModelSettings, ProviderInstanceView, ProviderKind } from "./types";

export function formatModelLabel(
  modelId: string | null | undefined,
  models: readonly GeminiModelOption[] = [],
  fallbackId = "",
) {
  const id = (modelId || fallbackId).trim();
  if (!id) return "AI Model";
  const named = models.find((model) => model.id === id);
  if (named?.displayName) return named.displayName;
  return id
    .replace(/^models\//, "")
    .split(/[-_]/)
    .filter(Boolean)
    .map((part) =>
      /^\d/.test(part) ? part : part.charAt(0).toUpperCase() + part.slice(1),
    )
    .join(" ");
}

export function currentPaperProviderSlot(settings: ModelSettings): ProviderInstanceView | null {
  if (!settings.currentProviderId) return null;
  return settings.providers.find((p) => p.id === settings.currentProviderId) ?? null;
}

export function isPaperProviderReady(settings: ModelSettings): boolean {
  const inst = currentPaperProviderSlot(settings);
  if (!inst || !inst.credentialConfigured) return false;
  return inst.kind === "gemini"
    ? Boolean(inst.connectionVerifiedAt)
    : inst.paperProbePassed;
}

export function paperProviderLabel(provider?: ProviderKind | string | null): string {
  if (provider === "gemini_proxy") return "Gemini Proxy";
  if (provider === "openai_compatible") return "OpenAI-compatible";
  if (provider === "grok") return "Grok";
  if (provider === "gemini") return "Gemini";
  return provider || "Provider";
}

export function paperProviderNotConfiguredStatus(provider?: string | null, t: TranslateFn = enT) {
  return uiText(t, "{provider} is not configured · open Settings to continue", { provider: paperProviderLabel(provider) });
}

export function paperProviderConnectionStatus(settings: ModelSettings, t: TranslateFn = enT) {
  const inst = currentPaperProviderSlot(settings);
  if (!inst) return uiText(t, "No active provider selected");
  return isPaperProviderReady(settings)
    ? uiText(t, "{provider} ready · {model}", { provider: inst.name, model: inst.paperModel })
    : uiText(t, "{provider} credential cleared", { provider: inst.name });
}

export function paperProviderComposerPlaceholder(provider?: string | null, t: TranslateFn = enT) {
  return uiText(t, "Configure {provider} to ask this paper…", { provider: paperProviderLabel(provider) });
}

export function paperProviderEmptyHeading(provider?: string | null, t: TranslateFn = enT) {
  return uiText(t, "Connect {provider} to begin.", { provider: paperProviderLabel(provider) });
}

export function paperProviderEmptyBody(provider?: string | null, t: TranslateFn = enT) {
  return uiText(t, "PDF Chat is paused until a verified {provider} configuration is active.", { provider: paperProviderLabel(provider) });
}

export function paperProviderSetupTitle(provider?: string | null, t: TranslateFn = enT) {
  return uiText(t, "{provider} is not configured", { provider: paperProviderLabel(provider) });
}

export function paperProviderComposerHint(
  provider?: string | null,
  configureAction = "click to configure",
  t: TranslateFn = enT,
) {
  return uiText(t, "{provider} not configured ({action})", { provider: paperProviderLabel(provider), action: configureAction });
}
