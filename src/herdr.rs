use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::config::AgentKind;
use crate::terminal_session::TerminalSession;

static GLOBAL_SEQ: AtomicU64 = AtomicU64::new(0);

/// Generates a strictly monotonically increasing sequence number for Herdr RPC calls
pub fn next_seq() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    loop {
        let current = GLOBAL_SEQ.load(Ordering::Relaxed);
        let next = if now > current { now } else { current + 1 };
        if GLOBAL_SEQ.compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            return next;
        }
    }
}

/// State of an AI agent according to Herdr's specification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentState {
    Idle,
    Working,
    Blocked, // Waiting for manual user action (permission, approval, prompt)
}

impl AgentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
        }
    }
}

/// Native integration client with Herdr (Terminal Workspace Manager for AI Coding Agents).
/// Manages live status tracking for both Antigravity CLI and Claude Code chats on Weavers pane.
#[derive(Clone)]
pub struct HerdrClient {
    pub socket_path: PathBuf,
    pub pane_id: String,
    tx: std::sync::mpsc::Sender<serde_json::Value>,
}

impl std::fmt::Debug for HerdrClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HerdrClient")
            .field("socket_path", &self.socket_path)
            .field("pane_id", &self.pane_id)
            .finish()
    }
}

impl HerdrClient {
    /// Creates a HerdrClient with the specified socket path and pane ID
    pub fn new(socket_path: PathBuf, pane_id: String) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<serde_json::Value>();
        let sp = socket_path.clone();
        std::thread::Builder::new()
            .name("herdr-rpc-worker".to_string())
            .spawn(move || {
                while let Ok(req) = rx.recv() {
                    let _ = send_herdr_request(&sp, &req);
                }
            })
            .ok();

        Self {
            socket_path,
            pane_id,
            tx,
        }
    }

    /// Attempts to initialize the Herdr client by discovering the Unix socket and current pane ID.
    pub fn try_detect() -> Option<Self> {
        let socket_path = resolve_socket_path()?;
        if !socket_path.exists() {
            return None;
        }

        // 1. Try obtaining pane_id directly from HERDR_PANE_ID env var
        let pane_id = if let Ok(env_id) = std::env::var("HERDR_PANE_ID") {
            if !env_id.trim().is_empty() {
                Some(env_id.trim().to_string())
            } else {
                None
            }
        } else {
            None
        };

        // 2. Fallback: Query Herdr socket for the active pane matching current process PID
        let pane_id = pane_id.or_else(|| discover_pane_id(&socket_path))?;

        Some(Self::new(socket_path, pane_id))
    }
}

/// Shape of `HERDR_PLUGIN_CONTEXT_JSON` injected by Herdr runtime into plugin commands and panes.
#[derive(serde::Deserialize, Default, Debug)]
struct HerdrPluginContext {
    #[serde(default)]
    focused_pane_cwd: Option<String>,
    #[serde(default)]
    workspace_cwd: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

/// Extracts the most specific working directory candidate from a Herdr plugin context JSON string.
pub fn parse_herdr_working_dir_from_json(json_str: &str) -> Option<String> {
    let raw: HerdrPluginContext = serde_json::from_str(json_str).ok()?;
    raw.focused_pane_cwd
        .filter(|s| !s.trim().is_empty())
        .or_else(|| raw.workspace_cwd.filter(|s| !s.trim().is_empty()))
        .or_else(|| raw.cwd.filter(|s| !s.trim().is_empty()))
}

/// Detects the target working directory from Herdr plugin context, if available and valid on disk.
pub fn detect_herdr_working_dir() -> Option<String> {
    let json_str = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok()?;
    let candidate = parse_herdr_working_dir_from_json(&json_str)?;
    let path = Path::new(&candidate);
    if path.exists() {
        Some(candidate)
    } else {
        None
    }
}

impl HerdrClient {
    pub fn report_active_agent(
        &self,
        agent: &str,
        state: AgentState,
        message: Option<&str>,
        session_id: Option<&str>,
        is_reviewer: bool,
    ) {
        let agent_kind = AgentKind::parse(agent).unwrap_or(AgentKind::Agy);

        let state_labels = if is_reviewer {
            serde_json::json!({
                "working": "Reviewing",
                "idle": "Reviewer",
                "blocked": "Needs Input",
            })
        } else {
            serde_json::json!({
                "working": "Working",
                "idle": "Ready",
                "blocked": "Needs Input",
            })
        };

        // 1. Report display metadata (agent kind & state labels) to Herdr.
        // NOTE: We deliberately do NOT include a custom "title" here, allowing plugins
        // like herdr-auto-title or the user to manage the tab title without an infinite rename conflict.
        let meta_seq = next_seq();
        let meta_req = serde_json::json!({
            "id": format!("herdr-diff:meta:{}", meta_seq),
            "method": "pane.report_metadata",
            "params": {
                "pane_id": &self.pane_id,
                "source": "herdr-interactive-diff",
                "agent": agent_kind.as_str(),
                "display_agent": agent_kind.as_str(),
                "state_labels": state_labels,
                "seq": meta_seq,
            }
        });
        let _ = self.tx.send(meta_req);

        // 2. Report active agent status
        let agent_seq = next_seq();
        let mut params = serde_json::json!({
            "pane_id": &self.pane_id,
            "source": "herdr-interactive-diff",
            "agent": agent_kind.as_str(),
            "state": state.as_str(),
            "seq": agent_seq,
        });

        if let Some(msg) = message {
            params["message"] = serde_json::json!(msg);
        }
        if let Some(sess) = session_id {
            params["agent_session_id"] = serde_json::json!(sess);
        }

        let request = serde_json::json!({
            "id": format!("herdr-diff:report:{}", agent_seq),
            "method": "pane.report_agent",
            "params": params,
        });
        let _ = self.tx.send(request);
    }

