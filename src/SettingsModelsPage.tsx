import React, { useState, useEffect, useMemo, useRef } from "react";
import { desktopClient } from "./desktopClient";
import ApiKeyHelp from "./ApiKeyHelp";
import type {
  ConnectionTestResult,
  GeminiModelOption,
  ModelSettings,
  ProviderInstanceView,
  ProviderKind,
} from "./types";
import { paperProviderLabel } from "./modelLabel";
import { useLocale, type TranslateFn } from "./i18n/LocaleContext";

interface SettingsModelsPageProps {
  modelSettings: ModelSettings;
  initialProviderId?: string | null;
  focusNonce?: number;
  onModelSettingsChange: (settings: ModelSettings) => void;
  setStatus: (status: string) => void;
}

function defaultNameForKind(kind: ProviderKind, count: number, t: TranslateFn): string {
  if (kind === "gemini") {
    return count === 0
      ? t("settings.models.defaultName.gemini")
      : t("settings.models.defaultName.geminiN", { n: count + 1 });
  }
  if (kind === "gemini_proxy") {
    return count === 0
      ? t("settings.models.defaultName.geminiProxy")
      : t("settings.models.defaultName.geminiProxyN", { n: count + 1 });
  }
  if (kind === "openai_compatible") {
    return count === 0
      ? t("settings.models.defaultName.openai")
      : t("settings.models.defaultName.openaiN", { n: count + 1 });
  }
  if (kind === "grok") {
    return count === 0
      ? t("settings.models.defaultName.grok")
      : t("settings.models.defaultName.grokN", { n: count + 1 });
  }
  return t("settings.models.defaultName.providerN", { n: count + 1 });
}

interface GeminiProxyFamilyDef {
  baseId: string;
  name: string;
  thinkingType: "flash3" | "pro2" | "none";
  defaultTier?: "low" | "medium" | "high";
  tag?: string;
}

const GEMINI_PROXY_FAMILIES: GeminiProxyFamilyDef[] = [
  { baseId: "gemini-3.7-flash", name: "Gemini 3.7 Flash", thinkingType: "flash3", defaultTier: "high", tag: "latestMain" },
  { baseId: "gemini-3.6-flash", name: "Gemini 3.6 Flash", thinkingType: "flash3", defaultTier: "medium", tag: "fast" },
  { baseId: "gemini-3.5-flash", name: "Gemini 3.5 Flash", thinkingType: "flash3", defaultTier: "medium", tag: "fast" },
  { baseId: "gemini-3.1-pro", name: "Gemini 3.1 Pro", thinkingType: "pro2", defaultTier: "high", tag: "deepReasoning" },
  { baseId: "gemini-3.1-flash-lite", name: "Gemini 3.1 Flash Lite", thinkingType: "none", tag: "fastNoThink" },
  { baseId: "gemini-2.5-flash", name: "Gemini 2.5 Flash", thinkingType: "none" },
  { baseId: "gemini-2.5-pro", name: "Gemini 2.5 Pro", thinkingType: "none" },
];

function parseGeminiProxyModelId(modelId: string): { baseId: string; tier: string; isCustom: boolean } {
  const trimmed = modelId.trim();
  if (!trimmed) {
    return { baseId: "gemini-3.7-flash", tier: "high", isCustom: false };
  }
  for (const f of GEMINI_PROXY_FAMILIES) {
    if (f.thinkingType === "flash3") {
      if (trimmed === `${f.baseId}-high`) return { baseId: f.baseId, tier: "high", isCustom: false };
      if (trimmed === `${f.baseId}-medium`) return { baseId: f.baseId, tier: "medium", isCustom: false };
      if (trimmed === `${f.baseId}-low`) return { baseId: f.baseId, tier: "low", isCustom: false };
      if (trimmed === f.baseId) return { baseId: f.baseId, tier: f.defaultTier || "medium", isCustom: false };
    } else if (f.thinkingType === "pro2") {
      if (trimmed === `${f.baseId}-high`) return { baseId: f.baseId, tier: "high", isCustom: false };
      if (trimmed === `${f.baseId}-low`) return { baseId: f.baseId, tier: "low", isCustom: false };
      if (trimmed === f.baseId) return { baseId: f.baseId, tier: f.defaultTier || "high", isCustom: false };
    } else if (trimmed === f.baseId) {
      return { baseId: f.baseId, tier: "", isCustom: false };
    }
  }
  return { baseId: "__custom__", tier: "", isCustom: true };
}

function composeGeminiProxyModelId(baseId: string, tier: string): string {
  const f = GEMINI_PROXY_FAMILIES.find((item) => item.baseId === baseId);
  if (!f) return baseId;
  if (f.thinkingType === "flash3") {
    return `${baseId}-${tier || f.defaultTier || "medium"}`;
  }
  if (f.thinkingType === "pro2") {
    return `${baseId}-${tier || f.defaultTier || "high"}`;
  }
  return baseId;
}

