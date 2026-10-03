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


/// Represents an active agent detected by Herdr in a pane
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DetectedAgent {
    #[serde(default)]
    pub terminal_id: Option<String>,
    pub agent: String,
    pub agent_status: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub tab_id: Option<String>,
    pub pane_id: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub agent_session: Option<serde_json::Value>,
}

/// Native integration client with Herdr (Terminal Workspace Manager for AI Coding Agents).
/// Manages live status tracking and RPC coordination with decoupled agent panes in Herdr.
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
        let env_pane_id = std::env::var("HERDR_PANE_ID")
            .ok()
            .filter(|s| !s.trim().is_empty());

        // 2. Discover if current process belongs to a known Herdr pane
        let (pane_id, is_dedicated_diff_pane) = if let Some(id) = env_pane_id {
            (id, true)
        } else if let Some(id) = discover_pane_id(&socket_path) {
            (id, true)
        } else {
            ("cli".to_string(), false)
        };

        let client = Self::new(socket_path, pane_id);

        // Ensure herdr-interactive-diff is never treated as an AI agent in Herdr's UI or sidebar,
        // but ONLY if running as a dedicated diff pane (never clear authority on arbitrary or CLI panes)
        if is_dedicated_diff_pane {
            let _ = client.call_sync("pane.clear_agent_authority", serde_json::json!({
                "pane_id": &client.pane_id,
                "source": "herdr-interactive-diff",
            }));
            let _ = client.call_sync("pane.release_agent", serde_json::json!({
                "pane_id": &client.pane_id,
                "source": "herdr-interactive-diff",
                "agent": "agy",
            }));
            let _ = client.call_sync("pane.release_agent", serde_json::json!({
                "pane_id": &client.pane_id,
                "source": "herdr-interactive-diff",
                "agent": "claude",
            }));
            let _ = client.call_sync("pane.report_metadata", serde_json::json!({
                "pane_id": &client.pane_id,
                "source": "herdr-interactive-diff",
                "clear_display_agent": true,
                "clear_state_labels": true,
            }));
        }

        Some(client)
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

    /// Reports Antigravity CLI status to Herdr on the Herdr Interactive Diff pane
    #[allow(dead_code)]
    pub fn report_agy(&self, state: AgentState, message: Option<&str>, session_id: Option<&str>, is_reviewer: bool) {
        self.report_active_agent("agy", state, message, session_id, is_reviewer);
    }

    /// Reports Claude Code status to Herdr on the Herdr Interactive Diff pane
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

    /// Cleanly releases registered agent state on Herdr Interactive Diff pane when exiting
    pub fn release_all(&mut self) {
        self.release_agents(&["agy", "claude"]);
    }

    /// Sends a synchronous RPC call to Herdr
    pub fn call_sync(&self, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
        send_herdr_rpc(&self.socket_path, method, params)
    }

    /// Queries the list of currently active agents from Herdr (agent.list)
    pub fn list_agents(&self) -> Vec<DetectedAgent> {
        let resp = match self.call_sync("agent.list", serde_json::json!({})) {
            Some(r) => r,
            None => return Vec::new(),
        };

        let agents_val = match resp.get("result").and_then(|r| r.get("agents")) {
            Some(a) => a,
            None => return Vec::new(),
        };

        let mut agents: Vec<DetectedAgent> = serde_json::from_value(agents_val.clone()).unwrap_or_default();
        agents.retain(|a| {
            !a.agent.to_lowercase().contains("diff")
                && !a.agent.to_lowercase().contains("interactive")
        });
        agents
    }

    /// Queries the currently focused workspace ID in Herdr
    pub fn get_focused_workspace_id(&self) -> Option<String> {
        let resp = self.call_sync("workspace.list", serde_json::json!({}))?;
        let workspaces = resp.get("result")?.get("workspaces")?.as_array()?;
        for ws in workspaces {
            if ws.get("focused").and_then(|v| v.as_bool()).unwrap_or(false) {
                return ws.get("workspace_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            }
        }
        None
    }

    /// Reads output text from a specific Herdr pane
    pub fn read_pane_text(&self, pane_id: &str, lines: u32) -> Option<String> {
        let params = serde_json::json!({
            "pane_id": pane_id,
            "source": "recent_unwrapped",
            "lines": lines,
        });
        let resp = self.call_sync("pane.read", params)?;
        resp.get("result")
            .and_then(|r| r.get("read"))
            .and_then(|rd| rd.get("text"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
    }

    /// Submits a prompt to an agent or pane in Herdr
    pub fn submit_agent_prompt(&self, target: &str, text: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "target": target,
            "text": text,
        });
        let resp = self.call_sync("agent.prompt", params);
        if let Some(r) = resp {
            if let Some(err) = r.get("error") {
                // If agent.prompt rejected, fallback to pane.send_text
                let _ = self.call_sync("pane.send_text", serde_json::json!({
                    "pane_id": target,
                    "text": format!("{}\n", text),
                }));
                anyhow::bail!("agent.prompt returned error: {:?}", err);
            }
            return Ok(());
        }
        // Fallback: send text directly to pane
        let _ = self.call_sync("pane.send_text", serde_json::json!({
            "pane_id": target,
            "text": format!("{}\n", text),
        }));
        Ok(())
    }

    /// Focuses a pane in Herdr
    pub fn focus_pane(&self, pane_id: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "pane_id": pane_id,
        });
        let resp = self.call_sync("pane.focus", params).ok_or_else(|| anyhow::anyhow!("Herdr socket not responding"))?;
        if let Some(err) = resp.get("error") {
            anyhow::bail!("pane.focus error: {:?}", err);
        }
        Ok(())
    }

    /// Focuses an agent in Herdr
    pub fn focus_agent(&self, target: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "target": target,
        });
        let resp = self.call_sync("agent.focus", params).ok_or_else(|| anyhow::anyhow!("Herdr socket not responding"))?;
        if let Some(err) = resp.get("error") {
            anyhow::bail!("agent.focus error: {:?}", err);
        }
        Ok(())
    }

    /// Shows a toast notification via Herdr
    pub fn show_notification(&self, message: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "message": message,
        });
        let resp = self.call_sync("notification.show", params).ok_or_else(|| anyhow::anyhow!("Herdr socket not responding"))?;
        if let Some(err) = resp.get("error") {
            anyhow::bail!("notification.show error: {:?}", err);
        }
        Ok(())
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

