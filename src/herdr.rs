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
#[derive(Clone, Debug)]
pub struct HerdrClient {
    pub socket_path: PathBuf,
    pub pane_id: String,
}

impl HerdrClient {
    /// Attempts to initialize the Herdr client by discovering the Unix socket and current pane ID.
    pub fn try_detect() -> Option<Self> {
        let socket_path = resolve_socket_path()?;
        if !socket_path.exists() {
            return None;
        }

        // 1. Try obtaining pane_id directly from HERDR_PANE_ID env var
        if let Ok(pane_id) = std::env::var("HERDR_PANE_ID") {
            if !pane_id.trim().is_empty() {
                return Some(Self {
                    socket_path,
                    pane_id: pane_id.trim().to_string(),
                });
            }
        }

        // 2. Fallback: Query Herdr socket for the active pane matching current process PID
        let pane_id = discover_pane_id(&socket_path)?;
        Some(Self {
            socket_path,
            pane_id,
        })
    }

    /// Spawns an agent in a new Herdr split pane and starts the agent CLI
    pub fn split_agent(&self, agent: &AgentKind, cwd: Option<&str>) -> Option<String> {
        let seq = next_seq();
        let cmd = agent.command_bin();
        let mut params = serde_json::json!({
            "direction": "right",
            "ratio": 0.5,
        });
        if !self.pane_id.is_empty() {
            params["target_pane_id"] = serde_json::Value::String(self.pane_id.clone());
        }
        if let Some(dir) = cwd {
            params["cwd"] = serde_json::Value::String(dir.to_string());
        }

        let req = serde_json::json!({
            "id": format!("herdr-diff:split:{}", seq),
            "method": "pane.split",
            "params": params,
        });

        let mut created_pane = None;
        if let Some(res) = send_herdr_request(&self.socket_path, &req) {
            if let Some(new_pane_id) = res.pointer("/result/pane/pane_id").and_then(|v| v.as_str()) {
                created_pane = Some(new_pane_id.to_string());
            } else if let Some(new_pane_id) = res.pointer("/result/pane_id").and_then(|v| v.as_str()) {
                created_pane = Some(new_pane_id.to_string());
            }
        }

        // Fallback using herdr CLI if socket didn't return a pane
        let herdr_bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
        if created_pane.is_none() {
            let mut cli_cmd = std::process::Command::new(&herdr_bin);
            if !self.pane_id.is_empty() {
                cli_cmd.args(["pane", "split", &self.pane_id, "--direction", "right"]);
            } else {
                cli_cmd.args(["pane", "split", "--direction", "right"]);
            }
            if let Some(dir) = cwd {
                cli_cmd.args(["--cwd", dir]);
            }
            if let Ok(output) = cli_cmd.output() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(out_str.trim()) {
                    if let Some(id) = json.pointer("/result/pane/pane_id").and_then(|v| v.as_str()) {
                        created_pane = Some(id.to_string());
                    } else if let Some(id) = json.pointer("/result/pane_id").and_then(|v| v.as_str()) {
                        created_pane = Some(id.to_string());
                    }
                }
            }
        }

        // Once pane is created, run the agent command inside it
        if let Some(ref pane) = created_pane {
            let run_req = serde_json::json!({
                "id": format!("herdr-diff:run:{}", next_seq()),
                "method": "pane.run",
                "params": {
                    "pane_id": pane,
                    "command": cmd,
                }
            });
            let _ = send_herdr_request(&self.socket_path, &run_req);

            // Also trigger via CLI as fallback
            let _ = std::process::Command::new(&herdr_bin)
                .args(["pane", "run", pane, cmd])
                .output();
        }

