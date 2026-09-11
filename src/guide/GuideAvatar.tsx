import { useState, useEffect } from "react";
import type { ResolvedGuidePersona } from "./personas";

export default function GuideAvatar({
  persona,
  size = 18,
  className = "",
}: {
  persona: ResolvedGuidePersona | null;
  size?: number;
  className?: string;
}) {
  const [loadFailed, setLoadFailed] = useState(false);
  const color = persona?.color ?? "#888888";
  const label = (persona?.displayName ?? "?").slice(0, 1);

  useEffect(() => {
    setLoadFailed(false);
  }, [persona?.avatarSrc]);

  const customStyle: React.CSSProperties = {
    width: size,
    height: size,
    minWidth: size,
    minHeight: size,
    flex: `0 0 ${size}px`,
    fontSize: `${Math.max(10, Math.round(size * 0.44))}px`,
  };

  if (persona?.avatarSrc && !loadFailed) {
    return (
      <img
        className={`guide-avatar ${className}`.trim()}
        src={persona.avatarSrc}
        alt={persona.displayName || ""}
        width={size}
        height={size}
        style={customStyle}
        onError={() => setLoadFailed(true)}
      />
    );
  }
  return (
    <span
      className={`guide-avatar is-fallback ${className}`.trim()}
      style={{ ...customStyle, background: color }}
    >
      {label}
    </span>
  );
}
