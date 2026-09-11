import resources from "./providerKeyResources.json";
import { desktopClient } from "./desktopClient";
import { useLocale } from "./i18n/LocaleContext";
import type { ProviderKind } from "./types";

export type ApiKeyResource = keyof typeof resources;

export function keyResourcesFor(kind: ProviderKind | "mistral", baseUrl = ""): ApiKeyResource[] {
  if (kind === "gemini") return ["gemini"];
  if (kind === "mistral") return ["mistral"];
  if (kind === "grok") return ["grok"];
  if (kind === "gemini_proxy") return [];
  let host = "";
  try { host = new URL(baseUrl).hostname; } catch { /* A draft endpoint can be incomplete. */ }
  if (host === "api.openai.com") return ["openai"];
  if (host === "api.deepseek.com") return ["deepseek"];
  if (host === "api.x.ai") return ["grok"];
  if (host === "generativelanguage.googleapis.com") return ["gemini"];
  if (host === "api.mistral.ai") return ["mistral"];
  return [];
}

export default function ApiKeyHelp({ kind, baseUrl, onError }: {
  kind: ProviderKind | "mistral";
  baseUrl?: string;
  onError: (message: string) => void;
}) {
  const { t } = useLocale();
  const links = keyResourcesFor(kind, baseUrl);
  const open = async (resource: ApiKeyResource) => {
    try {
      await desktopClient.command("open_api_key_page", { request: { resource } });
    } catch {
      onError(t("settings.models.keyPageFailed", { url: resources[resource].url }));
    }
  };
  return (
    <div className="api-key-help">
      {links.map(resource => (
        <a key={resource} href={resources[resource].url} target="_blank" rel="noopener noreferrer"
          onClick={event => {
            if (desktopClient.runtime !== "desktop") return;
            event.preventDefault();
            void open(resource);
          }}>
          {t("settings.models.getApiKey", { provider: resources[resource].name })} <span aria-hidden="true">↗</span>
        </a>
      ))}
      {kind === "gemini_proxy" ? <span className="field-hint">{t("settings.models.proxyKeyHelp")}</span> : null}
      {kind === "openai_compatible" && links.length === 0 ? (
        <span className="field-hint">{t("settings.models.customKeyHelp")}</span>
      ) : null}
    </div>
  );
}