    /// Reports Antigravity CLI status to Herdr on the Weavers pane
    #[allow(dead_code)]
    pub fn report_agy(&self, state: AgentState, message: Option<&str>, session_id: Option<&str>, is_reviewer: bool) {
        self.report_active_agent("agy", state, message, session_id, is_reviewer);
    }

    /// Reports Claude Code status to Herdr on the Weavers pane
    #[allow(dead_code)]
    pub fn report_claude(&self, state: AgentState, message: Option<&str>, session_id: Option<&str>, is_reviewer: bool) {
        self.report_active_agent("claude", state, message, session_id, is_reviewer);
    }

    /// Releases specific registered agents on exit without stalling
    pub fn release_agents(&self, agents: &[&str]) {
        for agent_name in agents {
            if let Some(agent) = AgentKind::parse(agent_name) {
                let ag_str = agent.as_str();
                let s = next_seq();
                let req = serde_json::json!({
                    "id": format!("herdr-diff:release:{}:{}", ag_str, s),
                    "method": "pane.release_agent",
                    "params": {
                        "pane_id": &self.pane_id,
                        "source": "herdr-interactive-diff",
                        "agent": ag_str,
                        "seq": s,
                    }
                });
                let _ = self.tx.send(req);
            }
        }

        let clear_seq = next_seq();
        let clear_req = serde_json::json!({
            "id": format!("herdr-diff:clear_authority:{}", clear_seq),
            "method": "pane.clear_agent_authority",
            "params": {
                "pane_id": &self.pane_id,
                "source": "herdr-interactive-diff",
                "seq": clear_seq,
            }
        });
        let _ = self.tx.send(clear_req);
    }

