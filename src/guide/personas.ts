import { convertFileSrc } from "@tauri-apps/api/core";
import type { GuideCastMember, GuideCastSnapshot } from "../types";

export const GUIDE_PERSONAS = [
  { id: "alin", displayName: "阿林", color: "#2458a8" },
  { id: "laozhou", displayName: "老周", color: "#b42318" },
  { id: "xiaxia", displayName: "小夏", color: "#9a6700" },
] as const;

export type GuidePersonaId = (typeof GUIDE_PERSONAS)[number]["id"];

export type ResolvedGuidePersona = {
  id: string;
  displayName: string;
  color: string;
  avatarSrc?: string | null;
};

export function resolveWorkspaceAssetSrc(
  relativePath: string | null | undefined,
  workspaceRoot?: string | null,
): string | null {
  if (!relativePath) return null;
  if (/^(data:|asset:|https?:|blob:)/i.test(relativePath)) return relativePath;
  if (!workspaceRoot) return null;
  const abs = `${workspaceRoot.replace(/[/\\]+$/, "")}\\${relativePath.replace(/\//g, "\\")}`;
  try {
    return convertFileSrc(abs);
  } catch {
    return null;
  }
}

export function v1HistoricalPersona(id: string): ResolvedGuidePersona | null {
  return GUIDE_PERSONAS.find((persona) => persona.id === id) ?? null;
}

export function guidePersona(
  id: string,
  snapshot?: GuideCastSnapshot | null,
  workspaceRoot?: string | null,
): ResolvedGuidePersona | null {
  const member = snapshot?.characters.find((item) => item.id === id);
  if (member) {
    return memberToPersona(member, workspaceRoot);
  }
  return v1HistoricalPersona(id);
}

export function memberToPersona(
  member: GuideCastMember,
  workspaceRoot?: string | null,
): ResolvedGuidePersona {
  return {
    id: member.id,
    displayName: member.displayName,
    color: member.inkColor,
    avatarSrc: resolveWorkspaceAssetSrc(member.workspaceAvatarPath, workspaceRoot),
  };
}
