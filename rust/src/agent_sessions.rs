use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentSessionProvider {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentSessionSource {
    Cli,
    DesktopApp,
    Ide,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentSessionState {
    Active,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionWorkspace {
    pub cwd: Option<String>,
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionActivity {
    pub started_at: Option<DateTime<Utc>>,
    pub last_activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AgentSessionFocusTarget {
    Process { pid: u32 },
    Transcript { transcript_path: String },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub provider: AgentSessionProvider,
    pub source: AgentSessionSource,
    pub state: AgentSessionState,
    pub pid: Option<u32>,
    pub transcript_path: Option<String>,
    pub host: String,
    pub workspace: AgentSessionWorkspace,
    pub activity: AgentSessionActivity,
    pub focus_target: AgentSessionFocusTarget,
}

impl AgentSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        provider: AgentSessionProvider,
        source: AgentSessionSource,
        state: AgentSessionState,
        pid: Option<u32>,
        transcript_path: Option<String>,
        host: impl Into<String>,
        workspace: AgentSessionWorkspace,
        activity: AgentSessionActivity,
        focus_target: AgentSessionFocusTarget,
    ) -> Self {
        Self {
            id: id.into(),
            provider,
            source,
            state,
            pid,
            transcript_path,
            host: host.into(),
            workspace,
            activity,
            focus_target,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionHostResult {
    pub host: String,
    pub sessions: Vec<AgentSession>,
    pub error: Option<String>,
}

impl AgentSessionHostResult {
    pub fn success(host: impl Into<String>, sessions: Vec<AgentSession>) -> Self {
        Self {
            host: host.into(),
            sessions,
            error: None,
        }
    }

    pub fn failed(host: impl Into<String>, message: impl std::fmt::Display) -> Self {
        Self {
            host: host.into(),
            sessions: Vec::new(),
            error: Some(crate::logging::safe_error_message(message)),
        }
    }

    pub fn from_json(body: &str) -> Result<Self, String> {
        RemoteSessionFetcher::decode_host_result(body)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionScanConfig {
    pub active_window: Duration,
    pub file_only_window: Duration,
}

impl Default for SessionScanConfig {
    fn default() -> Self {
        Self {
            active_window: Duration::from_secs(120),
            file_only_window: Duration::from_secs(30 * 60),
        }
    }
}

impl SessionScanConfig {
    pub fn state(
        &self,
        last_activity_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        has_live_process: bool,
    ) -> AgentSessionState {
        match last_activity_at {
            Some(last_activity_at) => {
                let age = now.signed_duration_since(last_activity_at);
                let active_window = ChronoDuration::from_std(self.active_window)
                    .unwrap_or_else(|_| ChronoDuration::seconds(120));
                if age <= active_window {
                    AgentSessionState::Active
                } else {
                    AgentSessionState::Idle
                }
            }
            None if has_live_process => AgentSessionState::Active,
            None => AgentSessionState::Idle,
        }
    }

    pub fn file_only_session_allowed(
        &self,
        modified_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> bool {
        let age = now.signed_duration_since(modified_at);
        let file_window = ChronoDuration::from_std(self.file_only_window)
            .unwrap_or_else(|_| ChronoDuration::seconds(30 * 60));
        age <= file_window
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentProcessKind {
    Agent,
    Helper,
    AppServer,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProcessRecord {
    pub pid: u32,
    pub ppid: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub provider: Option<AgentSessionProvider>,
    pub source: AgentSessionSource,
    pub executable: String,
    pub kind: AgentProcessKind,
}

impl AgentProcessRecord {
    pub fn is_agent(&self) -> bool {
        self.kind == AgentProcessKind::Agent
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeTranscript {
    pub url: PathBuf,
    pub modified_at: DateTime<Utc>,
}

impl ClaudeTranscript {
    pub fn new(url: PathBuf, modified_at: DateTime<Utc>) -> Self {
        Self { url, modified_at }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRolloutMetadata {
    pub session_id: String,
    pub cwd: Option<String>,
    pub originator: Option<String>,
    pub source: Option<String>,
}

impl CodexRolloutMetadata {
    pub fn session_source(&self) -> AgentSessionSource {
        let value = [self.originator.as_deref(), self.source.as_deref()]
            .into_iter()
            .flatten()
            .map(|part| part.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ");

        if value.contains("desktop") || value.contains("app-server") {
            AgentSessionSource::DesktopApp
        } else if value.contains("ide")
            || value.contains("vscode")
            || value.contains("cursor")
            || value.contains("zed")
        {
            AgentSessionSource::Ide
        } else if value.contains("codex_exec") || value.contains("exec") || value.contains("cli") {
            AgentSessionSource::Cli
        } else {
            AgentSessionSource::Unknown
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum SessionFocusResult {
    Focused,
    Unsupported { message: String },
    Failed { message: String },
}

impl SessionFocusResult {
    pub fn focused() -> Self {
        Self::Focused
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: crate::logging::safe_error_message(message.into()),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed {
            message: crate::logging::safe_error_message(message.into()),
        }
    }
}

pub struct AgentPSOutputParser;
pub struct LSOFCWDOutputParser;
pub struct ClaudeSessionProjectMapper;
pub struct CodexRolloutFirstLineParser;
pub struct AgentSessionCorrelation;
pub struct RemoteSessionFetcher;
pub struct TailscaleStatusParser;

impl AgentPSOutputParser {
    pub fn parse(output: &str) -> Vec<AgentProcessRecord> {
        let mut seen_pids = HashSet::new();
        output
            .lines()
            .filter_map(|line| Self::parse_line(line, &mut seen_pids))
            .collect()
    }

    pub fn agent_processes(records: &[AgentProcessRecord]) -> Vec<AgentProcessRecord> {
        let mut seen = HashSet::new();
        records
            .iter()
            .filter(|record| record.is_agent())
            .filter_map(|record| {
                if seen.insert(record.pid) {
                    Some(record.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn provider(record: &AgentProcessRecord) -> Option<AgentSessionProvider> {
        record.provider
    }

    pub fn source(record: &AgentProcessRecord) -> AgentSessionSource {
        record.source
    }

    pub fn has_codex_app_server(records: &[AgentProcessRecord]) -> bool {
        records.iter().any(|record| {
            record.kind == AgentProcessKind::AppServer
                && record.provider == Some(AgentSessionProvider::Codex)
        })
    }

    fn parse_line(line: &str, seen_pids: &mut HashSet<u32>) -> Option<AgentProcessRecord> {
        let mut fields = line.split_whitespace();
        let pid = fields.next()?.parse::<u32>().ok()?;
        let ppid = fields.next()?.parse::<u32>().ok()?;
        let weekday = fields.next()?;
        let month = fields.next()?;
        let day = fields.next()?;
        let time = fields.next()?;
        let year = fields.next()?;
        if !seen_pids.insert(pid) {
            return None;
        }

        let started_at = Self::parse_started_at(weekday, month, day, time, year)?;
        let command = fields.collect::<Vec<_>>().join(" ");
        let classification = classify_process_command(&command);
        Some(AgentProcessRecord {
            pid,
            ppid,
            started_at: Some(started_at),
            provider: classification.provider,
            source: classification.source,
            executable: classification.executable,
            kind: classification.kind,
        })
    }

    fn parse_started_at(
        weekday: &str,
        month: &str,
        day: &str,
        time: &str,
        year: &str,
    ) -> Option<DateTime<Utc>> {
        let text = format!("{weekday} {month} {day} {time} {year}");
        chrono::NaiveDateTime::parse_from_str(&text, "%a %b %e %H:%M:%S %Y")
            .ok()
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
    }
}

struct ProcessClassification {
    provider: Option<AgentSessionProvider>,
    source: AgentSessionSource,
    executable: String,
    kind: AgentProcessKind,
}

fn classify_process_command(command: &str) -> ProcessClassification {
    let lower = command.to_ascii_lowercase();
    let executable = executable_basename(command);

    if lower.contains("app-server") && lower.contains("codex") {
        return ProcessClassification {
            provider: Some(AgentSessionProvider::Codex),
            source: AgentSessionSource::DesktopApp,
            executable,
            kind: AgentProcessKind::AppServer,
        };
    }

    if lower.contains("codex (renderer)")
        || lower.contains("claude-code-acp")
        || lower.contains("--help")
        || lower.contains("--version")
        || lower.contains("--type=renderer")
        || lower.contains("disclaimer")
        || executable.eq_ignore_ascii_case("disclaimer")
    {
        return ProcessClassification {
            provider: None,
            source: AgentSessionSource::Unknown,
            executable,
            kind: AgentProcessKind::Helper,
        };
    }

    if lower.contains("application support/claude/claude-code/claude")
        || lower.contains("claude.app")
        || lower.contains("claude.exe")
        || executable.eq_ignore_ascii_case("claude")
    {
        return ProcessClassification {
            provider: Some(AgentSessionProvider::Claude),
            source: if lower.contains("application support/claude/claude-code")
                || lower.contains("claude.app")
            {
                AgentSessionSource::DesktopApp
            } else {
                AgentSessionSource::Cli
            },
            executable: if executable.eq_ignore_ascii_case("claude") {
                "claude".to_string()
            } else {
                executable
            },
            kind: AgentProcessKind::Agent,
        };
    }

    if lower.contains("codex.exe")
        || lower.contains("codex.app")
        || lower.contains("codex desktop")
        || executable.eq_ignore_ascii_case("codex")
    {
        return ProcessClassification {
            provider: Some(AgentSessionProvider::Codex),
            source: if lower.contains("codex.app") || lower.contains("codex desktop") {
                AgentSessionSource::DesktopApp
            } else {
                AgentSessionSource::Cli
            },
            executable: if executable.eq_ignore_ascii_case("codex") {
                "codex".to_string()
            } else {
                executable
            },
            kind: AgentProcessKind::Agent,
        };
    }

    ProcessClassification {
        provider: None,
        source: AgentSessionSource::Unknown,
        executable,
        kind: AgentProcessKind::Other,
    }
}

fn executable_basename(command: &str) -> String {
    let normalized = command.replace('\\', "/").to_ascii_lowercase();
    for (needle, name) in [
        ("claude-code-acp", "claude-code-acp"),
        ("application support/claude/claude-code/claude", "claude"),
        ("codex app-server", "codex"),
        ("codex (renderer)", "codex"),
        ("codex.app", "codex"),
        ("claude.app", "claude"),
        ("claude.exe", "claude"),
        ("codex.exe", "codex"),
        ("disclaimer", "disclaimer"),
    ] {
        if normalized.contains(needle) {
            return name.to_string();
        }
    }

    let first_token = command.split_whitespace().next().unwrap_or_default();
    if first_token.is_empty() {
        return String::new();
    }

    Path::new(first_token)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(first_token)
        .to_string()
}

impl LSOFCWDOutputParser {
    pub fn parse(output: &str) -> HashMap<u32, String> {
        let mut result = HashMap::new();
        let mut current_pid = None;

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match line.chars().next() {
                Some('p') => {
                    current_pid = line[1..].trim().parse::<u32>().ok();
                }
                Some('n') => {
                    if let Some(pid) = current_pid {
                        result.insert(pid, line[1..].to_string());
                    }
                }
                _ => {}
            }
        }

        result
    }
}

impl ClaudeSessionProjectMapper {
    pub fn escaped_cwd(cwd: &str) -> String {
        cwd.chars()
            .map(|scalar| {
                if scalar.is_ascii_alphanumeric() {
                    scalar
                } else {
                    '-'
                }
            })
            .collect()
    }

    pub fn project_directories(cwd: &str, home_directory: &Path) -> Vec<PathBuf> {
        if cwd.trim().is_empty() {
            return Vec::new();
        }

        vec![
            home_directory
                .join(".claude")
                .join("projects")
                .join(Self::escaped_cwd(cwd)),
        ]
    }

    pub fn transcripts(cwd: &str, home_directory: &Path) -> Vec<ClaudeTranscript> {
        let mut transcripts = Vec::new();

        for directory in Self::project_directories(cwd, home_directory) {
            let Ok(entries) = fs::read_dir(&directory) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                    continue;
                }

                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                let Ok(modified) = metadata.modified() else {
                    continue;
                };

                transcripts.push(ClaudeTranscript::new(path, modified.into()));
            }
        }

        transcripts.sort_by(|lhs, rhs| {
            rhs.modified_at
                .cmp(&lhs.modified_at)
                .then_with(|| rhs.url.cmp(&lhs.url))
        });
        transcripts
    }

    pub fn newest_transcript(cwd: &str, home_directory: &Path) -> Option<ClaudeTranscript> {
        Self::transcripts(cwd, home_directory).into_iter().next()
    }
}

impl CodexRolloutFirstLineParser {
    pub fn parse(line: &str) -> Option<CodexRolloutMetadata> {
        let value: Value = serde_json::from_str(line).ok()?;
        if value.get("type")?.as_str()? != "session_meta" {
            return None;
        }

        let payload = value.get("payload")?.as_object()?;
        let session_id = payload
            .get("session_id")
            .or_else(|| payload.get("id"))?
            .as_str()?;
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return None;
        }

        Some(CodexRolloutMetadata {
            session_id: session_id.to_string(),
            cwd: payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            originator: payload
                .get("originator")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
            source: payload
                .get("source")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
        })
    }

    pub fn read_first_line(path: &Path) -> Option<String> {
        let file = File::open(path).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            return None;
        }
        while line.ends_with(['\n', '\r']) {
            line.pop();
        }
        Some(line)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn make_session(
        metadata: CodexRolloutMetadata,
        transcript_path: &Path,
        modified_at: DateTime<Utc>,
        pid: Option<u32>,
        started_at: Option<DateTime<Utc>>,
        host: &str,
        config: SessionScanConfig,
        now: DateTime<Utc>,
    ) -> Option<AgentSession> {
        if pid.is_none() && !config.file_only_session_allowed(modified_at, now) {
            return None;
        }

        let session_source = metadata.session_source();
        let workspace = AgentSessionWorkspace {
            cwd: metadata.cwd.clone(),
            project_name: metadata.cwd.as_deref().and_then(project_name_from_cwd),
        };
        let transcript_path = transcript_path.to_string_lossy().to_string();
        let focus_target = match pid {
            Some(pid) => AgentSessionFocusTarget::Process { pid },
            None => AgentSessionFocusTarget::Transcript {
                transcript_path: transcript_path.clone(),
            },
        };

        Some(AgentSession::new(
            metadata.session_id,
            AgentSessionProvider::Codex,
            session_source,
            config.state(Some(modified_at), now, pid.is_some()),
            pid,
            Some(transcript_path),
            host,
            workspace,
            AgentSessionActivity {
                started_at,
                last_activity_at: Some(modified_at),
            },
            focus_target,
        ))
    }
}

impl AgentSessionCorrelation {
    pub fn project_name(cwd: Option<&str>) -> Option<String> {
        cwd.and_then(project_name_from_cwd)
    }
}

impl RemoteSessionFetcher {
    pub fn sanitized_hosts(hosts: &[String]) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut sanitized = Vec::new();

        for host in hosts {
            let Ok(host) = Self::validate_host(host) else {
                continue;
            };

            let key = host.to_ascii_lowercase();
            if seen.insert(key) {
                sanitized.push(host);
            }
        }

        sanitized
    }

    pub fn validate_host(host: &str) -> Result<String, String> {
        if host.is_empty() {
            return Err("host must not be empty".to_string());
        }
        if host.starts_with('-') {
            return Err("host must not start with '-'".to_string());
        }
        if host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || !is_safe_host_char(c))
        {
            return Err(
                "host must not contain whitespace, control characters, or unsafe shell characters"
                    .to_string(),
            );
        }

        Ok(host.to_string())
    }

    pub fn decode_host_result(body: &str) -> Result<AgentSessionHostResult, String> {
        let result: AgentSessionHostResult = serde_json::from_str(body)
            .map_err(|err| actionable_message("Unable to decode remote session response", err))?;
        Self::validate_host(&result.host).map_err(|err| {
            actionable_message("Remote session response has an invalid host", err)
        })?;
        Ok(result)
    }

    pub fn failed_result(host: &str, err: impl std::fmt::Display) -> AgentSessionHostResult {
        AgentSessionHostResult::failed(host.to_string(), err)
    }
}

fn actionable_message(label: &str, err: impl std::fmt::Display) -> String {
    crate::logging::safe_error_message(format!("{label}: {err}"))
}

fn is_safe_host_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']' | '_')
}

fn project_name_from_cwd(cwd: &str) -> Option<String> {
    let trimmed = cwd.trim().trim_end_matches(['\\', '/']);
    let path = Path::new(trimmed);
    let name = path.file_name()?.to_str()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn process_parser_filters_helpers_app_server_duplicates_and_malformed_lines() {
        let output = "\
101   1 Mon Jul  6 09:00:00 2026 /Applications/Claude.app/Contents/Resources/disclaimer /Users/test/Library/Application Support/Claude/claude-code/claude --dangerously-skip-permissions
102 101 Mon Jul  6 09:00:01 2026 /Users/test/Library/Application Support/Claude/claude-code/claude --dangerously-skip-permissions
102 101 Mon Jul  6 09:00:01 2026 /Users/test/Library/Application Support/Claude/claude-code/claude --dangerously-skip-permissions
201   1 Mon Jul  6 09:01:00 2026 /opt/homebrew/bin/codex exec --full-auto strange argv here
202   1 Mon Jul  6 09:02:00 2026 /Applications/Codex.app/Contents/Resources/codex app-server --listen stdio
203   1 Mon Jul  6 09:03:00 2026 /usr/local/bin/codex --help
301   1 Mon Jul  6 09:04:00 2026 /Users/test/.local/bin/claude-code-acp --stdio
401   1 Mon Jul  6 09:05:00 2026 /Applications/Codex.app/Contents/Frameworks/Codex Framework.framework/Helpers/Codex (Renderer) --type=renderer
bad line
";

        let records = AgentPSOutputParser::parse(output);
        let agents = AgentPSOutputParser::agent_processes(&records);

        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn session_scan_config_cuts_off_active_and_file_only_windows() {
        let config = SessionScanConfig::default();
        let now = Utc.with_ymd_and_hms(2026, 7, 12, 0, 0, 0).unwrap();

        assert_eq!(
            config.state(Some(now - chrono::Duration::seconds(119)), now, true),
            AgentSessionState::Active
        );
        assert_eq!(
            config.state(Some(now - chrono::Duration::seconds(121)), now, true),
            AgentSessionState::Idle
        );
        assert!(config.file_only_session_allowed(now - chrono::Duration::minutes(29), now));
        assert!(!config.file_only_session_allowed(now - chrono::Duration::minutes(31), now));
    }

    #[test]
    fn agent_session_round_trips_json() {
        let session = AgentSession {
            id: "session-1".to_string(),
            provider: AgentSessionProvider::Codex,
            source: AgentSessionSource::DesktopApp,
            state: AgentSessionState::Active,
            pid: Some(1234),
            transcript_path: Some("C:\\sessions\\rollout.jsonl".to_string()),
            host: "devbox".to_string(),
            workspace: AgentSessionWorkspace {
                cwd: Some("C:\\work\\proj".to_string()),
                project_name: Some("proj".to_string()),
            },
            activity: AgentSessionActivity {
                started_at: Some(Utc.with_ymd_and_hms(2026, 7, 12, 0, 0, 0).unwrap()),
                last_activity_at: Some(Utc.with_ymd_and_hms(2026, 7, 12, 0, 1, 0).unwrap()),
            },
            focus_target: AgentSessionFocusTarget::Process { pid: 1234 },
        };

        let json = serde_json::to_string(&session).unwrap();
        let round_tripped: AgentSession = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, session);
        assert!(json.contains("\"focusTarget\""));
    }

    #[test]
    fn focus_result_serializes_safely() {
        let focused = serde_json::to_value(&SessionFocusResult::Focused).unwrap();
        let unsupported = serde_json::to_value(&SessionFocusResult::Unsupported {
            message: "focus unavailable".to_string(),
        })
        .unwrap();
        let failed = serde_json::to_value(&SessionFocusResult::Failed {
            message: "failed to focus".to_string(),
        })
        .unwrap();

        assert!(focused.is_string() || focused.is_object());
        assert_eq!(unsupported["message"], "focus unavailable");
        assert_eq!(failed["message"], "failed to focus");
    }

    #[test]
    fn host_validation_dedupes_and_rejects_unsafe_values() {
        let hosts = RemoteSessionFetcher::sanitized_hosts(&[
            "".to_string(),
            " ".to_string(),
            "-bad".to_string(),
            "good".to_string(),
            "good".to_string(),
            "GOOD".to_string(),
            "bad host".to_string(),
            "bad\tcontrol".to_string(),
        ]);
        assert_eq!(hosts, vec!["good".to_string()]);
    }

    #[test]
    fn codex_rollout_parser_reads_first_line_metadata() {
        let metadata = CodexRolloutFirstLineParser::parse(
            r#"{"type":"session_meta","payload":{"session_id":"abc","cwd":"C:\\work\\proj","originator":"codex_exec","source":"cli"}}"#,
        )
        .unwrap();
        assert_eq!(metadata.session_id, "abc");
        assert_eq!(metadata.cwd.as_deref(), Some("C:\\work\\proj"));
    }

    #[test]
    fn claude_cwd_escape_is_stable() {
        assert_eq!(
            ClaudeSessionProjectMapper::escaped_cwd(r"C:\Users\me\My Project!"),
            "C--Users-me-My-Project-"
        );
    }

    #[test]
    fn remote_session_result_round_trips() {
        let result = AgentSessionHostResult {
            host: "devbox".to_string(),
            sessions: vec![AgentSession {
                id: "session-1".to_string(),
                provider: AgentSessionProvider::Claude,
                source: AgentSessionSource::Cli,
                state: AgentSessionState::Idle,
                pid: None,
                transcript_path: None,
                host: "devbox".to_string(),
                workspace: AgentSessionWorkspace {
                    cwd: None,
                    project_name: None,
                },
                activity: AgentSessionActivity {
                    started_at: None,
                    last_activity_at: None,
                },
                focus_target: AgentSessionFocusTarget::None,
            }],
            error: Some("ssh not found".to_string()),
        };

        let json = serde_json::to_string(&result).unwrap();
        let decoded: AgentSessionHostResult = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, result);
    }
}