    /// Cleanly releases registered agent state on Weavers pane when exiting
    pub fn release_all(&mut self) {
        self.release_agents(&["agy", "claude"]);
    }
}

/// Detects the operational state of an agent (Working, Blocked, or Idle) from its live terminal screen
pub fn detect_session_state(session: &TerminalSession, agent_name: &str) -> AgentState {
    let screen = session.read_screen_text();
    let trimmed = screen.trim();
    if trimmed.is_empty() {
        return AgentState::Idle;
    }

    let non_empty_lines: Vec<&str> = screen
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if non_empty_lines.is_empty() {
        return AgentState::Idle;
    }

    // Inspect bottom 6 lines where active prompts, confirmations, and spinners live
    let bottom_slice = if non_empty_lines.len() > 6 {
        &non_empty_lines[non_empty_lines.len() - 6..]
    } else {
        &non_empty_lines[..]
    };
    let bottom_text = bottom_slice.join("\n");
    let lower = bottom_text.to_lowercase();

    // 1. Check for Blocked state (manual user approval, confirmation, question dialog)
    let is_blocked = match agent_name {
        "claude" => {
            lower.contains("waiting for permission")
                || (lower.contains("esc to cancel")
                    && (lower.contains("enter to confirm")
                        || lower.contains("enter to select")
                        || lower.contains("allow")
                        || lower.contains("tab to amend")))
                || lower.contains("do you want to proceed?")
                || lower.contains("do you want to allow")
                || lower.contains("approval required")
                || lower.contains("requesting permission for:")
                || lower.contains("approve this command")
                || (lower.contains("[y/n]") || lower.contains("(y/n)"))
        }
        "agy" => {
            lower.contains("requesting permission for:")
                || lower.contains("do you want to proceed?")
                || lower.contains("tab amend")
                || lower.contains("edit command")
                || lower.contains("permission required")
                || (lower.contains("[y/n]") || lower.contains("(y/n)"))
        }
        _ => {
            lower.contains("requesting permission")
                || lower.contains("permission required")
                || lower.contains("do you want to proceed?")
                || lower.contains("approval required")
                || lower.contains("allow this command")
                || (lower.contains("[y/n]") || lower.contains("(y/n)"))
        }
    };

    if is_blocked {
        return AgentState::Blocked;
    }

    // 2. Check for Working state (spinners, thinking indicator, background task active)
    let braille_spinners = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let circle_spinners = ['◐', '◓', '◑', '◒'];
    let has_spinner = braille_spinners.iter().any(|&c| bottom_text.contains(c))
        || circle_spinners.iter().any(|&c| bottom_text.contains(c));

    if has_spinner {
        return AgentState::Working;
    }

    match agent_name {
        "claude"
            if lower.contains("esc to interrupt")
                || lower.contains("ctrl+c to interrupt")
                || lower.contains("thinking…")
                || lower.contains("thinking...")
                || lower.contains("waiting for")
                || lower.contains("mcp task") =>
        {
            return AgentState::Working;
        }
        "agy"
            if lower.contains("thinking…")
                || lower.contains("thinking...")
                || lower.contains("analyzing…")
                || lower.contains("analyzing...")
                || lower.contains("generating…")
                || lower.contains("generating...")
                || lower.contains("running command") =>
        {
            return AgentState::Working;
        }
        _ if lower.contains("thinking…")
            || lower.contains("thinking...")
            || lower.contains("analyzing…")
            || lower.contains("analyzing...")
            || lower.contains("generating…")
            || lower.contains("generating...")
            || lower.contains("running command") =>
        {
            return AgentState::Working;
        }
        _ => {}
    }

    AgentState::Idle
}

/// Helper for backward compatibility
#[allow(dead_code)]
pub fn detect_session_working(session: &TerminalSession, agent_name: &str) -> bool {
    detect_session_state(session, agent_name) == AgentState::Working
}

/// Resolves the socket path for the Herdr server
fn resolve_socket_path() -> Option<PathBuf> {
    if let Ok(path_str) = std::env::var("HERDR_SOCKET_PATH") {
        if !path_str.trim().is_empty() {
            return Some(PathBuf::from(path_str));
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let default_path = PathBuf::from(home).join(".config/herdr/herdr.sock");
        if default_path.exists() {
            return Some(default_path);
        }
    }

    None
}

/// Sends a JSON-RPC request to the Herdr Unix socket
fn send_herdr_request(socket_path: &Path, request: &serde_json::Value) -> Option<serde_json::Value> {
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(400))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(400))).ok()?;

    let mut body = serde_json::to_vec(request).ok()?;
    body.push(b'\n');
    stream.write_all(&body).ok()?;

    let mut response_buf = vec![0u8; 16384];
    let bytes_read = stream.read(&mut response_buf).ok()?;
    if bytes_read == 0 {
        return None;
    }

    serde_json::from_slice(&response_buf[..bytes_read]).ok()
}

