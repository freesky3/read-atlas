import { describe, expect, it } from "vitest";
import {
  cssPointFromPhysical,
  decideImportDrop,
  importDestination,
  importDestinationFromHit,
  importHitFromElement,
  MAX_IMPORT_SOURCES,
} from "./importDrop";

describe("importDrop", () => {
  it("uses the physical folder, Inbox for a root, and refuses smart collections", () => {
    expect(importDestination("Papers/ML")).toBe("Papers/ML");
    expect(importDestination("Papers")).toBe("Papers/Inbox");
    expect(importDestination("smart:reading")).toBeNull();
    expect(importDestination("")).toBeNull();
  });

  it("prefers the hovered physical folder and refuses smart / 全部", () => {
    expect(importDestinationFromHit({ folder: "Textbooks/ML", smart: false, overList: false }, "Papers/Inbox")).toBe("Textbooks/ML");
    expect(importDestinationFromHit({ folder: "Papers", smart: false, overList: false }, "Papers/ML")).toBe("Papers/Inbox");
    expect(importDestinationFromHit({ folder: "smart:reading", smart: true, overList: false }, "Papers/ML")).toBeNull();
    expect(importDestinationFromHit({ folder: "", smart: false, overList: false }, "Papers/ML")).toBeNull();
    expect(importDestinationFromHit({ folder: null, smart: false, overList: true }, "Papers/ML")).toBe("Papers/ML");
    expect(importDestinationFromHit(null, "Papers/ML")).toBe("Papers/ML");
  });

  it("reads data-folder / data-smart from the element under the pointer", () => {
    const smart = document.createElement("div");
    smart.dataset.folder = "smart:reading";
    smart.dataset.smart = "true";
    const child = document.createElement("span");
    smart.appendChild(child);
    expect(importHitFromElement(child)).toEqual({ folder: "smart:reading", smart: true, overList: false });

    const list = document.createElement("div");
    list.className = "hub-paper-list";
    const card = document.createElement("div");
    list.appendChild(card);
    expect(importHitFromElement(card)).toEqual({ folder: null, smart: false, overList: true });
  });

  it("converts physical drop coordinates to CSS pixels", () => {
    expect(cssPointFromPhysical({ x: 200, y: 100 }, 2)).toEqual({ x: 100, y: 50 });
    expect(cssPointFromPhysical(null, 2)).toBeNull();
  });

  it("shows overlay on enter and keeps non-PDF paths as batch items", () => {
    expect(decideImportDrop({ type: "enter" }, "Papers/Inbox")).toEqual({
      action: "overlay",
      message: "Papers/Inbox",
    });
    expect(decideImportDrop({ type: "leave" }, "Papers/Inbox")).toEqual({ action: "clear" });
    expect(
      decideImportDrop({ type: "drop", paths: ["C:\\a.pdf", "C:\\notes.txt", "C:\\chapters"] }, "Papers/Inbox"),
    ).toEqual({
      action: "import",
      paths: ["C:\\a.pdf", "C:\\notes.txt", "C:\\chapters"],
      collection: "Papers/Inbox",
    });
  });

  it("does not guess a destination and caps a drop at 500 sources including non-PDFs", () => {
    expect(decideImportDrop({ type: "drop", paths: ["a.pdf"] }, null).action).toBe("hint");
    const paths = Array.from({ length: MAX_IMPORT_SOURCES + 1 }, (_, index) => `${index}.txt`);
    const decision = decideImportDrop({ type: "drop", paths }, "Papers/Inbox");
    expect(decision.action).toBe("hint");
    if (decision.action === "hint") expect(decision.message).toContain("500");
  });
});
