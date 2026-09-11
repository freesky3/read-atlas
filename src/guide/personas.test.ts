import { describe, expect, it } from "vitest";
import { GUIDE_PERSONAS, guidePersona, resolveWorkspaceAssetSrc } from "./personas";

describe("guide personas", () => {
  it("freezes the three default friends", () => {
    expect(GUIDE_PERSONAS.map((persona) => persona.id)).toEqual([
      "alin",
      "laozhou",
      "xiaxia",
    ]);
    expect(guidePersona("alin")?.displayName).toBe("阿林");
    expect(guidePersona("unknown")).toBeNull();
  });

  it("never maps historical V1 ids through a new cast snapshot", () => {
    const snapshot = {
      schemaVersion: 1,
      characters: [
        {
          id: "preset:chitanda",
          revision: 1,
          displayName: "千反田爱瑠",
          inkColor: "#7653A6",
          workspaceAvatarPath: ".read-desktop/guide-avatars/x.png",
        },
      ],
      order: ["preset:chitanda"],
      relationHints: [],
      rulesVersion: "guide-cast-rules-v1",
      castDigest: "x",
    };
    expect(guidePersona("alin", snapshot as never)?.displayName).toBe("阿林");
    expect(guidePersona("preset:chitanda", snapshot as never)?.displayName).toBe(
      "千反田爱瑠",
    );
  });

  it("leaves relative avatar paths unresolved without a workspace root", () => {
    expect(
      resolveWorkspaceAssetSrc(".read-desktop/guide-avatars/x.png", null),
    ).toBeNull();
  });
});