/// Discovers the current pane ID by inspecting Herdr session snapshot
fn discover_pane_id(socket_path: &Path) -> Option<String> {
    let snap_req = serde_json::json!({
        "id": "weavers:discover_pane",
        "method": "session.snapshot",
        "params": {}
    });

    let resp = send_herdr_request(socket_path, &snap_req)?;
    let snapshot = resp.get("result")?.get("snapshot")?;

    let my_pid = std::process::id();
    let panes = snapshot.get("panes")?.as_array()?;

    // Search for a pane whose foreground processes contain this process PID
    for p in panes {
        if let Some(pane_id_val) = p.get("pane_id").and_then(|v| v.as_str()) {
            let proc_req = serde_json::json!({
                "id": "weavers:pane_proc",
                "method": "pane.process_info",
                "params": { "pane_id": pane_id_val }
            });

            if let Some(proc_resp) = send_herdr_request(socket_path, &proc_req) {
                if let Some(info) = proc_resp.get("result").and_then(|r| r.get("process_info")) {
                    if info.get("shell_pid").and_then(|v| v.as_u64()) == Some(my_pid as u64) {
                        return Some(pane_id_val.to_string());
                    }

                    if let Some(procs) = info.get("foreground_processes").and_then(|v| v.as_array()) {
                        for proc in procs {
                            if proc.get("pid").and_then(|v| v.as_u64()) == Some(my_pid as u64) {
                                return Some(pane_id_val.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: use focused_pane_id
    if let Some(focused) = snapshot.get("focused_pane_id").and_then(|v| v.as_str()) {
        if !focused.is_empty() {
            return Some(focused.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_socket_path() {
        let path = resolve_socket_path();
        if let Ok(home) = std::env::var("HOME") {
            let expected = PathBuf::from(home).join(".config/herdr/herdr.sock");
            if expected.exists() {
                assert!(path.is_some());
            }
        }
    }

    #[test]
    fn test_payload_generation() {
        let client = HerdrClient::new(PathBuf::from("/tmp/herdr.sock"), "test:p1".to_string());
        assert_eq!(client.pane_id, "test:p1");
    }

    #[test]
    fn test_agent_state_strings() {
        assert_eq!(AgentState::Idle.as_str(), "idle");
        assert_eq!(AgentState::Working.as_str(), "working");
        assert_eq!(AgentState::Blocked.as_str(), "blocked");
    }

    #[test]
    fn test_next_seq_strictly_increasing() {
        let s1 = next_seq();
        let s2 = next_seq();
        let s3 = next_seq();
        assert!(s1 < s2, "s1 ({}) should be < s2 ({})", s1, s2);
        assert!(s2 < s3, "s2 ({}) should be < s3 ({})", s2, s3);
    }

    #[test]
    fn test_reviewer_state_labels() {
        let reviewer_labels = serde_json::json!({
            "working": "Reviewing",
            "idle": "Reviewer",
            "blocked": "Needs Input",
        });
        assert_eq!(reviewer_labels["working"], "Reviewing");
        assert_eq!(reviewer_labels["idle"], "Reviewer");

        let primary_labels = serde_json::json!({
            "working": "Working",
            "idle": "Ready",
            "blocked": "Needs Input",
        });
        assert_eq!(primary_labels["working"], "Working");
        assert_eq!(primary_labels["idle"], "Ready");
    }

    #[test]
    fn test_parse_herdr_working_dir_prefers_focused_pane_cwd() {
        let json = r#"{"focused_pane_cwd": "/path/to/pane", "workspace_cwd": "/path/to/ws"}"#;
        assert_eq!(
            super::parse_herdr_working_dir_from_json(json),
            Some("/path/to/pane".to_string())
        );
    }

    #[test]
    fn test_parse_herdr_working_dir_falls_back_to_workspace_cwd() {
        let json = r#"{"focused_pane_cwd": "", "workspace_cwd": "/path/to/ws"}"#;
        assert_eq!(
            super::parse_herdr_working_dir_from_json(json),
            Some("/path/to/ws".to_string())
        );
    }

    #[test]
    fn test_parse_herdr_working_dir_falls_back_to_cwd() {
        let json = r#"{"cwd": "/path/to/cwd"}"#;
        assert_eq!(
            super::parse_herdr_working_dir_from_json(json),
            Some("/path/to/cwd".to_string())
        );
    }

    #[test]
    fn test_parse_herdr_working_dir_handles_empty_or_malformed() {
        assert_eq!(super::parse_herdr_working_dir_from_json("{}"), None);
        assert_eq!(super::parse_herdr_working_dir_from_json("invalid json"), None);
    }

    #[test]
    fn test_report_active_agent_payloads() {
        let (tx, rx) = std::sync::mpsc::channel();
        let client = HerdrClient {
            socket_path: PathBuf::from("/tmp/herdr.sock"),
            pane_id: "w1:p1".to_string(),
            tx,
        };
        client.report_active_agent("agy", AgentState::Working, Some("Analyzing"), Some("conv-123"), false);

        let meta = rx.recv().expect("meta message");
        assert_eq!(meta["method"], "pane.report_metadata");
        assert_eq!(meta["params"]["source"], "herdr-interactive-diff");
        assert_eq!(meta["params"]["agent"], "agy");
        assert_eq!(meta["params"]["display_agent"], "agy");

        let agent = rx.recv().expect("agent message");
        assert_eq!(agent["method"], "pane.report_agent");
        assert_eq!(agent["params"]["source"], "herdr-interactive-diff");
        assert_eq!(agent["params"]["agent"], "agy");
        assert_eq!(agent["params"]["state"], "working");
        assert_eq!(agent["params"]["agent_session_id"], "conv-123");
    }

    #[test]
    fn test_release_agents_payloads() {
        let (tx, rx) = std::sync::mpsc::channel();
        let client = HerdrClient {
            socket_path: PathBuf::from("/tmp/herdr.sock"),
            pane_id: "w1:p1".to_string(),
            tx,
        };
        client.release_agents(&["agy"]);

        let release = rx.recv().expect("release message");
        assert_eq!(release["method"], "pane.release_agent");
        assert_eq!(release["params"]["source"], "herdr-interactive-diff");
        assert_eq!(release["params"]["agent"], "agy");

        let clear = rx.recv().expect("clear authority message");
        assert_eq!(clear["method"], "pane.clear_agent_authority");
        assert_eq!(clear["params"]["source"], "herdr-interactive-diff");
    }
}
