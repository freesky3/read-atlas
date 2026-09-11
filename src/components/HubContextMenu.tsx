import { useEffect, useRef, useState } from "react";

export type MenuItem = {
  label: string;
  shortcut?: string;
  danger?: boolean;
  /** Non-empty greys the item out; shares the same DropEvaluation reason as drag targets. */
  disabledReason?: string;
  onClick: () => void;
};

export default function HubContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    let nx = x;
    let ny = y;
    if (x + rect.width > window.innerWidth - 8) nx = window.innerWidth - rect.width - 8;
    if (y + rect.height > window.innerHeight - 8) ny = window.innerHeight - rect.height - 8;
    if (nx !== x || ny !== y) setPos({ x: nx, y: ny });
  }, [x, y]);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    const onScroll = () => onClose();
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      className="hub-context-menu"
      style={{ left: pos.x, top: pos.y }}
      role="menu"
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it, idx) => (
        <button
          key={idx}
          role="menuitem"
          className={`hub-context-item ${it.danger ? "hub-context-danger" : ""} ${it.disabledReason ? "hub-context-disabled" : ""}`}
          disabled={Boolean(it.disabledReason)}
          title={it.disabledReason}
          aria-disabled={Boolean(it.disabledReason)}
          onClick={() => { if (it.disabledReason) return; onClose(); it.onClick(); }}
        >
          <span>{it.label}</span>
          {it.shortcut ? <span className="hub-context-shortcut">{it.shortcut}</span> : null}
        </button>
      ))}
    </div>
  );
}