const DEFAULT_GEMINI_PROXY_MODEL_OPTIONS: GeminiModelOption[] = [
  { id: "gemini-3.7-flash-high", displayName: "", description: "Default reading model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.7-flash-medium", displayName: "", description: "Medium thinking", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.7-flash-low", displayName: "", description: "Low thinking", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.6-flash-medium", displayName: "", description: "Fast model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.5-flash-medium", displayName: "", description: "Fast model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.1-pro-high", displayName: "", description: "Deep reasoning model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.1-pro-low", displayName: "", description: "Pro model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-3.1-flash-lite", displayName: "", description: "Fast translation & auxiliary model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-2.5-flash", displayName: "", description: "Flash model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
  { id: "gemini-2.5-pro", displayName: "", description: "Pro model", supportsGenerateContent: true, supportsNativePdf: true, supportsInteractions: true },
];

const MODEL_OPTION_I18N: Record<string, string> = {
  "gemini-3.7-flash-high": "settings.models.option.gemini_3_7_flash_high",
  "gemini-3.7-flash-medium": "settings.models.option.gemini_3_7_flash_medium",
  "gemini-3.7-flash-low": "settings.models.option.gemini_3_7_flash_low",
  "gemini-3.6-flash-medium": "settings.models.option.gemini_3_6_flash_medium",
  "gemini-3.5-flash-medium": "settings.models.option.gemini_3_5_flash_medium",
  "gemini-3.1-pro-high": "settings.models.option.gemini_3_1_pro_high",
  "gemini-3.1-pro-low": "settings.models.option.gemini_3_1_pro_low",
  "gemini-3.1-flash-lite": "settings.models.option.gemini_3_1_flash_lite",
  "gemini-2.5-flash": "settings.models.option.gemini_2_5_flash",
  "gemini-2.5-pro": "settings.models.option.gemini_2_5_pro",
};

export const SettingsModelsPage: React.FC<SettingsModelsPageProps> = ({
  modelSettings,
  initialProviderId,
  focusNonce,
  onModelSettingsChange,
  setStatus,
}) => {
  const { t } = useLocale();
  function modelOptionLabel(opt: GeminiModelOption): string {
    const key = MODEL_OPTION_I18N[opt.id];
    return key ? t(key) : (opt.displayName || opt.id);
  }
  const providers = modelSettings.providers;
  const currentProviderId = modelSettings.currentProviderId;

  const [activeInstanceId, setActiveInstanceId] = useState<string>(() => {
    return currentProviderId || providers[0]?.id || "";
  });

  const lastAppliedFocusNonce = useRef<number | undefined>(undefined);

  // Sync activeInstanceId if providers change and current active is missing
  useEffect(() => {
    if (!providers.some((p) => p.id === activeInstanceId)) {
      setActiveInstanceId(currentProviderId || providers[0]?.id || "");
    }
  }, [providers, currentProviderId, activeInstanceId]);

  useEffect(() => {
    if (
      focusNonce === undefined ||
      focusNonce === lastAppliedFocusNonce.current
    ) {
      return;
    }

    lastAppliedFocusNonce.current = focusNonce;
    if (!initialProviderId) return;
    if (providers.some((provider) => provider.id === initialProviderId)) {
      setActiveInstanceId(initialProviderId);
      return;
    }
    setStatus(t("settings.models.focusMissing"));
  }, [focusNonce, initialProviderId, providers, setStatus]);

  const activeInstance = useMemo(
    () => providers.find((p) => p.id === activeInstanceId) ?? null,
    [providers, activeInstanceId],
  );

  // Form state
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [paperModel, setPaperModel] = useState("");
  const [translationModel, setTranslationModel] = useState("");
  const [kind, setKind] = useState<ProviderKind>("gemini");
  const [name, setName] = useState("");
  // Custom model draft handling (P0-02)
  const [paperModelMode, setPaperModelMode] = useState<"preset" | "custom">("preset");
  const [customPaperDraft, setCustomPaperDraft] = useState("");
  const [customPaperError, setCustomPaperError] = useState<string | null>(null);
  const customPaperDraftsByProvider = useRef<Record<string, string>>({});
  const [geminiProxyCustomDraft, setGeminiProxyCustomDraft] = useState("");
  const [geminiProxyIsCustom, setGeminiProxyIsCustom] = useState(false);

  // UI state
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isEditingName, setIsEditingName] = useState(false);
  const [nameInput, setNameInput] = useState("");
  const [draggedId, setDraggedId] = useState<string | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);

  // Mistral state
  const [mistralApiKey, setMistralApiKey] = useState("");
  const [mistralSaving, setMistralSaving] = useState(false);

  // Reset form when switching active instance
  useEffect(() => {
    if (activeInstance) {
      setApiKey("");
      setBaseUrl(
        activeInstance.baseUrl ??
          (activeInstance.kind === "grok"
            ? "https://api.x.ai/v1"
            : activeInstance.kind === "gemini_proxy"
            ? "http://localhost:8045/v1"
            : activeInstance.kind === "openai_compatible"
            ? "https://api.openai.com/v1"
            : ""),
      );
      setPaperModel(activeInstance.paperModel);
      setTranslationModel(activeInstance.translationModel);
      setKind(activeInstance.kind);
      setName(activeInstance.name);
      setNameInput(activeInstance.name);
      setIsEditingName(false);
      setTestResult(null);
      setTestError(null);
      // P0-02: initialize custom draft/mode per instance
      if (activeInstance.kind === "gemini_proxy") {
        const parsed = parseGeminiProxyModelId(activeInstance.paperModel);
        setGeminiProxyIsCustom(parsed.isCustom);
        setGeminiProxyCustomDraft(parsed.isCustom ? activeInstance.paperModel : "");
      } else {
        const knownModelOptions =
          activeInstance.models.length > 0
            ? activeInstance.models
            : activeInstance.kind === "gemini"
              ? DEFAULT_GEMINI_PROXY_MODEL_OPTIONS
              : [];
        const isKnownPreset = knownModelOptions.some(
          (option) => option.id === activeInstance.paperModel,
        );
        const nextMode =
          knownModelOptions.length > 0 && !isKnownPreset
            ? "custom"
            : "preset";
        setPaperModelMode(nextMode);
        setCustomPaperDraft(
          customPaperDraftsByProvider.current[activeInstance.id] ??
            activeInstance.paperModel,
        );
        setCustomPaperError(null);
      }
    }
  }, [activeInstance?.id]);

  // A fresh Provider probe may reveal that a persisted/custom ID is now a preset.
  useEffect(() => {
    if (
      !activeInstance ||
      activeInstance.kind === "gemini_proxy" ||
      !testResult?.models.length
    ) {
      return;
    }
    const effective =
      paperModelMode === "custom" ? customPaperDraft.trim() : paperModel.trim();
    if (testResult.models.some((option) => option.id === effective)) {
      setPaperModel(effective);
      setPaperModelMode("preset");
    } else if (effective) {
      setPaperModelMode("custom");
      setCustomPaperDraft(effective);
      customPaperDraftsByProvider.current[activeInstance.id] = effective;
    }
  }, [activeInstance, customPaperDraft, paperModel, paperModelMode, testResult]);

  const handleAddProvider = async (newKind: ProviderKind = "gemini") => {
    if (providers.length >= 10) {
      setStatus(t("settings.models.maxProviders"));
      return;
    }
    try {
      const defaultName = defaultNameForKind(newKind, providers.filter((p) => p.kind === newKind).length, t);
      const next = await desktopClient.command<ModelSettings>("add_provider", {
        request: { name: defaultName, kind: newKind },
      });
      const added = next.providers[next.providers.length - 1];
      if (added) {
        setActiveInstanceId(added.id);
      }
      onModelSettingsChange(next);
      setStatus(t("settings.models.created", { name: defaultName }));
    } catch (e) {
      setStatus(t("settings.models.addFailed", { error: String(e) }));
    }
  };

  const handleSetCurrent = async (id: string) => {
    try {
      const next = await desktopClient.command<ModelSettings>("set_current_provider", {
        request: { id },
      });
      onModelSettingsChange(next);
      const inst = next.providers.find((p) => p.id === id);
      setStatus(t("settings.models.setCurrent", { name: inst?.name ?? id }));
    } catch (e) {
      setStatus(t("settings.models.setCurrentFailed", { error: String(e) }));
    }
  };

  const handleRename = async () => {
    if (!activeInstance || !nameInput.trim()) {
      setIsEditingName(false);
      return;
    }
    try {
      const next = await desktopClient.command<ModelSettings>("rename_provider", {
        request: { id: activeInstance.id, name: nameInput.trim() },
      });
      onModelSettingsChange(next);
      setName(nameInput.trim());
      setIsEditingName(false);
      setStatus(t("settings.models.renamed", { name: nameInput.trim() }));
    } catch (e) {
      setStatus(t("settings.models.renameFailed", { error: String(e) }));
    }
  };

  const handleDuplicate = async (id: string) => {
    if (providers.length >= 10) {
      setStatus(t("settings.models.maxProviders"));
      return;
    }
    try {
      let next = await desktopClient.command<ModelSettings>("duplicate_provider", {
        request: { id },
      });
      const dup = next.providers[next.providers.length - 1];
      if (dup) {
        setActiveInstanceId(dup.id);
        next = await desktopClient.command<ModelSettings>("set_current_provider", {
          request: { id: dup.id },
        });
      }
      onModelSettingsChange(next);
      setStatus(t("settings.models.duplicated"));
    } catch (e) {
      setStatus(t("settings.models.duplicateFailed", { error: String(e) }));
    }
  };

  const handleRemove = async (id: string) => {
    setDeleteConfirmId(null);
    try {
      const next = await desktopClient.command<ModelSettings>("remove_provider", {
        request: { id },
      });
      onModelSettingsChange(next);
      if (activeInstanceId === id) {
        setActiveInstanceId(next.currentProviderId || next.providers[0]?.id || "");
      }
      setStatus(t("settings.models.deleted"));
    } catch (e) {
      setStatus(t("settings.models.deleteFailed", { error: String(e) }));
    }
  };

  const handleDragStart = (id: string) => {
    setDraggedId(id);
  };

  const handleDragOver = (e: React.DragEvent, targetId: string) => {
    e.preventDefault();
  };

  const handleDrop = async (targetId: string) => {
    if (!draggedId || draggedId === targetId) {
      setDraggedId(null);
      return;
    }
    const currentIds = providers.map((p) => p.id);
    const dragIdx = currentIds.indexOf(draggedId);
    const dropIdx = currentIds.indexOf(targetId);
    if (dragIdx === -1 || dropIdx === -1) return;

    const newIds = [...currentIds];
    newIds.splice(dragIdx, 1);
    newIds.splice(dropIdx, 0, draggedId);
    setDraggedId(null);

    try {
      const next = await desktopClient.command<ModelSettings>("reorder_providers", {
        request: { ids: newIds },
      });
      onModelSettingsChange(next);
    } catch (e) {
      setStatus(t("settings.models.reorderFailed", { error: String(e) }));
    }
  };

  const effectivePaperModelForSubmit = (): string => {
    if (kind === "gemini_proxy") {
      return geminiProxyIsCustom ? geminiProxyCustomDraft.trim() : paperModel.trim();
    }
    if (paperModelMode === "custom") {
      return customPaperDraft.trim();
    }
    return paperModel.trim();
  };

  const validateCustomModel = (value: string): string | null => {
    const trimmed = value.trim();
    if (!trimmed) return t("settings.models.modelIdEmpty");
    if (trimmed.length > 120) return t("settings.models.modelIdTooLong");
    if (/[\x00-\x1F\x7F]/.test(trimmed)) return t("settings.models.modelIdIllegal");
    return null;
  };

  const handleTestConnection = async () => {
    if (!activeInstance) return;
    const eff = effectivePaperModelForSubmit();
    // Validate custom ID on the client before probing
    if ((kind === "gemini_proxy" && geminiProxyIsCustom) || paperModelMode === "custom") {
      const err = validateCustomModel(eff);
      if (err) {
        setCustomPaperError(err);
        setStatus(err);
        return;
      }
    }
    setIsTesting(true);
    setTestError(null);
    setTestResult(null);
    try {
      const result = await desktopClient.command<ConnectionTestResult>(
        "test_provider_connection",
        {
          request: {
            id: activeInstance.id,
            kind,
            apiKey: apiKey.trim() || null,
            baseUrl: kind === "gemini" ? null : baseUrl.trim() || null,
            paperModel: eff || null,
          },
        },
      );
      if (result.models.length > 0 && !paperModel) {
        setPaperModel(result.models[0].id);
      }
      if (desktopClient.runtime === "memory") {
        setTestResult(result);
        setStatus(t("settings.models.browserPreviewNoProbe"));
        return;
      }
      if (result.paperProbePassed === false && result.paperProbeError) {
        setTestResult(null);
        setTestError(t("settings.models.probeFailed", { error: result.paperProbeError }));
        setStatus(t("settings.models.probeFailed", { error: result.paperProbeError }));
      } else {
        setTestResult(result);
        setStatus(t("settings.models.testSuccess"));
      }
    } catch (e) {
      setTestError(String(e));
      setStatus(t("settings.models.testFailed", { error: String(e) }));
    } finally {
      setIsTesting(false);
    }
  };

  const handleSaveSettings = async () => {
    if (!activeInstance) return;
    const eff = effectivePaperModelForSubmit();
    if ((kind === "gemini_proxy" && geminiProxyIsCustom) || paperModelMode === "custom") {
      const err = validateCustomModel(eff);
      if (err) {
        setCustomPaperError(err);
        setStatus(err);
        return;
      }
    }
    if (!eff) {
      setStatus(t("settings.models.modelIdEmpty"));
      if (paperModelMode === "custom" || geminiProxyIsCustom) {
        setCustomPaperError(t("settings.models.modelIdEmpty"));
      }
      return;
    }
    setIsSaving(true);
    try {
      let next = await desktopClient.command<ModelSettings>("save_provider_settings", {
        request: {
          id: activeInstance.id,
          kind,
          apiKey: apiKey.trim() || null,
          baseUrl: kind === "gemini" ? null : baseUrl.trim() || null,
          paperModel: eff,
          translationModel: translationModel.trim(),
        },
      });
      onModelSettingsChange(next);
      setApiKey("");
      // sync local paperModel to saved effective value and keep draft
      setPaperModel(eff);
      if (kind === "gemini_proxy" && geminiProxyIsCustom) {
        setGeminiProxyCustomDraft(eff);
      }
      if (paperModelMode === "custom") {
        setCustomPaperDraft(eff);
      }
      setCustomPaperError(null);
      setStatus(t("settings.models.saved", { name: activeInstance.name }));
    } catch (e) {
      setStatus(t("settings.models.saveFailed", { error: String(e) }));
    } finally {
      setIsSaving(false);
    }
  };

  const handleClearCredential = async () => {
    if (!activeInstance) return;
    try {
      const next = await desktopClient.command<ModelSettings>(
        "clear_provider_credential",
        {
          request: { id: activeInstance.id },
        },
      );
      onModelSettingsChange(next);
      setApiKey("");
      setTestResult(null);
      setStatus(t("settings.models.credentialCleared"));
    } catch (e) {
      setStatus(t("settings.models.clearCredentialFailed", { error: String(e) }));
    }
  };

  const handleSaveMistral = async () => {
    if (!mistralApiKey.trim()) return;
    setMistralSaving(true);
    try {
      const next = await desktopClient.command<ModelSettings>(
        "save_mistral_credential",
        { apiKey: mistralApiKey.trim() },
      );
      onModelSettingsChange(next);
      setMistralApiKey("");
      setStatus(t("settings.models.mistralSaved"));
    } catch (e) {
      setStatus(t("settings.models.mistralSaveFailed", { error: String(e) }));
    } finally {
      setMistralSaving(false);
    }
  };

  const handleClearMistral = async () => {
    setMistralSaving(true);
    try {
      const next = await desktopClient.command<ModelSettings>("clear_mistral_credential");
      onModelSettingsChange(next);
      setMistralApiKey("");
      setStatus(t("settings.models.mistralCleared"));
    } catch (e) {
      setStatus(t("settings.models.mistralClearFailed", { error: String(e) }));
    } finally {
      setMistralSaving(false);
    }
  };

  const modelOptions = useMemo(() => {
    if (testResult?.models && testResult.models.length > 0) {
      return testResult.models;
    }
    if (activeInstance?.models && activeInstance.models.length > 0) {
      return activeInstance.models;
    }
    if (kind === "gemini_proxy" || kind === "gemini") {
      return DEFAULT_GEMINI_PROXY_MODEL_OPTIONS;
    }
    return [];
  }, [testResult, activeInstance?.models, kind]);

  return (
    <div className="settings-models-page">
      {/* 1. Horizontal Provider Instances Tab Rail */}
      <div className="provider-tabs-header">
        <div className="provider-tabs-scroll" role="tablist">
          {providers.map((inst) => {
            const isSelected = inst.id === activeInstanceId;
            const isCurrent = inst.id === currentProviderId;
            const isReady = inst.credentialConfigured && (inst.kind === "gemini" ? inst.connectionVerifiedAt : inst.paperProbePassed);

            return (
              <div
                key={inst.id}
                role="tab"
                aria-selected={isSelected}
                draggable
                onDragStart={() => handleDragStart(inst.id)}
                onDragOver={(e) => handleDragOver(e, inst.id)}
                onDrop={() => handleDrop(inst.id)}
                className={`provider-tab-pill ${isSelected ? "active" : ""} ${isCurrent ? "is-current" : ""}`}
                onClick={() => setActiveInstanceId(inst.id)}
                title={t("settings.models.tabTitle", { name: inst.name, kind: paperProviderLabel(inst.kind) })}
              >
                <span
                  className={`status-dot ${isReady ? "dot-green" : inst.credentialConfigured ? "dot-yellow" : "dot-gray"}`}
                  title={isReady ? t("settings.models.dotReady") : inst.credentialConfigured ? t("settings.models.dotPending") : t("settings.models.dotEmpty")}
                />
                <span className="tab-name">{inst.name}</span>
                <span className="tab-kind-tag">
                  {inst.kind === "gemini_proxy"
                    ? "Gemini Proxy"
                    : inst.kind === "openai_compatible"
                    ? "OpenAI"
                    : inst.kind === "grok"
                    ? "Grok"
                    : "Gemini"}
                </span>
                {isCurrent && <span className="tab-current-badge">{t("settings.models.currentBadge")}</span>}
              </div>
            );
          })}
        </div>

        {/* Add Provider Button with dropdown menu */}
        {/* Add Provider Button */}
        <button
          type="button"
          className="btn-add-provider"
          onClick={() => void handleAddProvider()}
          disabled={providers.length >= 10}
          title={providers.length >= 10 ? t("settings.models.addLimit") : t("settings.models.addTitle")}
        >
          <span>{t("settings.models.addProvider")}</span>
        </button>
      </div>

      {/* 2. Provider Instance Form Body */}
      {providers.length === 0 ? (
        <div className="provider-empty-state">
          <div className="empty-icon">✦</div>
          <h3>{t("settings.models.emptyTitle")}</h3>
          <p>{t("settings.models.emptyBody")}</p>
          <div className="empty-actions">
            <button
              type="button"
              className="btn-primary"
              onClick={() => void handleAddProvider()}
            >
              {t("settings.models.addProvider")}
            </button>
          </div>
        </div>
      ) : activeInstance ? (
        <div className="provider-form-card">
          {/* Header row: Name + Current Toggle + Duplicate + Delete */}
          <div className="provider-instance-header">
            <div className="instance-title-area">
              {isEditingName ? (
                <div className="instance-name-edit">
                  <input
                    type="text"
                    value={nameInput}
                    onChange={(e) => setNameInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleRename();
                      if (e.key === "Escape") setIsEditingName(false);
                    }}
                    autoFocus
                    className="input-text name-inline-input"
                  />
                  <button type="button" className="btn-sm btn-primary" onClick={() => void handleRename()}>
                    {t("settings.models.ok")}
                  </button>
                  <button type="button" className="btn-sm" onClick={() => setIsEditingName(false)}>
                    {t("settings.models.cancel")}
                  </button>
                </div>
              ) : (
                <div className="instance-name-display" onClick={() => setIsEditingName(true)}>
                  <h2>{activeInstance.name}</h2>
                  <button
                    type="button"
                    className="btn-icon-text"
                    onClick={(e) => {
                      e.stopPropagation();
                      setIsEditingName(true);
                    }}
                    title={t("settings.models.renameTitle")}
                  >
                    ✎
                  </button>
                  <span className="instance-kind-pill">{paperProviderLabel(activeInstance.kind)}</span>
                </div>
              )}
            </div>

            <div className="instance-header-actions">
              {activeInstance.isCurrent ? (
                <span className="current-active-tag">{t("settings.models.inUse")}</span>
              ) : (
                <button
                  type="button"
                  className="btn-sm btn-outline-primary"
                  onClick={() => void handleSetCurrent(activeInstance.id)}
                >
                  {t("settings.models.setAsCurrent")}
                </button>
              )}
              <button
                type="button"
                className="btn-sm btn-ghost"
                onClick={() => void handleDuplicate(activeInstance.id)}
                title={t("settings.models.duplicateTitle")}
              >
                {t("settings.models.duplicate")}
              </button>
              {deleteConfirmId === activeInstance.id ? (
                <div className="delete-confirm-group">
                  <span className="delete-prompt">{t("settings.models.confirmDelete")}</span>
                  <button
                    type="button"
                    className="btn-sm btn-danger"
                    onClick={() => void handleRemove(activeInstance.id)}
                  >
                    {t("settings.models.delete")}
                  </button>
                  <button
                    type="button"
                    className="btn-sm btn-ghost"
                    onClick={() => setDeleteConfirmId(null)}
                  >
                    {t("settings.models.cancel")}
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  className="btn-sm btn-ghost-danger"
                  onClick={() => setDeleteConfirmId(activeInstance.id)}
                  title={t("settings.models.deleteTitle")}
                >
                  {t("settings.models.delete")}
                </button>
              )}
            </div>
          </div>

          {/* Dynamic Smart Form by Provider Kind */}
          <div className="provider-smart-form">
            {/* Field: Provider Kind Dropdown */}
            <div className="form-group-row">
              <label className="form-label">{t("settings.models.serviceType")}</label>
              <div className="form-control-wrap">
                <select
                  className="select-dropdown"
                  value={kind}
                  onChange={(e) => {
                    const nextKind = e.target.value as ProviderKind;
                    setKind(nextKind);
                    if (nextKind === "gemini") {
                      setPaperModel("gemini-2.5-flash");
                      setTranslationModel("gemini-2.5-flash-lite");
                    } else if (nextKind === "gemini_proxy") {
                      if (!baseUrl) setBaseUrl("http://localhost:8045/v1");
                      setPaperModel("gemini-3.7-flash-high");
                      setTranslationModel("gemini-3.1-flash-lite");
                    } else if (nextKind === "grok") {
                      setBaseUrl("https://api.x.ai/v1");
                    } else if (nextKind === "openai_compatible") {
                      if (!baseUrl) setBaseUrl("https://api.openai.com/v1");
                    }
                  }}
                >
                  <option value="gemini">{t("settings.models.kindGeminiOfficial")}</option>
                  <option value="gemini_proxy">{t("settings.models.kindGeminiProxy")}</option>
                  <option value="openai_compatible">{t("settings.models.kindOpenai")}</option>
                  <option value="grok">{t("settings.models.kindGrok")}</option>
                </select>
              </div>
            </div>

            {/* Field: Base URL (Hidden for Gemini, Pre-filled for Grok/GeminiProxy, Custom for OpenAI) */}
            {kind !== "gemini" && (
              <div className="form-group-row">
                <label className="form-label">{t("settings.models.baseUrl")}</label>
                <div className="form-control-wrap">
                  <input
                    type="text"
                    className="input-text"
                    value={baseUrl}
                    onChange={(e) => setBaseUrl(e.target.value)}
                    placeholder={
                      kind === "grok"
                        ? "https://api.x.ai/v1"
                        : kind === "gemini_proxy"
                        ? "http://localhost:8045/v1"
                        : "https://api.openai.com/v1"
                    }
                  />
                  <span className="field-hint">{t("settings.models.baseUrlHint")}</span>
                </div>
              </div>
            )}

            {/* Field: API Key */}
            <div className="form-group-row">
              <label className="form-label">{t("settings.models.apiKey")}</label>
              <div className="form-control-wrap">
                <div className="input-password-wrap">
                  <input
                    type="password"
                    className="input-text"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder={activeInstance.credentialConfigured ? t("settings.models.apiKeyStored") : t("settings.models.apiKeyPlaceholder")}
                  />
                  {activeInstance.credentialConfigured && (
                    <button
                      type="button"
                      className="btn-clear-cred"
                      onClick={() => void handleClearCredential()}
                      title={t("settings.models.clearKeyTitle")}
                    >
                      {t("settings.models.clearCredential")}
                    </button>
                  )}
                </div>
                <span className="field-hint">{t("settings.models.credentialHint")}</span>
                <ApiKeyHelp kind={kind} baseUrl={baseUrl} onError={setStatus} />
              </div>
            </div>

            {/* Field: Paper Model & Translation Model */}
            <div className="form-group-row">
              <label className="form-label">{t("settings.models.paperModel")}</label>
              <div className="form-control-wrap">
                {kind === "gemini_proxy" ? (() => {
                  const parsed = parseGeminiProxyModelId(geminiProxyIsCustom ? geminiProxyCustomDraft || paperModel : paperModel);
                  const family = GEMINI_PROXY_FAMILIES.find((f) => f.baseId === parsed.baseId);

                  return (
                    <div className="gemini-proxy-model-builder">
                      <select
                        className="select-dropdown"
                        value={geminiProxyIsCustom ? "__custom__" : parsed.baseId}
                        onChange={(e) => {
                          const val = e.target.value;
                          if (val === "__custom__") {
                            setGeminiProxyIsCustom(true);
                            if (!geminiProxyCustomDraft) setGeminiProxyCustomDraft(paperModel);
                          } else {
                            setGeminiProxyIsCustom(false);
                            const newFam = GEMINI_PROXY_FAMILIES.find((f) => f.baseId === val);
                            const tier = newFam?.thinkingType === "flash3"
                              ? (parsed.tier || "high")
                              : newFam?.thinkingType === "pro2"
                              ? (parsed.tier === "medium" ? "high" : parsed.tier || "high")
                              : "";
                            setPaperModel(composeGeminiProxyModelId(val, tier));
                          }
                        }}
                      >
                        {GEMINI_PROXY_FAMILIES.map((f) => (
                          <option key={f.baseId} value={f.baseId}>
                            {f.name} {f.tag ? `(${t(`settings.models.tag.${f.tag}`)})` : ""}
                          </option>
                        ))}
                        <option value="__custom__">{t("settings.models.customModelId")}</option>
                      </select>

                      {geminiProxyIsCustom ? (
                        <>
                          <input
                            type="text"
                            className="input-text mt-2"
                            placeholder={t("settings.models.fullModelIdPlaceholder")}
                            value={geminiProxyCustomDraft}
                            aria-invalid={customPaperError ? "true" : undefined}
                            aria-describedby={
                              customPaperError
                                ? "gemini-proxy-custom-model-error"
                                : undefined
                            }
                            onChange={(e) => {
                              setGeminiProxyCustomDraft(e.target.value);
                              if (customPaperError) setCustomPaperError(null);
                            }}
                            autoFocus
                          />
                          {customPaperError && (
                            <span
                              id="gemini-proxy-custom-model-error"
                              className="field-error"
                            >
                              {customPaperError}
                            </span>
                          )}
                        </>
                      ) : family?.thinkingType === "flash3" ? (
                        <div className="thinking-tier-wrap">
                          <span className="thinking-tier-label">{t("settings.models.thinkingTier")}</span>
                          <div className="thinking-tier-buttons">
                            <button
                              type="button"
                              className={`thinking-tier-btn ${parsed.tier === "low" ? "active" : ""}`}
                              onClick={() => setPaperModel(composeGeminiProxyModelId(parsed.baseId, "low"))}
                            >
                              {t("settings.models.tierLow")}
                            </button>
                            <button
                              type="button"
                              className={`thinking-tier-btn ${parsed.tier === "medium" ? "active" : ""}`}
                              onClick={() => setPaperModel(composeGeminiProxyModelId(parsed.baseId, "medium"))}
                            >
                              {t("settings.models.tierMedium")}
                            </button>
                            <button
                              type="button"
                              className={`thinking-tier-btn ${parsed.tier === "high" ? "active" : ""}`}
                              onClick={() => setPaperModel(composeGeminiProxyModelId(parsed.baseId, "high"))}
                            >
                              {t("settings.models.tierHighRecommended")}
                            </button>
                          </div>
                        </div>
                      ) : family?.thinkingType === "pro2" ? (
                        <div className="thinking-tier-wrap">
                          <span className="thinking-tier-label">{t("settings.models.thinkingTier")}</span>
                          <div className="thinking-tier-buttons">
                            <button
                              type="button"
                              className={`thinking-tier-btn ${parsed.tier === "low" ? "active" : ""}`}
                              onClick={() => setPaperModel(composeGeminiProxyModelId(parsed.baseId, "low"))}
                            >
                              {t("settings.models.tierLow")}
                            </button>
                            <button
                              type="button"
                              className={`thinking-tier-btn ${parsed.tier === "high" ? "active" : ""}`}
                              onClick={() => setPaperModel(composeGeminiProxyModelId(parsed.baseId, "high"))}
                            >
                              {t("settings.models.tierHighDeep")}
                            </button>
                          </div>
                        </div>
                      ) : (
                        <div className="thinking-tier-wrap">
                          <span className="field-hint">{t("settings.models.noThinking")}</span>
                        </div>
                      )}
                      <span className="field-hint">
                        {t("settings.models.callingModel")} <code className="model-id-badge">{(geminiProxyIsCustom ? geminiProxyCustomDraft : paperModel) || composeGeminiProxyModelId("gemini-3.7-flash", "high")}</code>
                      </span>
                    </div>
                  );
                })() : modelOptions.length > 0 ? (
                  <div className="select-with-custom">
                    <select
                      className="select-dropdown"
                      value={paperModelMode === "custom" ? "__custom__" : paperModel}
                      onChange={(e) => {
                        const val = e.target.value;
                        if (val === "__custom__") {
                          setPaperModelMode("custom");
                          // preserve draft, default to current paperModel if not custom
                          if (!customPaperDraft) setCustomPaperDraft(paperModel && paperModel !== "__custom__" ? paperModel : "");
                        } else {
                          setPaperModelMode("preset");
                          setPaperModel(val);
                          setCustomPaperError(null);
                        }
                      }}
                    >
                      {modelOptions.map((opt) => (
                        <option key={opt.id} value={opt.id}>
                          {modelOptionLabel(opt)} {opt.supportsNativePdf ? t("settings.models.nativePdf") : ""}
                        </option>
                      ))}
                      <option value="__custom__">{t("settings.models.customModelName")}</option>
                    </select>
                    {paperModelMode === "custom" && (
                      <>
                        <input
                          type="text"
                          className="input-text mt-2"
                          placeholder={t("settings.models.modelIdExamples")}
                          value={customPaperDraft}
                          aria-invalid={customPaperError ? "true" : undefined}
                          aria-describedby={
                            customPaperError
                              ? "provider-custom-model-error"
                              : undefined
                          }
                          onChange={(e) => {
                            const nextDraft = e.target.value;
                            setCustomPaperDraft(nextDraft);
                            customPaperDraftsByProvider.current[
                              activeInstance.id
                            ] = nextDraft;
                            if (customPaperError) setCustomPaperError(null);
                          }}
                          autoFocus
                        />
                        {customPaperError && (
                          <span
                            id="provider-custom-model-error"
                            className="field-error"
                          >
                            {customPaperError}
                          </span>
                        )}
                      </>
                    )}
                  </div>
                ) : (
                  <input
                    type="text"
                    className="input-text"
                    value={paperModel}
                    onChange={(e) => setPaperModel(e.target.value)}
                    placeholder={kind === "gemini" ? "gemini-2.5-flash" : t("settings.models.modelIdGptExamples")}
                  />
                )}
                <span className="field-hint">{t("settings.models.paperHint")}</span>
              </div>
            </div>

            <div className="form-group-row">
              <label className="form-label">{t("settings.models.translationModel")}</label>
              <div className="form-control-wrap">
                {kind === "gemini_proxy" ? (
                  <select
                    className="select-dropdown"
                    value={translationModel}
                    onChange={(e) => setTranslationModel(e.target.value)}
                  >
                    <option value="">{t("settings.models.followPrimary")}</option>
                    <option value="gemini-3.1-flash-lite">{t("settings.models.translation.flashLite")}</option>
                    <option value="gemini-3.7-flash-low">{t("settings.models.translation.flash37Low")}</option>
                    <option value="gemini-3.6-flash-low">{t("settings.models.translation.flash36Low")}</option>
                    <option value="gemini-3.5-flash-low">{t("settings.models.translation.flash35Low")}</option>
                    <option value="gemini-3.1-pro-low">{t("settings.models.translation.proLow")}</option>
                    <option value="gemini-2.5-flash-lite">Gemini 2.5 Flash Lite</option>
                  </select>
                ) : modelOptions.length > 0 ? (
                  <select
                    className="select-dropdown"
                    value={translationModel}
                    onChange={(e) => setTranslationModel(e.target.value)}
                  >
                    <option value="">{t("settings.models.followPrimary")}</option>
                    {modelOptions.map((opt) => (
                      <option key={opt.id} value={opt.id}>
                        {modelOptionLabel(opt)}
                      </option>
                    ))}
                  </select>
                ) : (
                  <input
                    type="text"
                    className="input-text"
                    value={translationModel}
                    onChange={(e) => setTranslationModel(e.target.value)}
                    placeholder={kind === "gemini" ? "gemini-2.5-flash-lite" : t("settings.models.auxModelPlaceholder")}
                  />
                )}
                <span className="field-hint">{t("settings.models.translationHint")}</span>
              </div>
            </div>

            {/* Test & Save Actions */}
            <div className="form-actions-row">
              <button
                type="button"
                className="btn-test-conn"
                onClick={() => void handleTestConnection()}
                disabled={isTesting}
              >
                {isTesting ? t("settings.models.testing") : t("settings.models.testConnection")}
              </button>

              <button
                type="button"
                className="btn-primary btn-save-provider"
                onClick={() => void handleSaveSettings()}
                disabled={isSaving}
              >
                {isSaving ? t("settings.models.saving") : t("settings.models.saveSettings")}
              </button>
            </div>

            {/* Test Result / Feedback Alert */}
            {testResult && (
              <div
                className={
                  desktopClient.runtime === "memory"
                    ? "test-feedback-box"
                    : "test-feedback-box success"
                }
              >
                <div className="feedback-title">
                  {desktopClient.runtime === "memory"
                    ? t("settings.models.browserUnverified")
                    : t("settings.models.connectionOk")}
                </div>
                <div className="feedback-detail">
                  {desktopClient.runtime === "memory" ? (
                    <>{t("settings.models.browserStaticCatalog")}</>
                  ) : (
                    <>
                      {t("settings.models.modelsFetched", { count: testResult.models.length })}
                      {testResult.paperProbePassed &&
                        t("settings.models.probePassed")}
                    </>
                  )}
                </div>
              </div>
            )}
            {testError && (
              <div className="test-feedback-box error">
                <div className="feedback-title">{t("settings.models.testFailedTitle")}</div>
                <div className="feedback-detail">{testError}</div>
              </div>
            )}
          </div>
        </div>
      ) : null}

      {/* 3. Mistral OCR Independent Section */}
      <div className="ocr-provider-card">
        <div className="ocr-card-header">
          <div className="ocr-title-area">
            <h3>{t("settings.models.mistralTitle")}</h3>
            <span className={`ocr-status-badge ${modelSettings.mistralCredentialConfigured ? "configured" : "unconfigured"}`}>
              {modelSettings.mistralCredentialConfigured ? t("settings.models.configured") : t("settings.models.unconfigured")}
            </span>
          </div>
          <span className="ocr-model-tag">mistral-ocr-latest</span>
        </div>
        <ApiKeyHelp kind="mistral" onError={setStatus} />

        <div className="ocr-form-row">
          <div className="input-password-wrap">
            <input
              type="password"
              className="input-text"
              value={mistralApiKey}
              onChange={(e) => setMistralApiKey(e.target.value)}
              placeholder={modelSettings.mistralCredentialConfigured ? t("settings.models.apiKeyStored") : t("settings.models.mistralKeyPlaceholder")}
            />
          </div>

          <div className="ocr-actions">
            <button
              type="button"
              className="btn-primary"
              onClick={() => void handleSaveMistral()}
              disabled={mistralSaving || !mistralApiKey.trim()}
            >
              {mistralSaving ? t("settings.models.savingShort") : t("settings.models.saveKey")}
            </button>
            {modelSettings.mistralCredentialConfigured && (
              <button
                type="button"
                className="btn-ghost-danger"
                onClick={() => void handleClearMistral()}
                disabled={mistralSaving}
              >
                {t("settings.models.clearKey")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
