import { useEffect, useRef, type RefObject } from "react";

const overlayStack: symbol[] = [];
const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function useDialogFocusTrap({
  open,
  containerRef,
  onClose,
  initialFocusRef,
}: {
  open: boolean;
  containerRef: RefObject<HTMLElement | null>;
  onClose: () => void;
  initialFocusRef?: RefObject<HTMLElement | null>;
}) {
  const tokenRef = useRef(Symbol("dialog"));
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    if (!open) return;
    const token = tokenRef.current;
    overlayStack.push(token);
    const previousFocus = document.activeElement;
    const container = containerRef.current;
    const inerted = container?.parentElement
      ? Array.from(container.parentElement.children)
          .filter((element) => element !== container)
          .map((element) => ({
            element: element as HTMLElement,
            wasInert: (element as HTMLElement).inert,
          }))
      : [];
    inerted.forEach(({ element }) => {
      element.inert = true;
    });

    const focusInitial = window.setTimeout(() => {
      const target =
        initialFocusRef?.current ??
        containerRef.current?.querySelector<HTMLElement>(FOCUSABLE);
      target?.focus();
    }, 0);

    const onKeyDown = (event: KeyboardEvent) => {
      if (overlayStack.at(-1) !== token) return;
      const currentContainer = containerRef.current;
      if (!currentContainer) return;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(
        currentContainer.querySelectorAll<HTMLElement>(FOCUSABLE),
      ).filter(
        (element) =>
          element.offsetParent !== null &&
          element.getAttribute("aria-hidden") !== "true",
      );
      if (focusable.length === 0) {
        event.preventDefault();
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      } else if (!currentContainer.contains(document.activeElement)) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.clearTimeout(focusInitial);
      document.removeEventListener("keydown", onKeyDown, true);
      const index = overlayStack.lastIndexOf(token);
      if (index >= 0) overlayStack.splice(index, 1);
      inerted.forEach(({ element, wasInert }) => {
        element.inert = wasInert;
      });
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) {
        previousFocus.focus();
      }
    };
  }, [containerRef, initialFocusRef, open]);
}
