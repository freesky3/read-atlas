import { describe, expect, it, vi } from "vitest";
import {
  applyPinchZoom,
  applyWheelZoom,
  clampReaderZoom,
  eventOverElement,
  fitReaderZoom,
  isPinchZoomWheel,
  normalizeReaderRotation,
  subscribeNativePinchZoom,
  wheelDeltaPixels,
} from "./readerZoom";

describe("readerZoom", () => {
  it("clamps the reader zoom range", () => {
    expect(clampReaderZoom(20)).toBe(60);
    expect(clampReaderZoom(240)).toBe(180);
    expect(clampReaderZoom(117.4)).toBe(117);
  });

  it("normalizes rotation and fits the real rotated viewport", () => {
    expect(normalizeReaderRotation(-90)).toBe(270);
    expect(normalizeReaderRotation(450)).toBe(90);
    expect(
      fitReaderZoom({
        mode: "width",
        containerWidth: 798,
        containerHeight: 900,
        pageWidth: 600,
        pageHeight: 800,
        horizontalChrome: 48,
      }),
    ).toBe(100);
    expect(
      fitReaderZoom({
        mode: "page",
        containerWidth: 1000,
        containerHeight: 548,
        pageWidth: 800,
        pageHeight: 400,
        horizontalChrome: 0,
        verticalChrome: 48,
      }),
    ).toBe(100);
  });

  it("treats only ctrl/meta wheel events as pinch zoom", () => {
    expect(isPinchZoomWheel({ ctrlKey: true, metaKey: false })).toBe(true);
    expect(isPinchZoomWheel({ ctrlKey: false, metaKey: true })).toBe(true);
    expect(isPinchZoomWheel({ ctrlKey: false, metaKey: false })).toBe(false);
  });

  it("normalizes line and page wheel deltas into pixels", () => {
    expect(wheelDeltaPixels({ deltaY: -40, deltaMode: 0 })).toBe(-40);
    expect(wheelDeltaPixels({ deltaY: -3, deltaMode: 1 })).toBe(-48);
    expect(wheelDeltaPixels({ deltaY: 1, deltaMode: 2 })).toBe(800);
  });

  it("accumulates sub-pixel pinch wheel deltas instead of rounding them away", () => {
    let zoom = 100;
    for (let index = 0; index < 40; index += 1) {
      zoom = applyWheelZoom(zoom, -0.4);
    }
    expect(zoom).toBeGreaterThan(116);
    expect(clampReaderZoom(zoom)).toBeGreaterThan(100);
  });

  it("maps two-pointer distance to the same zoom range", () => {
    expect(applyPinchZoom(100, 40, 80)).toBe(180);
    expect(applyPinchZoom(100, 80, 40)).toBe(60);
    expect(applyPinchZoom(100, 0, 40)).toBe(100);
  });

  it("treats a wheel whose target is the document as over the reader when the cursor is inside it", () => {
    const element = {
      contains: (_node: Node | null) => false,
      getBoundingClientRect: () => ({
        left: 20,
        top: 40,
        right: 420,
        bottom: 640,
      }),
    };
    expect(
      eventOverElement({ target: document.body, clientX: 120, clientY: 200 }, element),
    ).toBe(true);
    expect(
      eventOverElement({ target: document.body, clientX: 8, clientY: 8 }, element),
    ).toBe(false);
  });

  it("forwards native WebView2 pinch factors through the window event", () => {
    const onFactor = vi.fn();
    const stop = subscribeNativePinchZoom(onFactor);
    window.dispatchEvent(
      new CustomEvent("webview-pinch-zoom", { detail: 1.08 }),
    );
    expect(onFactor).toHaveBeenCalledWith(1.08);
    stop();
  });
});
