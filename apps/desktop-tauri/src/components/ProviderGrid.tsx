import type { CSSProperties } from "react";
import type { ProviderUsageSnapshot } from "../types/bridge";
import { ProviderIcon } from "./providers/ProviderIcon";
import { getProviderIcon } from "./providers/providerIcons";

export default function ProviderGrid({
  providers,
  selectedProviderId,
  showAsUsed,
  onSelect,
}: {
  providers: ProviderUsageSnapshot[];
  selectedProviderId: string | null;
  showAsUsed: boolean;
  onSelect: (providerId: string | null) => void;
}) {
  const gridPercent = (provider: ProviderUsageSnapshot) => {
    const pct = showAsUsed
      ? provider.primary.usedPercent
      : provider.primary.remainingPercent;
    return Math.max(0, Math.min(100, pct));
  };
  const totalItems = providers.length + 1;
  const densityClass =
    totalItems <= 6
      ? " provider-grid--sparse"
      : totalItems > 32
        ? " provider-grid--compact"
        : "";
  const labelFor = (name: string) =>
    densityClass.includes("compact") ? compactGridLabel(name) : name;

  return (
    <div
      className={`provider-grid${densityClass}`}
      data-provider-count={totalItems}
    >
      <button
        type="button"
        className={`provider-grid__item${selectedProviderId === null ? " provider-grid__item--active" : ""}`}
        onClick={() => onSelect(null)}
        title="Overview"
        aria-label="All providers"
      >
        <span className="provider-grid__icon-overview">⊞</span>
        <span className="provider-grid__label">All</span>
      </button>
      {providers.map((p) => (
        <button
          key={p.providerId}
          type="button"
          className={`provider-grid__item${p.providerId === selectedProviderId ? " provider-grid__item--active" : ""}`}
          onClick={() => onSelect(p.providerId)}
          title={p.displayName}
          aria-label={p.displayName}
        >
          <ProviderIcon providerId={p.providerId} size={16} />
          <span className="provider-grid__label">{labelFor(p.displayName)}</span>
          {!p.error && (
            <span
              className="provider-grid__weekly-track"
              style={{
                "--weekly-pct": `${gridPercent(p)}%`,
                "--weekly-color": getProviderIcon(p.providerId).brandColor,
              } as CSSProperties}
            />
          )}
        </button>
      ))}
    </div>
  );
}

function compactGridLabel(displayName: string): string {
  const clean = displayName.replace(/[._-]+/g, " ").replace(/\s+/g, " ").trim();
  if (clean.length <= 5) return clean;

  const words = clean.split(" ").filter(Boolean);
  const first = words[0] ?? clean;
  if (words.length > 1) {
    if (first.length <= 3 && /\d|^[A-Z]+$/.test(first)) return first;
    const initials = words
      .slice(0, 2)
      .map((word) => word[0]?.toUpperCase() ?? "")
      .join("");
    if (initials.length >= 2) return initials;
  }

  const capitals = clean.match(/[A-Z0-9]/g);
  if (capitals && capitals.length >= 2 && capitals.length <= 4) {
    return capitals.join("");
  }

  return clean.slice(0, 4);
}
