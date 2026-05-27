import { useEffect, useState } from "react";
import type { LocaleKey } from "../../../../../i18n/keys";
import {
  getProviderWorkspaceId,
  setProviderWorkspaceId,
} from "../../../../../lib/tauri";

interface Props {
  providerId?: string;
  t: (key: LocaleKey) => string;
}

/**
 * OpenAI/Codex-specific detail help.
 *
 * Port of the help strings below the `ProviderId::Codex` toggles in
 * `rust/src/native_ui/preferences.rs::render_provider_detail_panel` (~6625).
 * The toggles themselves (`codex_historical_tracking`,
 * `codex_openai_web_extras`) are not yet persisted through
 * `update_settings` in the Tauri bridge, so this component shows the
 * upstream hint copy only. The toggles will be surfaced once they join
 * the SettingsUpdate bridge (tracked alongside Phase 6e token-accounts).
 */
export function OpenAiExtras({ providerId = "codex", t }: Props) {
  const [projectId, setProjectId] = useState("");
  const [savedProjectId, setSavedProjectId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (providerId !== "openaiapi") return;
    let cancelled = false;
    void getProviderWorkspaceId(providerId)
      .then((value) => {
        if (!cancelled) {
          setProjectId(value ?? "");
          setSavedProjectId(value ?? "");
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [providerId]);

  const saveProjectId = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = projectId.trim();
      await setProviderWorkspaceId(providerId, next);
      setSavedProjectId(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (providerId === "openaiapi") {
    return (
      <section className="provider-detail-section">
        <h4>OpenAI Admin API</h4>
        <label className="provider-detail-field">
          <span className="provider-detail-field__label">
            Project ID
          </span>
          <input
            className="provider-detail-field__input"
            value={projectId}
            placeholder="proj_..."
            spellCheck={false}
            onChange={(event) => setProjectId(event.target.value)}
          />
        </label>
        <div className="provider-detail-helper">
          Leave blank for organization-wide usage. Set a project ID to scope OpenAI usage and cost requests with the Admin API.
        </div>
        <div className="provider-detail-actions">
          <button
            type="button"
            className="credential-btn credential-btn--primary"
            disabled={busy || projectId.trim() === savedProjectId}
            onClick={saveProjectId}
          >
            Save
          </button>
        </div>
        {error && <div className="provider-detail-error">{error}</div>}
      </section>
    );
  }

  return (
    <section className="provider-detail-section">
      <h4>{t("CredentialsSectionTitle")}</h4>
      <div className="provider-detail-helper">
        {t("ProviderCodexHistoryHelp")}
      </div>
      <div className="provider-detail-helper">
        {t("CredsOpenAiHistoryHelp")}
      </div>
    </section>
  );
}