/// Sends a JSON-RPC request to the Herdr Unix socket and reads the complete response
fn send_herdr_request(socket_path: &Path, request: &serde_json::Value) -> Option<serde_json::Value> {
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(1000))).ok()?;

    let mut body = serde_json::to_vec(request).ok()?;
    body.push(b'\n');
    stream.write_all(&body).ok()?;

    let mut response_buf = Vec::with_capacity(32768);
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                response_buf.extend_from_slice(&chunk[..n]);
                if response_buf.contains(&b'\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if response_buf.is_empty() {
        return None;
    }

    serde_json::from_slice(&response_buf).ok()
}

/// Standalone helper to send a JSON-RPC request to Herdr socket
pub fn send_herdr_rpc(socket_path: &Path, method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    let seq = next_seq();
    let req = serde_json::json!({
        "id": format!("herdr-diff:standalone:{}", seq),
        "method": method,
        "params": params,
    });
    send_herdr_request(socket_path, &req)
}

/// Discovers the current pane ID by inspecting Herdr session snapshot
fn discover_pane_id(socket_path: &Path) -> Option<String> {
    let snap_req = serde_json::json!({
        "id": "herdr-diff:discover_pane",
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
                "id": "herdr-diff:pane_proc",
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

    None
}

/// Extracts pane ID from a `plugin.pane.open` response
pub fn extract_plugin_pane_id(response: &serde_json::Value) -> Option<String> {
    let result = response.get("result")?;
    // Format 1: result.plugin_pane.pane.pane_id
    if let Some(id) = result
        .get("plugin_pane")
        .and_then(|pp| pp.get("pane"))
        .and_then(|p| p.get("pane_id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // Format 2: result.plugin_pane.pane_id
    if let Some(id) = result
        .get("plugin_pane")
        .and_then(|pp| pp.get("pane_id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // Format 3: result.pane.pane_id
    if let Some(id) = result
        .get("pane")
        .and_then(|p| p.get("pane_id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // Format 4: result.pane_id
    if let Some(id) = result.get("pane_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    None
}

/// Helper to extract tab_id from Herdr plugin.pane.open responses
pub fn extract_plugin_tab_id(resp: &serde_json::Value) -> Option<String> {
    let result = resp.get("result")?;

    // Format 1: result.plugin_pane.pane.tab_id
    if let Some(id) = result
        .get("plugin_pane")
        .and_then(|pp| pp.get("pane"))
        .and_then(|p| p.get("tab_id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // Format 2: result.tab.tab_id
    if let Some(id) = result
        .get("tab")
        .and_then(|t| t.get("tab_id"))
        .and_then(|v| v.as_str())
    {
        return Some(id.to_string());
    }
    // Format 3: result.tab_id
    if let Some(id) = result.get("tab_id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    None
}

/// Opens herdr-interactive-diff in a vertical split docked on the left side of the current active pane.
/// If interactive diff is already running in the current tab/workspace, focuses it instead of opening a duplicate.
pub fn open_split_left(socket_path_opt: Option<&Path>) -> Result<(), String> {
    let socket_path = match socket_path_opt {
        Some(p) => p.to_path_buf(),
        None => resolve_socket_path().ok_or_else(|| "Could not find Herdr socket".to_string())?,
    };

    // 1. Query current session snapshot
    let snap_resp = send_herdr_rpc(&socket_path, "session.snapshot", serde_json::json!({}))
        .ok_or_else(|| "Failed to query Herdr session snapshot".to_string())?;

    let snapshot = snap_resp.get("result").and_then(|r| r.get("snapshot"));
    let focused_pane_id = snapshot
        .and_then(|s| s.get("focused_pane_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let panes = snapshot
        .and_then(|s| s.get("panes"))
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    // Determine current tab from focused pane
    let current_pane = panes
        .iter()
        .find(|p| p.get("pane_id").and_then(|v| v.as_str()) == focused_pane_id.as_deref());
    let current_tab_id = current_pane
        .and_then(|p| p.get("tab_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Check if an interactive diff pane is already open in the CURRENT tab
    if let Some(ref cur_tab) = current_tab_id {
        for p in &panes {
            if p.get("tab_id").and_then(|v| v.as_str()) == Some(cur_tab.as_str()) {
                let p_id = p.get("pane_id").and_then(|v| v.as_str()).unwrap_or("");
                let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let label = p.get("label").and_then(|v| v.as_str()).unwrap_or("");
                if (title.contains("Interactive Diff")
                    || label.contains("Interactive Diff")
                    || title.contains("herdr-interactive-diff"))
                    && !p_id.is_empty()
                {
                    let _ = send_herdr_rpc(&socket_path, "pane.focus", serde_json::json!({ "pane_id": p_id }));
                    return Ok(());
                }
            }
        }
    }

    let focused_ref = focused_pane_id.as_deref().unwrap_or("w1:p1");

    // 2. Query pane layout to find the leftmost pane (x=0) in the current tab
    let layout_resp = send_herdr_rpc(&socket_path, "pane.layout", serde_json::json!({
        "pane_id": focused_ref
    }));

    let layout_panes = layout_resp
        .as_ref()
        .and_then(|r| r.get("result"))
        .and_then(|r| r.get("layout"))
        .and_then(|l| l.get("panes"))
        .and_then(|p| p.as_array());

    let leftmost_pane_id = layout_panes
        .and_then(|arr| {
            arr.iter().min_by_key(|p| {
                p.get("rect")
                    .and_then(|r| r.get("x"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
            })
        })
        .and_then(|p| p.get("pane_id"))
        .and_then(|v| v.as_str())
        .unwrap_or(focused_ref);

    // 3. Open plugin pane in split to the right of the leftmost pane
    let open_resp = send_herdr_rpc(
        &socket_path,
        "plugin.pane.open",
        serde_json::json!({
            "plugin_id": "herdr-interactive-diff",
            "entrypoint": "diff",
            "placement": "split",
            "target_pane_id": leftmost_pane_id,
            "direction": "right",
            "focus": true
        }),
    )
    .ok_or_else(|| "Failed to execute plugin.pane.open via Herdr socket".to_string())?;

    if let Some(err) = open_resp.get("error") {
        return Err(format!("Herdr error opening plugin pane: {:?}", err));
    }

    let new_pane_id = extract_plugin_pane_id(&open_resp)
        .ok_or_else(|| format!("plugin.pane.open did not return a pane ID: {:?}", open_resp))?;

    // 4. Swap new pane with the leftmost pane so new pane becomes the leftmost (x=0)!
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.swap",
        serde_json::json!({
            "source_pane_id": &new_pane_id,
            "target_pane_id": leftmost_pane_id
        }),
    );

    // 5. Ensure the new pane has focus
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.focus",
        serde_json::json!({
            "pane_id": &new_pane_id
        }),
    );

    // 6. Release any agent authority on the new pane so it is never treated as an AI agent in Herdr
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.clear_agent_authority",
        serde_json::json!({
            "pane_id": &new_pane_id,
            "source": "herdr-interactive-diff",
        }),
    );
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.report_metadata",
        serde_json::json!({
            "pane_id": &new_pane_id,
            "source": "herdr-interactive-diff",
            "clear_display_agent": true,
            "clear_state_labels": true,
        }),
    );

    Ok(())
}

/// Opens herdr-interactive-diff in a new tab in the current workspace.
/// If interactive diff is already open in the current workspace, focuses it instead of opening a duplicate.
pub fn open_tab(socket_path_opt: Option<&Path>) -> Result<(), String> {
    let socket_path = match socket_path_opt {
        Some(p) => p.to_path_buf(),
        None => resolve_socket_path().ok_or_else(|| "Could not find Herdr socket".to_string())?,
    };

    // 1. Query current session snapshot
    let snap_resp = send_herdr_rpc(&socket_path, "session.snapshot", serde_json::json!({}))
        .ok_or_else(|| "Failed to query Herdr session snapshot".to_string())?;

    let snapshot = snap_resp.get("result").and_then(|r| r.get("snapshot"));
    let focused_pane_id = snapshot
        .and_then(|s| s.get("focused_pane_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let panes = snapshot
        .and_then(|s| s.get("panes"))
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    let tabs = snapshot
        .and_then(|s| s.get("tabs"))
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();

    // Determine current workspace from focused pane (e.g. "w1W:pA" -> "w1W")
    let current_ws = focused_pane_id
        .as_ref()
        .and_then(|id| id.split(':').next());

    // Check if an interactive diff pane is already open in the current workspace
    for p in &panes {
        let p_id = p.get("pane_id").and_then(|v| v.as_str()).unwrap_or("");
        let title = p.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let label = p.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let p_tab_id = p.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
        let ws_match = current_ws.map(|ws| p_id.starts_with(ws)).unwrap_or(true);

        if ws_match
            && (title.contains("Interactive Diff")
                || label.contains("Interactive Diff")
                || title.contains("herdr-interactive-diff"))
            && !p_id.is_empty()
        {
            // Check if this pane is in a dedicated tab (pane_count == 1)
            let is_dedicated_tab = tabs.iter().any(|t| {
                t.get("tab_id").and_then(|v| v.as_str()) == Some(p_tab_id)
                    && t.get("pane_count").and_then(|v| v.as_u64()).unwrap_or(0) == 1
            });

            if is_dedicated_tab {
                // Focus the existing dedicated tab and pane
                if !p_tab_id.is_empty() {
                    let _ = send_herdr_rpc(&socket_path, "tab.focus", serde_json::json!({ "tab_id": p_tab_id }));
                }
                let _ = send_herdr_rpc(&socket_path, "pane.focus", serde_json::json!({ "pane_id": p_id }));
                return Ok(());
            } else {
                // It was open in a split; close the obsolete split pane so it transitions cleanly to a dedicated tab
                let _ = send_herdr_rpc(&socket_path, "pane.close", serde_json::json!({ "pane_id": p_id }));
            }
        }
    }

    // 2. Open plugin pane in tab placement via Herdr plugin runtime
    let open_resp = send_herdr_rpc(
        &socket_path,
        "plugin.pane.open",
        serde_json::json!({
            "plugin_id": "herdr-interactive-diff",
            "entrypoint": "diff-tab",
            "placement": "tab",
            "focus": true
        }),
    )
    .ok_or_else(|| "Failed to execute plugin.pane.open via Herdr socket".to_string())?;

    let (new_pane_id, new_tab_id) = if let Some(id) = extract_plugin_pane_id(&open_resp) {
        (id, extract_plugin_tab_id(&open_resp))
    } else {
        // Fallback: try entrypoint "diff" with placement "tab"
        let fallback_resp = send_herdr_rpc(
            &socket_path,
            "plugin.pane.open",
            serde_json::json!({
                "plugin_id": "herdr-interactive-diff",
                "entrypoint": "diff",
                "placement": "tab",
                "focus": true
            }),
        );
        match fallback_resp.as_ref().and_then(extract_plugin_pane_id) {
            Some(id) => {
                let tab_id = fallback_resp.as_ref().and_then(extract_plugin_tab_id);
                (id, tab_id)
            }
            None => {
                if let Some(err) = open_resp.get("error") {
                    return Err(format!("Herdr error opening plugin tab: {:?}", err));
                }
                return Err("Failed to obtain pane ID when opening tab".to_string());
            }
        }
    };

    // 3. Ensure both the newly created tab and pane have active focus in Herdr
    if let Some(ref t_id) = new_tab_id {
        let _ = send_herdr_rpc(
            &socket_path,
            "tab.focus",
            serde_json::json!({
                "tab_id": t_id
            }),
        );
    }
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.focus",
        serde_json::json!({
            "pane_id": &new_pane_id
        }),
    );

    // 4. Release any agent authority on the new pane
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.clear_agent_authority",
        serde_json::json!({
            "pane_id": &new_pane_id,
            "source": "herdr-interactive-diff",
        }),
    );
    let _ = send_herdr_rpc(
        &socket_path,
        "pane.report_metadata",
        serde_json::json!({
            "pane_id": &new_pane_id,
            "source": "herdr-interactive-diff",
            "clear_display_agent": true,
            "clear_state_labels": true,
        }),
    );

    Ok(())
}

/// Opens herdr-interactive-diff according to configured placement (Split or Tab)
pub fn open_diff_with_placement(
    socket_path_opt: Option<&Path>,
    placement: crate::config::DiffPlacement,
) -> Result<(), String> {
    match placement {
        crate::config::DiffPlacement::Split => open_split_left(socket_path_opt),
        crate::config::DiffPlacement::Tab => open_tab(socket_path_opt),
    }
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

    #[test]
    fn test_extract_plugin_pane_id() {
        let resp1 = serde_json::json!({
            "result": {
                "plugin_pane": {
                    "pane": {
                        "pane_id": "w1:p3"
                    }
                }
            }
        });
        assert_eq!(extract_plugin_pane_id(&resp1), Some("w1:p3".to_string()));

        let resp2 = serde_json::json!({
            "result": {
                "pane_id": "w2:p5"
            }
        });
        assert_eq!(extract_plugin_pane_id(&resp2), Some("w2:p5".to_string()));
    }

    #[test]
    fn test_extract_plugin_tab_id() {
        let resp1 = serde_json::json!({
            "result": {
                "plugin_pane": {
                    "entrypoint": "diff-tab",
                    "pane": {
                        "pane_id": "w1W:pY",
                        "tab_id": "w1W:tA"
                    }
                }
            }
        });
        assert_eq!(extract_plugin_tab_id(&resp1), Some("w1W:tA".to_string()));

        let resp2 = serde_json::json!({
            "result": {
                "tab": {
                    "tab_id": "w2:t3"
                }
            }
        });
        assert_eq!(extract_plugin_tab_id(&resp2), Some("w2:t3".to_string()));

        let resp3 = serde_json::json!({
            "result": {
                "tab_id": "w3:t7"
            }
        });
        assert_eq!(extract_plugin_tab_id(&resp3), Some("w3:t7".to_string()));
    }

    #[test]
    fn test_detected_agent_deserialization() {
        let json_data = serde_json::json!({
            "agents": [
                {
                    "pane_id": "w1W:p2",
                    "agent": "agy",
                    "agent_status": "working",
                    "cwd": "/path/to/repo"
                },
                {
                    "pane_id": "w1W:p3",
                    "agent": "claude",
                    "agent_status": "idle"
                }
            ]
        });

        let agents: Vec<DetectedAgent> = serde_json::from_value(json_data["agents"].clone()).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].pane_id, "w1W:p2");
        assert_eq!(agents[0].agent, "agy");
        assert_eq!(agents[0].agent_status, "working");
        assert_eq!(agents[0].cwd.as_deref(), Some("/path/to/repo"));
        assert_eq!(agents[1].pane_id, "w1W:p3");
        assert_eq!(agents[1].agent, "claude");
    }
}
