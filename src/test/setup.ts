import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

afterEach(() => {
  cleanup();
});

if (typeof window.PointerEvent === "undefined") {
  class PointerEventPolyfill extends MouseEvent {
    pointerId: number;
    pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
      this.pointerType = init.pointerType ?? "";
    }
  }
  Object.defineProperty(window, "PointerEvent", {
    configurable: true,
    writable: true,
    value: PointerEventPolyfill,
  });
}

Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
  configurable: true,
  value: vi.fn(() => ({
    canvas: document.createElement("canvas"),
    save: vi.fn(),
    restore: vi.fn(),
    transform: vi.fn(),
    setTransform: vi.fn(),
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => ({ data: new Uint8ClampedArray() })),
    putImageData: vi.fn(),
    createImageData: vi.fn(),
  })),
});

if (typeof SVGElement !== "undefined") {
  const proto = SVGElement.prototype as SVGElement & {
    getBBox?: () => { x: number; y: number; width: number; height: number };
  };
  if (typeof proto.getBBox !== "function") {
    Object.defineProperty(proto, "getBBox", {
      configurable: true,
      value() {
        return { x: 0, y: 0, width: 40, height: 16 };
      },
    });
  }
}

// KaTeX htmlAndMathml injects <math> nodes. jsdom's getComputedStyle
// throws when a stylesheet's cssRules is null, which testing-library
// hits while computing accessible names.
const originalGetComputedStyle = window.getComputedStyle.bind(window);
window.getComputedStyle = ((
  elt: Element,
  pseudoElt?: string | null,
) => {
  try {
    return originalGetComputedStyle(elt, pseudoElt);
  } catch {
    return originalGetComputedStyle(document.documentElement);
  }
}) as typeof window.getComputedStyle;

if (typeof globalThis.ResizeObserver === "undefined") {
  class TestResizeObserver implements ResizeObserver {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  globalThis.ResizeObserver = TestResizeObserver;
}

if (typeof globalThis.DOMMatrix === "undefined") {
  class TestDOMMatrix {
    a = 1;
    b = 0;
    c = 0;
    d = 1;
    e = 0;
    f = 0;
    m11 = 1;
    m12 = 0;
    m13 = 0;
    m14 = 0;
    m21 = 0;
    m22 = 1;
    m23 = 0;
    m24 = 0;
    m31 = 0;
    m32 = 0;
    m33 = 1;
    m34 = 0;
    m41 = 0;
    m42 = 0;
    m43 = 0;
    m44 = 1;
    is2D = true;
    isIdentity = true;
    transformPoint(point?: { x?: number; y?: number }) {
      return { x: point?.x ?? 0, y: point?.y ?? 0, z: 0, w: 1 };
    }
  }
  Object.defineProperty(globalThis, "DOMMatrix", {
    configurable: true,
    writable: true,
    value: TestDOMMatrix,
  });
  if (typeof window !== "undefined") {
    Object.defineProperty(window, "DOMMatrix", {
      configurable: true,
      writable: true,
      value: TestDOMMatrix,
    });
  }
}

if (typeof window !== "undefined" && typeof window.HTMLElement.prototype.scrollIntoView !== "function") {
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
}