        created_pane
    }

    /// Closes a pane in Herdr
    pub fn close_pane(&self, pane_id: &str) -> bool {
        let seq = next_seq();
        let req = serde_json::json!({
            "id": format!("herdr-diff:close:{}", seq),
            "method": "pane.close",
            "params": {
                "pane_id": pane_id
            }
        });
        if send_herdr_request(&self.socket_path, &req).is_some() {
            return true;
        }

        // Fallback using herdr CLI
        let herdr_bin = std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string());
        let _ = std::process::Command::new(herdr_bin)
            .args(["pane", "close", pane_id])
            .output();
        true
    }

    /// Focuses a pane in Herdr
    #[allow(dead_code)]
    pub fn focus_pane(&self, pane_id: &str) {
        let seq = next_seq();
        let req = serde_json::json!({
            "id": format!("herdr-diff:focus:{}", seq),
            "method": "pane.zoom",
            "params": {
                "pane_id": pane_id,
                "mode": "toggle"
            }
        });
        let _ = send_herdr_request(&self.socket_path, &req);
    }

    /// Reports the active agent status and metadata on Herdr pane without creating duplicate panels
    pub fn report_active_agent(
        &self,
        agent: &str,
        state: AgentState,
        message: Option<&str>,
        session_id: Option<&str>,
        is_reviewer: bool,
    ) {
        let seq = next_seq();
        let agent_kind = AgentKind::parse(agent).unwrap_or(AgentKind::Agy);

        // 1. Title and state_labels formatted specifically for the agent and reviewer status
        let title = if is_reviewer {
            format!("Interactive Diff: {} (Reviewer)", agent_kind.display_name())
        } else {
            format!("Interactive Diff: {}", agent_kind.display_name())
        };

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

        let meta_req = serde_json::json!({
            "id": format!("herdr-diff:meta:{}", seq),
            "method": "pane.report_metadata",
            "params": {
                "pane_id": &self.pane_id,
                "source": "herdr-interactive-diff",
                "display_agent": agent_kind.as_str(),
                "title": title,
                "state_labels": state_labels,
                "seq": seq,
            }
        });
        let _ = send_herdr_request(&self.socket_path, &meta_req);

        let official_source = agent_kind.herdr_source();

        // 2. Report active agent status
        let agent_seq = next_seq();
        let mut params = serde_json::json!({
            "pane_id": &self.pane_id,
            "source": official_source,
            "agent": agent,
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
            "id": format!("weavers:report:{}", agent_seq),
            "method": "pane.report_agent",
            "params": params,
        });
        let _ = send_herdr_request(&self.socket_path, &request);

        // 3. Report active agent session if known
        if let Some(sess) = session_id {
            let sess_seq = next_seq();
            let session_req = serde_json::json!({
                "id": format!("weavers:session:{}", sess_seq),
                "method": "pane.report_agent_session",
                "params": {
                    "pane_id": &self.pane_id,
                    "source": official_source,
                    "agent": agent,
                    "agent_session_id": sess,
                    "seq": sess_seq,
                }
            });
            let _ = send_herdr_request(&self.socket_path, &session_req);
        }
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

    /// Cleanly releases registered agent state on Weavers pane when exiting
    pub fn release_all(&mut self) {
        for (agent, source) in &[("agy", "herdr:antigravity_cli"), ("claude", "herdr:claude"), ("agy", "weavers"), ("claude", "weavers")] {
            let s = next_seq();
            let req = serde_json::json!({
                "id": format!("weavers:release:{}:{}:{}", agent, source, s),
                "method": "pane.release_agent",
                "params": {
                    "pane_id": &self.pane_id,
                    "source": source,
                    "agent": agent,
                    "seq": s,
                }
            });
            let _ = send_herdr_request(&self.socket_path, &req);
        }
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
    match agent_name {
        "claude" => {
            let is_blocked = lower.contains("waiting for permission")
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
                || (lower.contains("[y/n]") || lower.contains("(y/n)"));
            if is_blocked {
                return AgentState::Blocked;
            }
        }
        "agy" => {
            let is_blocked = lower.contains("requesting permission for:")
                || lower.contains("do you want to proceed?")
                || lower.contains("tab amend")
                || lower.contains("edit command")
                || lower.contains("permission required")
                || (lower.contains("[y/n]") || lower.contains("(y/n)"));
            if is_blocked {
                return AgentState::Blocked;
            }
        }
        _ => {}
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
            if bottom_text.contains("esc to interrupt")
                || bottom_text.contains("Thinking…")
                || bottom_text.contains("Thinking...")
                || bottom_text.contains("Waiting for")
                || bottom_text.contains("MCP task") =>
        {
            return AgentState::Working;
        }
        "agy"
            if bottom_text.contains("Thinking")
                || bottom_text.contains("Thought for")
                || bottom_text.contains("Analyzing")
                || bottom_text.contains("Generating")
                || bottom_text.contains("Running command") =>
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
        let client = HerdrClient {
            socket_path: PathBuf::from("/tmp/herdr.sock"),
            pane_id: "test:p1".to_string(),
        };
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
}
