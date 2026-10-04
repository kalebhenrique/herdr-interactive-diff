use std::collections::HashMap;
use std::path::Path;
use crate::ai_engine::{ComplexityLevel, HunkClassification};
use crate::git::{DiffHunk, GitDiff, LineOrigin};
use crate::herdr::HerdrClient;
use crate::markdown_renderer::{ArtifactItem, discover_artifacts};

/// Pergunta pontual feita pelo usuário via binding '?' sobre um trecho do diff
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct PointwiseQuestion {
    pub id: String,
    pub timestamp: String,
    pub file_path: String,
    pub hunk_id: Option<String>,
    pub question: String,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    GitDiff,   // Tab 1: File drawer + Diff / Full File Viewer
    Artifacts, // Tab 2: Gemini / Antigravity Markdown Artifacts viewer
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactPaneFocus {
    FileList,     // Lateral drawer with .md files
    DocumentView, // Document viewer and scroll area
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFocus {
    Antigravity, // Antigravity CLI
    Claude,      // Claude Code
}

impl From<crate::config::AgentKind> for AgentFocus {
    fn from(kind: crate::config::AgentKind) -> Self {
        match kind {
            crate::config::AgentKind::Claude => AgentFocus::Claude,
            _ => AgentFocus::Antigravity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPickerTarget {
    Primary,
    Review,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffPaneFocus {
    FileList, // Focus on file drawer
    CodeView, // Focus on code / diff viewer
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeViewMode {
    Diff,     // Display only changed hunks
    FullFile, // Display full file with highlights
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooltipKind {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone)]
pub struct TooltipData {
    pub title: String,
    pub kind: TooltipKind,
    pub summary: String,
    pub details: String,
    pub hint: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum CodeLineDisplay {
    HunkHeader {
        hunk_id: String,
        header: String,
    },
    CommentHeader {
        hunk_id: String,
        level: ComplexityLevel,
        title: String,
    },
    CommentFooter {
        hunk_id: String,
        level: ComplexityLevel,
        fix_or_hint: String,
    },
    DiffLine {
        hunk_id: String,
        origin: LineOrigin,
        content: String,
        old_lineno: Option<usize>,
        new_lineno: Option<usize>,
        level: ComplexityLevel,
        has_caveman: bool,
        has_curiosity: bool,
        in_comment_block: bool,
    },
    FullFileLine {
        lineno: usize,
        content: String,
        is_changed: bool,
        origin: Option<LineOrigin>,
        level: ComplexityLevel,
        has_caveman: bool,
        has_curiosity: bool,
        in_comment_block: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HerdrReportSnapshot {
    pub agent: String,
    pub state: crate::herdr::AgentState,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub is_reviewer: bool,
}

#[allow(dead_code)]
pub struct App {
    pub diff: GitDiff,
    pub active_tab: ActiveTab,

    // Decoupled Herdr Agents & Configuration
    pub config: crate::config::PluginConfig,
    pub detected_agents: Vec<crate::herdr::DetectedAgent>,
    pub primary_pane_id: Option<String>,
    pub review_pane_id: Option<String>,

    // Tab 1: Git Diff & File Tree
    pub diff_pane_focus: DiffPaneFocus,
    pub code_view_mode: CodeViewMode,
    pub show_diff_tree: bool,
    pub selected_file_idx: usize,
    pub code_lines: Vec<CodeLineDisplay>,
    pub code_cursor_idx: usize,
    pub code_scroll_offset: usize,

    // Floating Tooltip for Caveman & Insights
    pub show_tooltip: bool,
    pub tooltip_scroll_offset: usize,
    pub show_help: bool,
    pub help_scroll_offset: usize,

    // On-demand AI question about diff
    pub is_asking_question: bool,
    pub question_input: String,

    // Target repository directory path
    pub repo_path: Option<String>,

    // AI Classifications & Review Comments
    pub classifications: HashMap<String, HunkClassification>,
    pub is_classifying: bool,

    // Tab 2: Artifacts
    pub artifacts: Vec<ArtifactItem>,
    pub active_antigravity_conversation_id: Option<String>,
    pub last_agy_pid: Option<u32>,
    pub last_conv_detect_time: std::time::Instant,
    pub show_artifact_tree: bool,
    pub selected_artifact_idx: usize,
    pub artifact_scroll_offset: usize,
    pub artifact_cursor_idx: usize,
    pub artifact_pane_focus: ArtifactPaneFocus,

    // Pointwise questions (?) asked during the session
    pub pointwise_questions: Vec<PointwiseQuestion>,

    // UI Status
    pub status_message: String,
    pub should_quit: bool,

    // Native Herdr Integration
    pub herdr: Option<HerdrClient>,
    pub last_herdr_state: Option<String>,
    pub last_herdr_snapshot: Option<HerdrReportSnapshot>,
    pub last_herdr_report_time: Option<std::time::Instant>,

    // Throttled cache for background transcript syncing and artifacts
    pub last_claude_transcript_mtime: Option<std::time::SystemTime>,
    pub last_claude_transcript_len: u64,
    pub last_artifacts_check_time: std::time::Instant,

    // Herdr Visual Theme & Identity
    pub palette: crate::theme::Palette,

    // AI Picker Modal (Ctrl+A)
    pub show_agent_picker: bool,
    pub agent_picker_target: AgentPickerTarget,
    pub agent_picker_idx: usize,

    // Mouse selection & Drag
    pub mouse_drag_start: Option<(u16, u16)>,
    pub mouse_drag_end: Option<(u16, u16)>,
    pub selected_text: Option<String>,

    // Collapsible file tree in Git Diff drawer
    pub collapsed_dirs: std::collections::HashSet<String>,
    pub diff_tree_cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTreeItem {
    Directory {
        path: String,
        name: String,
        is_collapsed: bool,
        file_count: usize,
        depth: usize,
    },
    File {
        file_idx: usize,
        name: String,
        depth: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentNavTarget {
    pub file_idx: usize,
    pub hunk_id: String,
    pub title: String,
}

/// Extracts session ID (UUID) from Herdr's DetectedAgent session metadata
pub fn extract_session_id_from_agent(agent: &crate::herdr::DetectedAgent) -> Option<String> {
    if let Some(session) = &agent.agent_session {
        if let Some(val) = session.get("value").and_then(|v| v.as_str()) {
            if !val.trim().is_empty() {
                return Some(val.to_string());
            }
        }
        if let Some(val) = session.get("id").and_then(|v| v.as_str()) {
            if !val.trim().is_empty() {
                return Some(val.to_string());
            }
        }
        if let Some(s) = session.as_str() {
            if !s.trim().is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Queries Herdr for pane process info and extracts the conversation ID from active presence lock or screen text
pub fn resolve_pane_antigravity_conversation_id(
    herdr: Option<&HerdrClient>,
    pane_id: Option<&str>,
    repo_path: Option<&str>,
) -> Option<String> {
    let (h, p_id) = (herdr?, pane_id?);
    if let Some(proc_resp) = h.call_sync("pane.process_info", serde_json::json!({ "pane_id": p_id })) {
        if let Some(info) = proc_resp.get("result").and_then(|r| r.get("process_info")) {
            let mut pids = Vec::new();
            if let Some(fg) = info.get("foreground_processes").and_then(|v| v.as_array()) {
                for p in fg {
                    if let Some(pid) = p.get("pid").and_then(|v| v.as_u64()) {
                        pids.push(pid as u32);
                    }
                }
            }
            if let Some(sp) = info.get("shell_pid").and_then(|v| v.as_u64()) {
                pids.push(sp as u32);
            }
            for pid in pids {
                if let Some(cid) = crate::markdown_renderer::detect_presence_lock_for_pid(pid) {
                    return Some(cid);
                }
            }
        }
    }

    let screen_text = h.read_pane_text(p_id, 100);
    crate::markdown_renderer::detect_active_antigravity_conversation_id(
        repo_path,
        screen_text.as_deref(),
        None,
    )
}

impl App {
    #[allow(dead_code)]
    pub fn new(diff: GitDiff, repo_path: Option<String>) -> Self {
        Self::new_with_config(diff, repo_path, crate::config::load_config())
    }

    pub fn new_with_config(diff: GitDiff, repo_path: Option<String>, config: crate::config::PluginConfig) -> Self {
        let herdr = if cfg!(test) {
            None
        } else {
            HerdrClient::try_detect()
        };

        let mut detected_agents = Vec::new();
        if let Some(ref h) = herdr {
            detected_agents = h.list_agents();
        }

        let focused_workspace_id = herdr.as_ref().and_then(|h| h.get_focused_workspace_id());

        let primary_pane_id = config
            .primary_pane_id
            .clone()
            .filter(|id| {
                if detected_agents.is_empty() {
                    return true;
                }
                if let Some(ag) = detected_agents.iter().find(|a| &a.pane_id == id) {
                    focused_workspace_id.is_none() || ag.workspace_id.as_deref() == focused_workspace_id.as_deref()
                } else {
                    false
                }
            })
            .or_else(|| {
                let expected = config.primary_agent.as_str().to_lowercase();
                // 1. Prefer agent of expected kind in the focused workspace
                if let Some(ref ws_id) = focused_workspace_id {
                    if let Some(ag) = detected_agents.iter().find(|a| {
                        a.agent.to_lowercase() == expected && a.workspace_id.as_deref() == Some(ws_id.as_str())
                    }) {
                        return Some(ag.pane_id.clone());
                    }
                }
                // 2. Match by repo_path
                if let Some(ref path) = repo_path {
                    let norm_path = std::fs::canonicalize(path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| path.clone());
                    for ag in &detected_agents {
                        if ag.agent.to_lowercase() == expected {
                            if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                                let norm_cwd = std::fs::canonicalize(cwd)
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|_| cwd.to_string());
                                if norm_cwd == norm_path || norm_path.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_path) {
                                    return Some(ag.pane_id.clone());
                                }
                            }
                        }
                    }
                }
                // 3. Fallback to any agent of expected kind
                detected_agents
                    .iter()
                    .find(|a| a.agent.to_lowercase() == expected)
                    .map(|a| a.pane_id.clone())
            });

        let review_pane_id = config
            .review_pane_id
            .clone()
            .filter(|id| {
                if detected_agents.is_empty() {
                    return true;
                }
                if let Some(ag) = detected_agents.iter().find(|a| &a.pane_id == id) {
                    focused_workspace_id.is_none() || ag.workspace_id.as_deref() == focused_workspace_id.as_deref()
                } else {
                    false
                }
            })
            .or_else(|| {
                let expected = config.review_agent.as_str().to_lowercase();
                // 1. Prefer agent of expected kind in the focused workspace
                if let Some(ref ws_id) = focused_workspace_id {
                    if let Some(ag) = detected_agents.iter().find(|a| {
                        a.agent.to_lowercase() == expected && a.workspace_id.as_deref() == Some(ws_id.as_str())
                    }) {
                        return Some(ag.pane_id.clone());
                    }
                }
                // 2. Match by repo_path
                if let Some(ref path) = repo_path {
                    let norm_path = std::fs::canonicalize(path)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| path.clone());
                    for ag in &detected_agents {
                        if ag.agent.to_lowercase() == expected {
                            if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                                let norm_cwd = std::fs::canonicalize(cwd)
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|_| cwd.to_string());
                                if norm_cwd == norm_path || norm_path.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_path) {
                                    return Some(ag.pane_id.clone());
                                }
                            }
                        }
                    }
                }
                // 3. Fallback to any agent of expected kind
                detected_agents
                    .iter()
                    .find(|a| a.agent.to_lowercase() == expected)
                    .map(|a| a.pane_id.clone())
            });

        let mut active_antigravity_conversation_id = None;
        if let Some(ref target_id) = primary_pane_id {
            if let Some(ag) = detected_agents.iter().find(|a| &a.pane_id == target_id) {
                active_antigravity_conversation_id = extract_session_id_from_agent(ag);
            }
            if active_antigravity_conversation_id.is_none() {
                active_antigravity_conversation_id = resolve_pane_antigravity_conversation_id(
                    herdr.as_ref(),
                    Some(target_id),
                    repo_path.as_deref(),
                );
            }
        }
        if active_antigravity_conversation_id.is_none() {
            if let Some(ref ws_id) = focused_workspace_id {
                for ag in &detected_agents {
                    if (ag.agent.to_lowercase() == "agy" || ag.agent.to_lowercase() == "antigravity")
                        && ag.workspace_id.as_deref() == Some(ws_id.as_str())
                    {
                        active_antigravity_conversation_id = extract_session_id_from_agent(ag)
                            .or_else(|| resolve_pane_antigravity_conversation_id(
                                herdr.as_ref(),
                                Some(&ag.pane_id),
                                ag.cwd.as_deref().or(repo_path.as_deref()),
                            ));
                        if active_antigravity_conversation_id.is_some() {
                            break;
                        }
                    }
                }
            }
        }
        if active_antigravity_conversation_id.is_none() {
            if let Some(ref path) = repo_path {
                let norm_path = std::fs::canonicalize(path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.clone());
                for ag in &detected_agents {
                    if ag.agent.to_lowercase() == "agy" || ag.agent.to_lowercase() == "antigravity" {
                        if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                            let norm_cwd = std::fs::canonicalize(cwd)
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_else(|_| cwd.to_string());
                            if norm_cwd == norm_path || norm_path.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_path) {
                                active_antigravity_conversation_id = extract_session_id_from_agent(ag)
                                    .or_else(|| resolve_pane_antigravity_conversation_id(
                                        herdr.as_ref(),
                                        Some(&ag.pane_id),
                                        Some(norm_cwd.as_str()),
                                    ));
                                if active_antigravity_conversation_id.is_some() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        if active_antigravity_conversation_id.is_none() {
            active_antigravity_conversation_id = crate::markdown_renderer::detect_active_antigravity_conversation_id(
                repo_path.as_deref(),
                None,
                None,
            );
        }
        if active_antigravity_conversation_id.is_none() {
            if let Some(ag) = detected_agents.iter().find(|a| a.agent.to_lowercase() == "agy" || a.agent.to_lowercase() == "antigravity") {
                active_antigravity_conversation_id = extract_session_id_from_agent(ag)
                    .or_else(|| resolve_pane_antigravity_conversation_id(
                        herdr.as_ref(),
                        Some(&ag.pane_id),
                        repo_path.as_deref(),
                    ));
            }
        }

        let artifacts = if let Some(ref conv_id) = active_antigravity_conversation_id {
            crate::markdown_renderer::discover_artifacts_for_session(conv_id)
        } else {
            crate::markdown_renderer::discover_artifacts(
                repo_path.as_deref(),
                None,
                None,
            )
        };

        let palette = if let Some(ref custom) = config.custom_theme {
            crate::theme::Palette::from_name(custom).unwrap_or_else(crate::theme::detect_herdr_theme)
        } else {
            crate::theme::detect_herdr_theme()
        };

        let mut app = Self {
            diff,
            active_tab: ActiveTab::GitDiff,
            config,
            detected_agents,
            primary_pane_id,
            review_pane_id,
            diff_pane_focus: DiffPaneFocus::FileList,
            code_view_mode: CodeViewMode::Diff,
            show_diff_tree: true,
            selected_file_idx: 0,
            code_lines: Vec::new(),
            code_cursor_idx: 0,
            code_scroll_offset: 0,
            show_tooltip: false,
            tooltip_scroll_offset: 0,
            show_help: false,
            help_scroll_offset: 0,
            is_asking_question: false,
            question_input: String::new(),
            repo_path,
            classifications: HashMap::new(),
            is_classifying: false,
            artifacts,
            active_antigravity_conversation_id,
            last_agy_pid: None,
            last_conv_detect_time: std::time::Instant::now(),
            show_artifact_tree: true,
            selected_artifact_idx: 0,
            artifact_scroll_offset: 0,
            artifact_cursor_idx: 0,
            artifact_pane_focus: ArtifactPaneFocus::FileList,
            pointwise_questions: Vec::new(),
            status_message: "herdr-interactive-diff ready. [?] Help | [a] Agents | [/] Ask AI | [n/N] Comments".to_string(),
            should_quit: false,
            herdr,
            last_herdr_state: Some("idle".to_string()),
            last_herdr_snapshot: None,
            last_herdr_report_time: Some(std::time::Instant::now()),
            last_claude_transcript_mtime: None,
            last_claude_transcript_len: 0,
            last_artifacts_check_time: std::time::Instant::now(),
            palette,
            show_agent_picker: false,
            agent_picker_target: AgentPickerTarget::Primary,
            agent_picker_idx: 0,
            mouse_drag_start: None,
            mouse_drag_end: None,
            selected_text: None,
            collapsed_dirs: std::collections::HashSet::new(),
            diff_tree_cursor: 0,
        };

        app.rebuild_code_lines();
        app
    }

    /// Cycles through Herdr's 18 themes at runtime
    #[allow(dead_code)]
    pub fn cycle_theme(&mut self) {
        let themes = [
            "catppuccin", "catppuccin-latte", "terminal", "tokyo-night", "tokyo-night-day",
            "dracula", "nord", "gruvbox", "gruvbox-light", "one-dark", "one-light",
            "solarized", "solarized-light", "kanagawa", "kanagawa-lotus", "rose-pine",
            "rose-pine-dawn", "vesper"
        ];
        let current_theme = self.config.custom_theme.as_deref().unwrap_or("catppuccin");
        let next_idx = match themes.iter().position(|&t| t == current_theme) {
            Some(i) => (i + 1) % themes.len(),
            None => 0,
        };
        let next_name = themes[next_idx];
        if let Some(pal) = crate::theme::Palette::from_name(next_name) {
            self.palette = pal;
            self.config.custom_theme = Some(next_name.to_string());
            let _ = crate::config::save_config(&self.config);
            self.status_message = format!("Theme switched to {}", next_name);
        }
    }

    /// Returns or detects the active Antigravity conversation ID, caching it to avoid loss on resize/splits
    pub fn get_or_detect_active_agy_conversation_id(&mut self) -> Option<String> {
        // 1. First probe primary target agent via Herdr process info & presence lock
        let target_pane = self.find_primary_target();
        if let Some(ref target) = target_pane {
            for ag in &self.detected_agents {
                if &ag.pane_id == target {
                    if let Some(val) = extract_session_id_from_agent(ag) {
                        self.active_antigravity_conversation_id = Some(val.clone());
                        return Some(val);
                    }
                }
            }

            if let Some(conv_id) = resolve_pane_antigravity_conversation_id(
                self.herdr.as_ref(),
                Some(target.as_str()),
                self.repo_path.as_deref(),
            ) {
                self.active_antigravity_conversation_id = Some(conv_id.clone());
                return Some(conv_id);
            }
        }

        // 2. If we already have a cached active conversation ID, verify it exists on disk before returning
        if let Some(ref id) = self.active_antigravity_conversation_id {
            if !id.trim().is_empty() {
                if let Ok(home) = std::env::var("HOME") {
                    let bdir = std::path::PathBuf::from(home).join(".gemini/antigravity-cli/brain").join(id);
                    if bdir.is_dir() {
                        return Some(id.clone());
                    }
                }
            }
        }

        // 2. Check any detected agy agent with an active session ID
        for ag in &self.detected_agents {
            if ag.agent.to_lowercase() == "agy" || ag.agent.to_lowercase() == "antigravity" {
                if let Some(val) = extract_session_id_from_agent(ag) {
                    self.active_antigravity_conversation_id = Some(val.clone());
                    return Some(val);
                }
            }
        }

        // 3. Fallback: inspect screen text and brain transcripts matching repo_path
        let screen_text = self
            .find_primary_target()
            .and_then(|t| self.herdr.as_ref().and_then(|h| h.read_pane_text(&t, 100)));
        let detected = crate::markdown_renderer::detect_active_antigravity_conversation_id(
            self.repo_path.as_deref(),
            screen_text.as_deref(),
            None,
        );
        if let Some(new_id) = detected {
            self.active_antigravity_conversation_id = Some(new_id);
        }
        self.active_antigravity_conversation_id.clone()
    }

    /// Finds AI review classification for a hunk, supporting exact ID, file path without index, or basename matching
    pub fn find_classification_for_hunk(&self, hunk: &DiffHunk) -> Option<&HunkClassification> {
        if let Some(c) = self.classifications.get(&hunk.id) {
            return Some(c);
        }
        let file_part = hunk.id.split('#').next().unwrap_or(&hunk.id);
        if let Some(c) = self.classifications.get(file_part) {
            return Some(c);
        }
        let hunk_base = Path::new(file_part).file_name().and_then(|s| s.to_str()).unwrap_or(file_part);
        for (k, v) in &self.classifications {
            let k_file = k.split('#').next().unwrap_or(k);
            let k_base = Path::new(k_file).file_name().and_then(|s| s.to_str()).unwrap_or(k_file);
            if k_base == hunk_base {
                if let (Some(k_idx), Some(h_idx)) = (k.split('#').nth(1), hunk.id.split('#').nth(1)) {
                    if k_idx == h_idx {
                        return Some(v);
                    }
                } else {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Finds classification by hunk_id string, supporting basename or partial matches
    pub fn find_classification_for_hunk_id(&self, hunk_id: &str) -> Option<&HunkClassification> {
        if let Some(c) = self.classifications.get(hunk_id) {
            return Some(c);
        }
        let file_part = hunk_id.split('#').next().unwrap_or(hunk_id);
        if let Some(c) = self.classifications.get(file_part) {
            return Some(c);
        }
        let base = Path::new(file_part).file_name().and_then(|s| s.to_str()).unwrap_or(file_part);
        for (k, v) in &self.classifications {
            let k_file = k.split('#').next().unwrap_or(k);
            let k_base = Path::new(k_file).file_name().and_then(|s| s.to_str()).unwrap_or(k_file);
            if k_base == base {
                return Some(v);
            }
        }
        None
    }

    /// Rebuilds display lines for the selected file according to CodeViewMode
    pub fn rebuild_code_lines(&mut self) {
        self.code_lines.clear();

        let file = match self.diff.files.get(self.selected_file_idx) {
            Some(f) => f,
            None => return,
        };

        match self.code_view_mode {
            CodeViewMode::Diff => {
                for hunk in &file.hunks {
                    self.code_lines.push(CodeLineDisplay::HunkHeader {
                        hunk_id: hunk.id.clone(),
                        header: hunk.header.clone(),
                    });

                    let (has_comment, level, title_opt, fix_opt) = match self.find_classification_for_hunk(hunk) {
                        Some(c) if c.level != ComplexityLevel::Normal || c.caveman_msg.is_some() || c.explanation.is_some() => {
                            let title = c.caveman_msg.clone()
                                .or_else(|| c.explanation.as_ref().and_then(|e| e.lines().next().map(|s| s.to_string())))
                                .unwrap_or_else(|| "AI Review Finding".to_string());
                            let fix = c.hint.clone()
                                .or_else(|| {
                                    c.explanation.as_ref().map(|e| {
                                        e.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or(e).trim().to_string()
                                    })
                                })
                                .unwrap_or_else(|| "Press [Enter] or [Space] to inspect details in tooltip".to_string());
                            (true, c.level, Some(title), Some(fix))
                        }
                        Some(c) => (false, c.level, None, None),
                        None => (false, ComplexityLevel::Normal, None, None),
                    };

                    if has_comment {
                        self.code_lines.push(CodeLineDisplay::CommentHeader {
                            hunk_id: hunk.id.clone(),
                            level,
                            title: title_opt.unwrap_or_else(|| "AI Review Finding".to_string()),
                        });
                    }

                    let target_line_idx = hunk
                        .lines
                        .iter()
                        .position(|l| l.origin == LineOrigin::Addition)
                        .unwrap_or(0);

                    for (idx, line) in hunk.lines.iter().enumerate() {
                        let is_target = idx == target_line_idx;
                        let line_level = if is_target || line.origin == LineOrigin::Addition {
                            level
                        } else {
                            ComplexityLevel::Normal
                        };

                        self.code_lines.push(CodeLineDisplay::DiffLine {
                            hunk_id: hunk.id.clone(),
                            origin: line.origin,
                            content: line.content.clone(),
                            old_lineno: line.old_lineno,
                            new_lineno: line.new_lineno,
                            level: line_level,
                            has_caveman: is_target && level == ComplexityLevel::Forte,
                            has_curiosity: is_target && level == ComplexityLevel::Curiosidade,
                            in_comment_block: has_comment,
                        });
                    }

                    if has_comment {
                        self.code_lines.push(CodeLineDisplay::CommentFooter {
                            hunk_id: hunk.id.clone(),
                            level,
                            fix_or_hint: fix_opt.unwrap_or_else(|| "Press [Enter] or [Space] to inspect details in tooltip".to_string()),
                        });
                    }
                }
            }
            CodeViewMode::FullFile => {
                if let Some(content) = &file.full_content {
                    let mut added_lines: HashMap<usize, (ComplexityLevel, bool, bool, bool)> = HashMap::new();
                    let mut comment_ranges = Vec::new();

                    for hunk in &file.hunks {
                        let (has_comment, level, title_opt, fix_opt) = match self.find_classification_for_hunk(hunk) {
                            Some(c) if c.level != ComplexityLevel::Normal || c.caveman_msg.is_some() || c.explanation.is_some() => {
                                let title = c.caveman_msg.clone()
                                    .or_else(|| c.explanation.as_ref().and_then(|e| e.lines().next().map(|s| s.to_string())))
                                    .unwrap_or_else(|| "AI Review Finding".to_string());
                                let fix = c.hint.clone()
                                    .or_else(|| {
                                        c.explanation.as_ref().map(|e| {
                                            e.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).unwrap_or(e).trim().to_string()
                                        })
                                    })
                                    .unwrap_or_else(|| "Press [Enter] or [Space] to inspect details in tooltip".to_string());
                                (true, c.level, Some(title), Some(fix))
                            }
                            Some(c) => (false, c.level, None, None),
                            None => (false, ComplexityLevel::Normal, None, None),
                        };

                        let target_line_idx = hunk
                            .lines
                            .iter()
                            .position(|l| l.origin == LineOrigin::Addition)
                            .unwrap_or(0);

                        let mut min_line = None;
                        let mut max_line = None;

                        for (idx, l) in hunk.lines.iter().enumerate() {
                            if let Some(new_no) = l.new_lineno {
                                min_line = Some(min_line.map(|m: usize| m.min(new_no)).unwrap_or(new_no));
                                max_line = Some(max_line.map(|m: usize| m.max(new_no)).unwrap_or(new_no));
                                let is_target = idx == target_line_idx;
                                added_lines.insert(
                                    new_no,
                                    (
                                        level,
                                        is_target && level == ComplexityLevel::Forte,
                                        is_target && level == ComplexityLevel::Curiosidade,
                                        has_comment,
                                    ),
                                );
                            }
                        }

                        if has_comment {
                            if let (Some(start), Some(end), Some(title), Some(fix)) = (min_line, max_line, title_opt, fix_opt) {
                                comment_ranges.push((start, end, hunk.id.clone(), level, title, fix));
                            }
                        }
                    }

                    for (idx, line) in content.lines().enumerate() {
                        let lineno = idx + 1;

                        for (start, _, h_id, lvl, title, _) in &comment_ranges {
                            if *start == lineno {
                                self.code_lines.push(CodeLineDisplay::CommentHeader {
                                    hunk_id: h_id.clone(),
                                    level: *lvl,
                                    title: title.clone(),
                                });
                            }
                        }

                        if let Some(&(lvl, has_caveman, has_curiosity, in_comment)) = added_lines.get(&lineno) {
                            self.code_lines.push(CodeLineDisplay::FullFileLine {
                                lineno,
                                content: line.to_string(),
                                is_changed: true,
                                origin: Some(LineOrigin::Addition),
                                level: lvl,
                                has_caveman,
                                has_curiosity,
                                in_comment_block: in_comment,
                            });
                        } else {
                            self.code_lines.push(CodeLineDisplay::FullFileLine {
                                lineno,
                                content: line.to_string(),
                                is_changed: false,
                                origin: None,
                                level: ComplexityLevel::Normal,
                                has_caveman: false,
                                has_curiosity: false,
                                in_comment_block: false,
                            });
                        }

                        for (_, end, h_id, lvl, _, fix) in &comment_ranges {
                            if *end == lineno {
                                self.code_lines.push(CodeLineDisplay::CommentFooter {
                                    hunk_id: h_id.clone(),
                                    level: *lvl,
                                    fix_or_hint: fix.clone(),
                                });
                            }
                        }
                    }
                } else {
                    self.code_view_mode = CodeViewMode::Diff;
                    self.rebuild_code_lines();
                    return;
                }
            }
        }

        if self.code_cursor_idx >= self.code_lines.len() && !self.code_lines.is_empty() {
            self.code_cursor_idx = self.code_lines.len() - 1;
        }
    }

    /// Selects previous file in drawer
    #[allow(dead_code)]
    pub fn select_prev_file(&mut self) {
        if self.selected_file_idx > 0 {
            self.selected_file_idx -= 1;
            self.code_cursor_idx = 0;
            self.code_scroll_offset = 0;
            self.show_tooltip = false;
            self.tooltip_scroll_offset = 0;
            self.rebuild_code_lines();
        }
    }

    /// Selects next file in drawer
    #[allow(dead_code)]
    pub fn select_next_file(&mut self) {
        if !self.diff.files.is_empty() && self.selected_file_idx + 1 < self.diff.files.len() {
            self.selected_file_idx += 1;
            self.code_cursor_idx = 0;
            self.code_scroll_offset = 0;
            self.show_tooltip = false;
            self.tooltip_scroll_offset = 0;
            self.rebuild_code_lines();
        }
    }

    /// Moves cursor down in code viewer
    #[allow(dead_code)]
    pub fn move_cursor_down(&mut self) {
        if !self.code_lines.is_empty() && self.code_cursor_idx + 1 < self.code_lines.len() {
            self.code_cursor_idx += 1;
            self.show_tooltip = false;
            self.tooltip_scroll_offset = 0;
        }
    }

    /// Moves cursor up in code viewer
    #[allow(dead_code)]
    pub fn move_cursor_up(&mut self) {
        if self.code_cursor_idx > 0 {
            self.code_cursor_idx -= 1;
            self.show_tooltip = false;
            self.tooltip_scroll_offset = 0;
        }
    }

    /// Moves cursor by page
    #[allow(dead_code)]
    pub fn move_cursor_page(&mut self, delta: i32) {
        if delta < 0 {
            self.code_cursor_idx = self.code_cursor_idx.saturating_sub((-delta) as usize);
        } else {
            let max_idx = self.code_lines.len().saturating_sub(1);
            self.code_cursor_idx = (self.code_cursor_idx + delta as usize).min(max_idx);
        }
        self.show_tooltip = false;
        self.tooltip_scroll_offset = 0;
    }

    /// Toggles floating review / curiosity tooltip
    #[allow(dead_code)]
    pub fn toggle_tooltip(&mut self) {
        if self.get_active_tooltip().is_some() {
            self.show_tooltip = !self.show_tooltip;
            self.tooltip_scroll_offset = 0;
        } else {
            self.show_tooltip = false;
        }
    }

    /// Scrolls floating review / curiosity tooltip
    pub fn scroll_tooltip(&mut self, delta: i32) {
        if delta < 0 {
            self.tooltip_scroll_offset = self.tooltip_scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.tooltip_scroll_offset = self.tooltip_scroll_offset.saturating_add(delta as usize);
        }
    }

    /// Returns tooltip content if the line under cursor has review annotations
    pub fn get_active_tooltip(&self) -> Option<TooltipData> {
        let line = self.code_lines.get(self.code_cursor_idx)?;

        let hunk_id = match line {
            CodeLineDisplay::DiffLine { hunk_id, .. } => hunk_id.clone(),
            CodeLineDisplay::HunkHeader { hunk_id, .. } => hunk_id.clone(),
            CodeLineDisplay::CommentHeader { hunk_id, .. } => hunk_id.clone(),
            CodeLineDisplay::CommentFooter { hunk_id, .. } => hunk_id.clone(),
            CodeLineDisplay::FullFileLine { lineno, .. } => {
                let file = self.diff.files.get(self.selected_file_idx)?;
                let mut found_hunk = None;
                for hunk in &file.hunks {
                    for l in &hunk.lines {
                        if l.new_lineno == Some(*lineno) {
                            found_hunk = Some(hunk.id.clone());
                            break;
                        }
                    }
                    if found_hunk.is_some() {
                        break;
                    }
                }
                found_hunk?
            }
        };

        let classification = self.find_classification_for_hunk_id(&hunk_id)?;

        let (title, kind) = match classification.level {
            ComplexityLevel::Forte => ("REVIEW COMMENT [HIGH]".to_string(), TooltipKind::High),
            ComplexityLevel::Media => ("REVIEW COMMENT [MEDIUM]".to_string(), TooltipKind::Medium),
            ComplexityLevel::Curiosidade => ("REVIEW COMMENT [LOW]".to_string(), TooltipKind::Low),
            ComplexityLevel::Normal => {
                if classification.explanation.is_some() || classification.caveman_msg.is_some() {
                    ("REVIEW COMMENT [LOW]".to_string(), TooltipKind::Low)
                } else {
                    return None;
                }
            }
        };

        let summary = classification
            .caveman_msg
            .clone()
            .unwrap_or_else(|| "Complexity alert in this hunk.".to_string());

        let details = classification
            .explanation
            .clone()
            .unwrap_or_else(|| "No detailed explanation provided.".to_string());

        let hint = classification.hint.clone();

        Some(TooltipData {
            title,
            kind,
            summary,
            details,
            hint,
        })
    }

    /// Updates classifications received from AI analysis
    pub fn set_classifications(&mut self, classifications: Vec<HunkClassification>) {
        self.is_classifying = false;
        for c in classifications {
            self.classifications.insert(c.hunk_id.clone(), c);
        }
        self.rebuild_code_lines();
        self.status_message = format!("AI review completed ({} hunks annotated).", self.classifications.len());
    }

    /// Refreshes Git Diff from disk
    pub fn refresh_diff(&mut self) {
        if let Ok(new_diff) = GitDiff::from_local_repo(self.repo_path.as_deref()) {
            self.update_diff(new_diff);
        }
    }

    /// Updates diff when files change on disk
    pub fn update_diff(&mut self, new_diff: GitDiff) {
        self.diff = new_diff;

        // Prune classifications for files that are no longer part of the diff
        let active_files: std::collections::HashSet<String> = self.diff.files.iter().map(|f| f.new_path.clone()).collect();
        self.classifications.retain(|k, _| {
            let file_part = k.split('#').next().unwrap_or(k);
            active_files.iter().any(|f| {
                f == file_part
                    || f.ends_with(&format!("/{}", file_part))
                    || file_part.ends_with(&format!("/{}", f))
            })
        });

        if self.selected_file_idx >= self.diff.files.len() {
            self.selected_file_idx = self.diff.files.len().saturating_sub(1);
        }
        self.diff_tree_cursor = 0;
        self.code_cursor_idx = 0;
        self.code_scroll_offset = 0;
        self.show_tooltip = false;
        self.rebuild_code_lines();
        self.sync_diff_tree_cursor_to_selected_file();

        if self.diff.files.is_empty() {
            self.status_message = "Git diff updated: clean working tree (no changes).".to_string();
        } else {
            self.status_message = format!("Git diff updated: {} file(s) modified.", self.diff.files.len());
        }
    }

    /// Builds a hierarchical tree of changed files in git diff with collapsible directories
    pub fn build_diff_tree(&self) -> Vec<DiffTreeItem> {
        if self.diff.files.is_empty() {
            return Vec::new();
        }

        struct DirBuilder {
            path: String,
            subdirs: std::collections::BTreeMap<String, DirBuilder>,
            files: Vec<(usize, String)>,
        }

        impl DirBuilder {
            fn new(path: String) -> Self {
                Self {
                    path,
                    subdirs: std::collections::BTreeMap::new(),
                    files: Vec::new(),
                }
            }

            fn total_files(&self) -> usize {
                let mut count = self.files.len();
                for sub in self.subdirs.values() {
                    count += sub.total_files();
                }
                count
            }

            fn flatten_into(
                &self,
                collapsed: &std::collections::HashSet<String>,
                depth: usize,
                out: &mut Vec<DiffTreeItem>,
            ) {
                for (dirname, subdir) in &self.subdirs {
                    let is_collapsed = collapsed.contains(&subdir.path);
                    let file_count = subdir.total_files();
                    out.push(DiffTreeItem::Directory {
                        path: subdir.path.clone(),
                        name: dirname.clone(),
                        is_collapsed,
                        file_count,
                        depth,
                    });
                    if !is_collapsed {
                        subdir.flatten_into(collapsed, depth + 1, out);
                    }
                }

                for &(file_idx, ref fname) in &self.files {
                    out.push(DiffTreeItem::File {
                        file_idx,
                        name: fname.clone(),
                        depth,
                    });
                }
            }
        }

        let mut root = DirBuilder::new(String::new());

        for (idx, file) in self.diff.files.iter().enumerate() {
            let path_parts: Vec<&str> = file.new_path.split('/').filter(|s| !s.is_empty()).collect();
            if path_parts.is_empty() {
                continue;
            }
            if path_parts.len() == 1 {
                root.files.push((idx, path_parts[0].to_string()));
            } else {
                let mut current = &mut root;
                let mut accumulated = String::new();
                for &dir in &path_parts[..path_parts.len() - 1] {
                    if !accumulated.is_empty() {
                        accumulated.push('/');
                    }
                    accumulated.push_str(dir);
                    current = current.subdirs.entry(dir.to_string()).or_insert_with(|| {
                        DirBuilder::new(accumulated.clone())
                    });
                }
                current.files.push((idx, path_parts.last().unwrap().to_string()));
            }
        }

        let mut result = Vec::new();
        root.flatten_into(&self.collapsed_dirs, 0, &mut result);
        result
    }

    /// Synchronizes diff tree cursor index to currently selected file
    pub fn sync_diff_tree_cursor_to_selected_file(&mut self) {
        let tree = self.build_diff_tree();
        for (idx, item) in tree.iter().enumerate() {
            if let DiffTreeItem::File { file_idx, .. } = item {
                if *file_idx == self.selected_file_idx {
                    self.diff_tree_cursor = idx;
                    break;
                }
            }
        }
    }

    /// Moves cursor down in file tree drawer
    pub fn diff_tree_next(&mut self) {
        let tree = self.build_diff_tree();
        if tree.is_empty() {
            return;
        }
        if self.diff_tree_cursor + 1 < tree.len() {
            self.diff_tree_cursor += 1;
            if let Some(DiffTreeItem::File { file_idx, .. }) = tree.get(self.diff_tree_cursor) {
                if *file_idx != self.selected_file_idx {
                    self.selected_file_idx = *file_idx;
                    self.code_cursor_idx = 0;
                    self.code_scroll_offset = 0;
                    self.rebuild_code_lines();
                }
            }
        }
    }

    /// Moves cursor up in file tree drawer
    pub fn diff_tree_prev(&mut self) {
        let tree = self.build_diff_tree();
        if tree.is_empty() {
            return;
        }
        if self.diff_tree_cursor > 0 {
            self.diff_tree_cursor -= 1;
            if let Some(DiffTreeItem::File { file_idx, .. }) = tree.get(self.diff_tree_cursor) {
                if *file_idx != self.selected_file_idx {
                    self.selected_file_idx = *file_idx;
                    self.code_cursor_idx = 0;
                    self.code_scroll_offset = 0;
                    self.rebuild_code_lines();
                }
            }
        }
    }

    /// Toggles folder collapse state or selects file under cursor in file tree
    pub fn diff_tree_toggle_current(&mut self) {
        let tree = self.build_diff_tree();
        if let Some(item) = tree.get(self.diff_tree_cursor).cloned() {
            match item {
                DiffTreeItem::Directory { path, is_collapsed, .. } => {
                    if is_collapsed {
                        self.collapsed_dirs.remove(&path);
                    } else {
                        self.collapsed_dirs.insert(path);
                    }
                }
                DiffTreeItem::File { file_idx, .. } => {
                    self.selected_file_idx = file_idx;
                    self.rebuild_code_lines();
                }
            }
        }
    }

    /// Expands folder under cursor
    pub fn diff_tree_expand_current(&mut self) {
        let tree = self.build_diff_tree();
        if let Some(DiffTreeItem::Directory { path, .. }) = tree.get(self.diff_tree_cursor) {
            self.collapsed_dirs.remove(path);
        }
    }

    /// Collapses folder under cursor
    pub fn diff_tree_collapse_current(&mut self) {
        let tree = self.build_diff_tree();
        if let Some(DiffTreeItem::Directory { path, .. }) = tree.get(self.diff_tree_cursor) {
            self.collapsed_dirs.insert(path.clone());
        }
    }

    /// Collects all review comment locations across all files in git diff
    pub fn collect_all_comment_targets(&self) -> Vec<CommentNavTarget> {
        let mut targets = Vec::new();
        for (file_idx, file) in self.diff.files.iter().enumerate() {
            for hunk in &file.hunks {
                if let Some(c) = self.find_classification_for_hunk(hunk) {
                    if c.level != ComplexityLevel::Normal || c.caveman_msg.is_some() || c.explanation.is_some() {
                        let title = c.caveman_msg.clone()
                            .or_else(|| c.explanation.as_ref().and_then(|e| e.lines().next().map(|s| s.to_string())))
                            .unwrap_or_else(|| "Review comment".to_string());
                        targets.push(CommentNavTarget {
                            file_idx,
                            hunk_id: hunk.id.clone(),
                            title,
                        });
                    }
                }
            }
        }
        targets
    }

    /// Returns (current_1_based_index, total_comments) if review comments exist
    pub fn current_comment_progress(&self) -> Option<(usize, usize)> {
        let targets = self.collect_all_comment_targets();
        if targets.is_empty() {
            return None;
        }

        let current_hunk = self.current_selected_hunk_id();
        for (idx, t) in targets.iter().enumerate() {
            if t.file_idx == self.selected_file_idx && Some(&t.hunk_id) == current_hunk.as_ref() {
                return Some((idx + 1, targets.len()));
            }
        }

        for (idx, t) in targets.iter().enumerate() {
            if t.file_idx == self.selected_file_idx {
                return Some((idx + 1, targets.len()));
            }
        }

        Some((1, targets.len()))
    }

    /// Jumps cursor to next review comment hunk across diff files (n key)
    pub fn jump_to_next_comment(&mut self) {
        let targets = self.collect_all_comment_targets();
        if targets.is_empty() {
            self.status_message = "No review comments in diff.".to_string();
            return;
        }

        let current_pos = self.current_comment_progress().map(|(cur, _)| cur.saturating_sub(1)).unwrap_or(0);
        let next_idx = (current_pos + 1) % targets.len();
        let target = targets[next_idx].clone();

        if self.selected_file_idx != target.file_idx {
            self.selected_file_idx = target.file_idx;
            self.rebuild_code_lines();
            self.sync_diff_tree_cursor_to_selected_file();
        }

        if let Some(line_idx) = self.code_lines.iter().position(|l| match l {
            CodeLineDisplay::CommentHeader { hunk_id, .. } => hunk_id == &target.hunk_id,
            CodeLineDisplay::HunkHeader { hunk_id, .. } => hunk_id == &target.hunk_id,
            CodeLineDisplay::DiffLine { hunk_id, .. } => hunk_id == &target.hunk_id,
            _ => false,
        }) {
            self.code_cursor_idx = line_idx;
            self.code_scroll_offset = line_idx.saturating_sub(3);
        }

        self.diff_pane_focus = DiffPaneFocus::CodeView;
        self.status_message = format!(
            "Review Comment [{}/{}]: {} • [n/N to navigate]",
            next_idx + 1,
            targets.len(),
            target.title
        );
    }

    /// Jumps cursor to previous review comment hunk across diff files (N key)
    pub fn jump_to_prev_comment(&mut self) {
        let targets = self.collect_all_comment_targets();
        if targets.is_empty() {
            self.status_message = "No review comments in diff.".to_string();
            return;
        }

        let current_pos = self.current_comment_progress().map(|(cur, _)| cur.saturating_sub(1)).unwrap_or(0);
        let prev_idx = if current_pos > 0 { current_pos - 1 } else { targets.len() - 1 };
        let target = targets[prev_idx].clone();

        if self.selected_file_idx != target.file_idx {
            self.selected_file_idx = target.file_idx;
            self.rebuild_code_lines();
            self.sync_diff_tree_cursor_to_selected_file();
        }

        if let Some(line_idx) = self.code_lines.iter().position(|l| match l {
            CodeLineDisplay::CommentHeader { hunk_id, .. } => hunk_id == &target.hunk_id,
            CodeLineDisplay::HunkHeader { hunk_id, .. } => hunk_id == &target.hunk_id,
            CodeLineDisplay::DiffLine { hunk_id, .. } => hunk_id == &target.hunk_id,
            _ => false,
        }) {
            self.code_cursor_idx = line_idx;
            self.code_scroll_offset = line_idx.saturating_sub(3);
        }

        self.diff_pane_focus = DiffPaneFocus::CodeView;
        self.status_message = format!(
            "Review Comment [{}/{}]: {} • [n/N to navigate]",
            prev_idx + 1,
            targets.len(),
            target.title
        );
    }

    /// Checks whether two agent panels should be displayed simultaneously
    #[allow(dead_code)]
    pub fn is_dual_agent_active(&self) -> bool {
        false
    }

    /// Returns the target identifier (pane ID or agent name) for Primary AI
    pub fn find_primary_target(&self) -> Option<String> {
        let focused_ws = self.herdr.as_ref().and_then(|h| h.get_focused_workspace_id());

        // 1. Explicit configured primary pane (if it matches focused workspace or if workspace is unknown)
        if let Some(id) = self.primary_pane_id.as_ref().or(self.config.primary_pane_id.as_ref()) {
            if !id.trim().is_empty() {
                if let Some(ag) = self.detected_agents.iter().find(|a| &a.pane_id == id) {
                    if focused_ws.is_none() || ag.workspace_id.as_deref() == focused_ws.as_deref() {
                        return Some(id.clone());
                    }
                }
            }
        }

        let expected = self.config.primary_agent.as_str().to_lowercase();

        // 2. Match agent in focused workspace
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = self.detected_agents.iter().find(|a| {
                a.agent.to_lowercase() == expected && a.workspace_id.as_deref() == Some(ws_id.as_str())
            }) {
                return Some(ag.pane_id.clone());
            }
        }

        // 3. Match by repo_path
        if let Some(ref path) = self.repo_path {
            let norm_path = std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.clone());
            for ag in &self.detected_agents {
                if ag.agent.to_lowercase() == expected {
                    if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                        let norm_cwd = std::fs::canonicalize(cwd).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| cwd.to_string());
                        if norm_cwd == norm_path || norm_path.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_path) {
                            return Some(ag.pane_id.clone());
                        }
                    }
                }
            }
        }

        // 4. Any agent in current focused workspace
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = self.detected_agents.iter().find(|a| a.workspace_id.as_deref() == Some(ws_id.as_str())) {
                return Some(ag.pane_id.clone());
            }
        }

        // 5. Configured pane ID if valid
        if let Some(id) = self.primary_pane_id.as_ref().or(self.config.primary_pane_id.as_ref()) {
            if !id.trim().is_empty() && self.detected_agents.iter().any(|a| &a.pane_id == id) {
                return Some(id.clone());
            }
        }

        // 6. Match any agent of requested kind
        for ag in &self.detected_agents {
            if ag.agent.to_lowercase() == expected {
                return Some(ag.pane_id.clone());
            }
        }
        self.detected_agents
            .first()
            .map(|a| a.pane_id.clone())
            .or_else(|| Some(self.config.primary_agent.as_str().to_string()))
    }

    /// Returns the target identifier (pane ID or agent name) for Review AI
    pub fn find_review_target(&self) -> Option<String> {
        let focused_ws = self.herdr.as_ref().and_then(|h| h.get_focused_workspace_id());

        // 1. Explicit configured review pane
        if let Some(id) = self.review_pane_id.as_ref().or(self.config.review_pane_id.as_ref()) {
            if !id.trim().is_empty() {
                if let Some(ag) = self.detected_agents.iter().find(|a| &a.pane_id == id) {
                    if focused_ws.is_none() || ag.workspace_id.as_deref() == focused_ws.as_deref() {
                        return Some(id.clone());
                    }
                }
            }
        }

        let expected = self.config.review_agent.as_str().to_lowercase();

        // 2. Match agent in focused workspace
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = self.detected_agents.iter().find(|a| {
                a.agent.to_lowercase() == expected && a.workspace_id.as_deref() == Some(ws_id.as_str())
            }) {
                return Some(ag.pane_id.clone());
            }
        }

        // 3. Match by repo_path
        if let Some(ref path) = self.repo_path {
            let norm_path = std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.clone());
            for ag in &self.detected_agents {
                if ag.agent.to_lowercase() == expected {
                    if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                        let norm_cwd = std::fs::canonicalize(cwd).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| cwd.to_string());
                        if norm_cwd == norm_path || norm_path.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_path) {
                            return Some(ag.pane_id.clone());
                        }
                    }
                }
            }
        }

        // 4. Any agent in current focused workspace
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = self.detected_agents.iter().find(|a| a.workspace_id.as_deref() == Some(ws_id.as_str())) {
                return Some(ag.pane_id.clone());
            }
        }

        // 5. Configured pane ID if valid
        if let Some(id) = self.review_pane_id.as_ref().or(self.config.review_pane_id.as_ref()) {
            if !id.trim().is_empty() && self.detected_agents.iter().any(|a| &a.pane_id == id) {
                return Some(id.clone());
            }
        }

        // 6. Match any agent of requested kind
        for ag in &self.detected_agents {
            if ag.agent.to_lowercase() == expected {
                return Some(ag.pane_id.clone());
            }
        }
        if self.detected_agents.len() > 1 {
            return Some(self.detected_agents[1].pane_id.clone());
        }
        Some(self.config.review_agent.as_str().to_string())
    }

    /// Refreshes the list of active agents detected by Herdr
    pub fn refresh_detected_agents(&mut self) {
        if let Some(h) = &self.herdr {
            self.detected_agents = h.list_agents();
        }
    }

    /// Scrolls the Help modal content up or down
    pub fn scroll_help(&mut self, delta: isize) {
        if delta < 0 {
            self.help_scroll_offset = self.help_scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.help_scroll_offset = self.help_scroll_offset.saturating_add(delta as usize);
        }
    }

    /// Opens AI Picker Modal (Ctrl+A)
    pub fn open_agent_picker(&mut self) {
        self.show_agent_picker = true;
        self.refresh_detected_agents();
        self.agent_picker_idx = 0;
        self.status_message = "Select AI Agent from Herdr [↑/↓ Navigate | Tab Switch Role | Enter Select | r Refresh | Esc Close]".to_string();
    }

    /// Closes AI Picker Modal
    pub fn close_agent_picker(&mut self) {
        self.show_agent_picker = false;
    }

    /// Switches between Primary and Review target in AI Picker Modal
    pub fn toggle_agent_picker_target(&mut self) {
        self.agent_picker_target = match self.agent_picker_target {
            AgentPickerTarget::Primary => AgentPickerTarget::Review,
            AgentPickerTarget::Review => AgentPickerTarget::Primary,
        };
        self.agent_picker_idx = 0;
    }

    /// Selects next AI in list
    pub fn agent_picker_next(&mut self) {
        let total = self.detected_agents.len();
        if total > 0 {
            self.agent_picker_idx = (self.agent_picker_idx + 1) % total;
        }
    }

    /// Selects previous AI in list
    pub fn agent_picker_prev(&mut self) {
        let total = self.detected_agents.len();
        if total > 0 {
            if self.agent_picker_idx > 0 {
                self.agent_picker_idx -= 1;
            } else {
                self.agent_picker_idx = total - 1;
            }
        }
    }

    /// Confirms AI selection for current target and saves to config
    pub fn confirm_agent_picker(&mut self) {
        if let Some(agent) = self.detected_agents.get(self.agent_picker_idx).cloned() {
            let parsed_kind = crate::config::AgentKind::parse(&agent.agent).unwrap_or(crate::config::AgentKind::Agy);
            match self.agent_picker_target {
                AgentPickerTarget::Primary => {
                    self.config.primary_agent = parsed_kind;
                    self.config.primary_pane_id = Some(agent.pane_id.clone());
                    self.primary_pane_id = Some(agent.pane_id.clone());

                    if let Some(ref cwd) = agent.cwd {
                        self.repo_path = Some(cwd.clone());
                    }

                    // Reset cached conversation ID so we re-detect for the newly selected agent
                    self.active_antigravity_conversation_id = None;

                    let mut conv_id = extract_session_id_from_agent(&agent);

                    if conv_id.is_none() && parsed_kind == crate::config::AgentKind::Agy {
                        conv_id = resolve_pane_antigravity_conversation_id(
                            self.herdr.as_ref(),
                            Some(&agent.pane_id),
                            agent.cwd.as_deref().or(self.repo_path.as_deref()),
                        );
                    }

                    if let Some(id) = conv_id {
                        self.active_antigravity_conversation_id = Some(id);
                    } else if parsed_kind != crate::config::AgentKind::Agy {
                        self.active_antigravity_conversation_id = None;
                    }

                    self.refresh_artifacts();
                    let _ = crate::config::save_config(&self.config);

                    if parsed_kind == crate::config::AgentKind::Agy {
                        self.active_tab = ActiveTab::Artifacts;
                        if !self.artifacts.is_empty() {
                            self.status_message = format!(
                                "Primary AI set to {} [{}]. Loaded {} artifact(s).",
                                agent.agent, agent.pane_id, self.artifacts.len()
                            );
                        } else {
                            self.status_message = format!(
                                "Primary AI set to {} [{}]. Switched to [2] Artifacts.",
                                agent.agent, agent.pane_id
                            );
                        }
                    } else {
                        self.status_message = format!("Primary AI set to {} [{}].", agent.agent, agent.pane_id);
                    }
                }
                AgentPickerTarget::Review => {
                    self.config.review_agent = parsed_kind;
                    self.config.review_pane_id = Some(agent.pane_id.clone());
                    self.review_pane_id = Some(agent.pane_id.clone());
                    let _ = crate::config::save_config(&self.config);
                    self.status_message = format!("Review AI set to {} [{}].", agent.agent, agent.pane_id);
                }
            }
        }
        self.show_agent_picker = false;
    }

    /// Triggers automated 5-lens review on the decoupled Review AI in Herdr
    pub fn trigger_review(&mut self) {
        let prompt = crate::ai_engine::build_claude_review_prompt(&self.diff.raw);
        let _ = crate::clipboard::copy_to_clipboard(&prompt);

        let target = self.find_review_target().unwrap_or_else(|| self.config.review_agent.as_str().to_string());
        if let Some(h) = &self.herdr {
            match h.submit_agent_prompt(&target, &prompt) {
                Ok(_) => {
                    let _ = h.show_notification(&format!("5-lens review submitted to {}!", target));
                    self.status_message = format!("5-lens automated review submitted to Review AI ({})!", target);
                }
                Err(err) => {
                    self.status_message = format!("Review prompt copied to clipboard (submission error: {})", err);
                }
            }
        } else {
            self.status_message = "Review prompt copied to clipboard (Herdr socket not active).".to_string();
        }
    }

    /// Backward-compatible alias for trigger_review
    #[allow(dead_code)]
    pub fn trigger_claude_review(&mut self) {
        self.trigger_review();
    }

    /// Syncs review comments emitted by Review Agent into Git Diff
    pub fn sync_review_to_diff(&mut self) -> bool {
        let review_text = match self.config.review_agent {
            crate::config::AgentKind::Claude => {
                match crate::markdown_renderer::read_latest_claude_session_text_if_modified(
                    self.repo_path.as_deref(),
                    &mut self.last_claude_transcript_mtime,
                    &mut self.last_claude_transcript_len,
                ) {
                    crate::markdown_renderer::ClaudeTranscriptResult::Unchanged => return false,
                    crate::markdown_renderer::ClaudeTranscriptResult::Modified(text) => {
                        text.or_else(|| {
                            let rev_target = self.find_review_target()?;
                            self.herdr.as_ref()?.read_pane_text(&rev_target, 200)
                        })
                    }
                    crate::markdown_renderer::ClaudeTranscriptResult::NoSessionFile => {
                        self.find_review_target()
                            .and_then(|t| self.herdr.as_ref().and_then(|h| h.read_pane_text(&t, 200)))
                    }
                }
            }
            _ => {
                self.find_review_target()
                    .and_then(|t| self.herdr.as_ref().and_then(|h| h.read_pane_text(&t, 200)))
            }
        };

        let text = match review_text {
            Some(t) if !t.trim().is_empty() => t,
            _ => return false,
        };

        let new_classifications = crate::ai_engine::extract_classifications_from_review(&text, &self.diff);
        if new_classifications.is_empty() {
            return false;
        }

        let mut updated = false;
        for c in new_classifications {
            if let Some(existing) = self.classifications.get(&c.hunk_id) {
                if existing.level != c.level || existing.caveman_msg != c.caveman_msg {
                    self.classifications.insert(c.hunk_id.clone(), c);
                    updated = true;
                }
            } else {
                self.classifications.insert(c.hunk_id.clone(), c);
                updated = true;
            }
        }

        if updated {
            self.rebuild_code_lines();
            let agent_name = self.config.review_agent.display_name();
            self.status_message = format!(
                "{} review comments synced to Git Diff ({} hunks annotated).",
                agent_name,
                self.classifications.len()
            );
        }

        updated
    }

    /// Backward-compatible alias for sync_review_to_diff
    pub fn sync_claude_review_to_diff(&mut self) -> bool {
        self.sync_review_to_diff()
    }

    /// Triggers anti-overengineering validation on Primary Agent in Herdr
    pub fn trigger_validation(&mut self) {
        self.sync_review_to_diff();

        let review_text = match self.config.review_agent {
            crate::config::AgentKind::Claude => {
                crate::markdown_renderer::read_latest_claude_session_text(self.repo_path.as_deref())
                    .or_else(|| {
                        let rev_target = self.find_review_target()?;
                        self.herdr.as_ref()?.read_pane_text(&rev_target, 200)
                    })
                    .unwrap_or_default()
            }
            _ => {
                self.find_review_target()
                    .and_then(|target| self.herdr.as_ref()?.read_pane_text(&target, 200))
                    .unwrap_or_default()
            }
        };

        let cleaned_review_text: String = review_text
            .lines()
            .map(|l| l.trim_end())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let context = if !cleaned_review_text.trim().is_empty() {
            cleaned_review_text
        } else {
            format!("(Reference diff)\n{}", self.diff.raw)
        };

        let prompt = crate::ai_engine::build_antigravity_validation_prompt(&context);
        let _ = crate::clipboard::copy_to_clipboard(&prompt);

        let primary_target = self.find_primary_target().unwrap_or_else(|| self.config.primary_agent.as_str().to_string());
        if let Some(h) = &self.herdr {
            match h.submit_agent_prompt(&primary_target, &prompt) {
                Ok(_) => {
                    let _ = h.show_notification(&format!("Validation submitted to {}!", primary_target));
                    self.status_message = format!("Review validation submitted to Primary AI ({})!", primary_target);
                }
                Err(err) => {
                    self.status_message = format!("Validation prompt copied to clipboard (submission error: {})", err);
                }
            }
        } else {
            self.status_message = "Validation prompt copied to clipboard (Herdr socket not active).".to_string();
        }
    }

    /// Backward-compatible alias for trigger_validation
    #[allow(dead_code)]
    pub fn trigger_antigravity_validation(&mut self) {
        self.trigger_validation();
    }

    /// Dynamically updates status in Herdr with state deduplication
    pub fn update_herdr_state(&mut self) {
        if let Some(last) = self.last_herdr_report_time {
            if last.elapsed() < std::time::Duration::from_millis(1500) {
                return;
            }
        }
        if let Some(h) = &self.herdr {
            let agents = h.list_agents();
            if !agents.is_empty() {
                self.detected_agents = agents;
            }
        }
        self.last_herdr_report_time = Some(std::time::Instant::now());
    }

    /// Cleans up Herdr registrations when exiting
    pub fn cleanup(&mut self) {
        if let Some(h) = &mut self.herdr {
            let _ = h.call_sync("pane.clear_agent_authority", serde_json::json!({
                "pane_id": &h.pane_id,
                "source": "herdr-interactive-diff",
            }));
            let _ = h.call_sync("pane.report_metadata", serde_json::json!({
                "pane_id": &h.pane_id,
                "source": "herdr-interactive-diff",
                "clear_display_agent": true,
                "clear_state_labels": true,
            }));
        }
    }

    /// Gets context text for selected diff hunk
    pub fn get_selected_hunk_context(&self) -> String {
        let file = match self.diff.files.get(self.selected_file_idx) {
            Some(f) => f,
            None => return String::new(),
        };

        if let Some(line) = self.code_lines.get(self.code_cursor_idx) {
            match line {
                CodeLineDisplay::DiffLine { hunk_id, .. }
                | CodeLineDisplay::HunkHeader { hunk_id, .. }
                | CodeLineDisplay::CommentHeader { hunk_id, .. }
                | CodeLineDisplay::CommentFooter { hunk_id, .. } => {
                    if let Some(hunk) = file.hunks.iter().find(|h| &h.id == hunk_id) {
                        return hunk.to_diff_text();
                    }
                }
                CodeLineDisplay::FullFileLine { lineno, content, .. } => {
                    let start = lineno.saturating_sub(4);
                    let end = lineno + 4;
                    return format!("Line {}: {}\n(Approximate context lines {}-{})", lineno, content, start, end);
                }
            }
        }

        if let Some(first_hunk) = file.hunks.first() {
            first_hunk.to_diff_text()
        } else {
            format!("File: {}", file.new_path)
        }
    }

    /// Submits question about selected diff hunk directly to Primary AI in Herdr
    pub fn submit_question_to_agent(&mut self) {
        let question = self.question_input.trim().to_string();
        if question.is_empty() {
            self.is_asking_question = false;
            return;
        }

        let file_path = self
            .diff
            .files
            .get(self.selected_file_idx)
            .map(|f| f.new_path.clone())
            .unwrap_or_else(|| "file".to_string());

        let context = self.get_selected_hunk_context();
        let prompt = format!(
            "Question regarding change in `{}`:\n```diff\n{}\n```\nQuestion: {}\nPlease provide a concise explanation of the context and impact of this change.",
            file_path, context, question
        );

        let _ = crate::clipboard::copy_to_clipboard(&prompt);

        let primary_target = self.find_primary_target().unwrap_or_else(|| self.config.primary_agent.as_str().to_string());
        if let Some(h) = &self.herdr {
            match h.submit_agent_prompt(&primary_target, &prompt) {
                Ok(_) => {
                    let _ = h.show_notification(&format!("Question sent to Primary AI ({})!", primary_target));
                    self.status_message = format!("Question submitted to Primary AI ({})!", primary_target);
                }
                Err(e) => {
                    self.status_message = format!("Question copied to clipboard (send failed: {})", e);
                }
            }
        } else {
            self.status_message = "Question copied to clipboard (Herdr socket not active).".to_string();
        }

        let current_hunk_id = self.current_selected_hunk_id();
        let pq = PointwiseQuestion {
            id: format!("q-{}", self.pointwise_questions.len() + 1),
            timestamp: crate::markdown_renderer::current_timestamp_str(),
            file_path: file_path.clone(),
            hunk_id: current_hunk_id.clone(),
            question: question.clone(),
            answer: None,
        };
        self.pointwise_questions.push(pq);

        if let Some(ref h_id) = current_hunk_id {
            let entry = self.classifications.entry(h_id.clone()).or_insert_with(|| {
                HunkClassification {
                    hunk_id: h_id.clone(),
                    level: ComplexityLevel::Curiosidade,
                    target_line: None,
                    caveman_msg: Some(format!("󰌵 QUESTION: {}", question)),
                    explanation: Some(format!("Question sent to Primary AI ({}):\n{}", primary_target, question)),
                    hint: None,
                }
            });
            entry.caveman_msg = Some(format!("󰌵 QUESTION: {}", question));
            let prev_expl = entry.explanation.clone().unwrap_or_default();
            if !prev_expl.contains(&question) {
                entry.explanation = Some(format!("{}\n\n󰌵 Question [?]: {}", prev_expl, question));
            }
        }
        self.rebuild_code_lines();

        self.status_message = format!("Question submitted to Primary AI ({})!", primary_target);
        self.question_input.clear();
        self.is_asking_question = false;
    }

    /// Indicates whether active Antigravity session generated artifacts
    pub fn has_artifacts(&self) -> bool {
        !self.artifacts.is_empty()
    }

    /// Selects next artifact in list
    pub fn select_next_artifact(&mut self) {
        if !self.artifacts.is_empty() && self.selected_artifact_idx + 1 < self.artifacts.len() {
            self.selected_artifact_idx += 1;
            self.artifact_scroll_offset = 0;
            self.artifact_cursor_idx = 0;
        }
    }

    /// Selects previous artifact in list
    pub fn select_prev_artifact(&mut self) {
        if self.selected_artifact_idx > 0 {
            self.selected_artifact_idx -= 1;
            self.artifact_scroll_offset = 0;
            self.artifact_cursor_idx = 0;
        }
    }

    /// Scrolls artifact document view
    pub fn scroll_artifact(&mut self, delta: i32) {
        if let Some(artifact) = self.artifacts.get(self.selected_artifact_idx) {
            let max_scroll = artifact.rendered_lines.len().saturating_sub(1);
            if delta < 0 {
                self.artifact_scroll_offset = self.artifact_scroll_offset.saturating_sub((-delta) as usize);
            } else {
                self.artifact_scroll_offset = (self.artifact_scroll_offset + delta as usize).min(max_scroll);
            }
        }
    }

    /// Reloads artifacts from active Antigravity session
    pub fn refresh_artifacts(&mut self) {
        self.last_conv_detect_time = std::time::Instant::now();
        self.refresh_detected_agents();
        if let Some(conv_id) = self.get_or_detect_active_agy_conversation_id() {
            self.artifacts = crate::markdown_renderer::discover_artifacts_for_session(&conv_id);
        } else {
            let screen_text = self
                .find_primary_target()
                .and_then(|t| self.herdr.as_ref().and_then(|h| h.read_pane_text(&t, 100)));
            self.artifacts = discover_artifacts(self.repo_path.as_deref(), screen_text.as_deref(), None);
            if let Some(first) = self.artifacts.first().and_then(|a| a.conversation_id.clone()) {
                self.active_antigravity_conversation_id = Some(first);
            }
        }
        if self.artifacts.is_empty() {
            self.selected_artifact_idx = 0;
            self.status_message = "No artifacts found in active Antigravity session.".to_string();
        } else {
            if self.selected_artifact_idx >= self.artifacts.len() {
                self.selected_artifact_idx = self.artifacts.len().saturating_sub(1);
            }
            self.status_message = format!("Active session artifacts: {} document(s).", self.artifacts.len());
        }
        self.artifact_scroll_offset = 0;
        self.artifact_cursor_idx = 0;
    }

    /// Periodic lightweight check for updated artifacts
    pub fn check_artifacts_update(&mut self) {
        let is_artifacts_tab = self.active_tab == ActiveTab::Artifacts;
        if !is_artifacts_tab && self.last_artifacts_check_time.elapsed() < std::time::Duration::from_millis(3000) {
            return;
        }
        self.last_artifacts_check_time = std::time::Instant::now();

        let screen_text = self
            .find_primary_target()
            .and_then(|t| self.herdr.as_ref().and_then(|h| h.read_pane_text(&t, 100)));
        let has_resume_on_screen = screen_text.as_deref().map(|s| s.contains("resume") || s.contains("Resume")).unwrap_or(false);
        let should_detect = self.active_antigravity_conversation_id.is_none()
            || has_resume_on_screen
            || self.last_conv_detect_time.elapsed() >= std::time::Duration::from_millis(1000);

        let conv_id = if should_detect {
            self.last_conv_detect_time = std::time::Instant::now();
            self.get_or_detect_active_agy_conversation_id()
        } else {
            self.active_antigravity_conversation_id.clone()
        };

        let new_artifacts = if let Some(ref id) = conv_id {
            crate::markdown_renderer::discover_artifacts_for_session(id)
        } else {
            let list = discover_artifacts(self.repo_path.as_deref(), screen_text.as_deref(), None);
            if let Some(first) = list.first().and_then(|a| a.conversation_id.clone()) {
                self.active_antigravity_conversation_id = Some(first);
            }
            list
        };

        let count_changed = new_artifacts.len() != self.artifacts.len();
        let content_changed = new_artifacts.iter().zip(self.artifacts.iter()).any(|(a, b)| {
            a.file_name != b.file_name || a.size_bytes != b.size_bytes
        });

        if count_changed || content_changed {
            self.artifacts = new_artifacts;
            if self.artifacts.is_empty() {
                self.selected_artifact_idx = 0;
            } else if self.selected_artifact_idx >= self.artifacts.len() {
                self.selected_artifact_idx = self.artifacts.len().saturating_sub(1);
            }
        }
    }

    /// Toggles file tree drawer visibility in Git Diff view
    pub fn toggle_diff_tree(&mut self) {
        self.show_diff_tree = !self.show_diff_tree;
        if !self.show_diff_tree {
            self.diff_pane_focus = DiffPaneFocus::CodeView;
        }
        self.status_message = if self.show_diff_tree {
            "File tree drawer shown.".to_string()
        } else {
            "File tree drawer hidden. Press [e] to show.".to_string()
        };
    }

    /// Toggles artifact list drawer visibility in Artifacts view
    pub fn toggle_artifact_tree(&mut self) {
        self.show_artifact_tree = !self.show_artifact_tree;
        if !self.show_artifact_tree {
            self.artifact_pane_focus = ArtifactPaneFocus::DocumentView;
        }
        self.status_message = if self.show_artifact_tree {
            "Artifacts drawer shown.".to_string()
        } else {
            "Artifacts drawer hidden. Press [e] to show.".to_string()
        };
    }

    /// Toggles between Diff Only and Full File view mode
    pub fn toggle_code_view_mode(&mut self) {
        self.code_view_mode = match self.code_view_mode {
            CodeViewMode::Diff => CodeViewMode::FullFile,
            CodeViewMode::FullFile => CodeViewMode::Diff,
        };
        self.code_cursor_idx = 0;
        self.code_scroll_offset = 0;
        self.show_tooltip = false;
        self.rebuild_code_lines();
        let mode_name = match self.code_view_mode {
            CodeViewMode::Diff => "Diff Only",
            CodeViewMode::FullFile => "Full File",
        };
        self.status_message = format!("View mode changed to: {}", mode_name);
    }

    /// Returns hunk under cursor
    #[allow(dead_code)]
    pub fn current_selected_hunk_id(&self) -> Option<String> {
        let line = self.code_lines.get(self.code_cursor_idx)?;
        match line {
            CodeLineDisplay::HunkHeader { hunk_id, .. } => Some(hunk_id.clone()),
            CodeLineDisplay::CommentHeader { hunk_id, .. } => Some(hunk_id.clone()),
            CodeLineDisplay::CommentFooter { hunk_id, .. } => Some(hunk_id.clone()),
            CodeLineDisplay::DiffLine { hunk_id, .. } => Some(hunk_id.clone()),
            _ => {
                let file = self.diff.files.get(self.selected_file_idx)?;
                file.hunks.first().map(|h| h.id.clone())
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_hunk_by_id(&self, target_id: &str) -> Option<&DiffHunk> {
        for file in &self.diff.files {
            for hunk in &file.hunks {
                if hunk.id == target_id {
                    return Some(hunk);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_engine::ClassificationResponse;

    #[test]
    fn test_app_new() {
        let diff = GitDiff::demo();
        let app = App::new_with_config(diff, None, crate::config::PluginConfig::default());
        assert_eq!(app.active_tab, ActiveTab::GitDiff);
        assert_eq!(app.config.primary_agent, crate::config::AgentKind::Agy);
        assert_eq!(app.config.review_agent, crate::config::AgentKind::Claude);
    }

    #[test]
    fn test_launch_claude() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.trigger_claude_review();
        assert!(app.status_message.contains("review") || app.status_message.contains("Review"));
    }

    #[test]
    fn test_toggle_code_view_mode() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert_eq!(app.code_view_mode, CodeViewMode::Diff);

        app.toggle_code_view_mode();
        assert_eq!(app.code_view_mode, CodeViewMode::FullFile);
        assert!(!app.code_lines.is_empty());
    }

    #[test]
    fn test_floating_tooltip_data() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        let demo_classifications = ClassificationResponse::demo();
        app.set_classifications(demo_classifications.classifications);

        let caveman_idx = app
            .code_lines
            .iter()
            .position(|l| matches!(l, CodeLineDisplay::DiffLine { has_caveman: true, .. }));
        assert!(caveman_idx.is_some());

        app.code_cursor_idx = caveman_idx.unwrap();
        let tooltip = app.get_active_tooltip();
        assert!(tooltip.is_some());
        let t = tooltip.unwrap();
        assert_eq!(t.kind, TooltipKind::High);
        assert!(t.summary.contains("PONT NO CHECK"));
    }

    #[test]
    fn test_submit_question_to_agent() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.active_tab = ActiveTab::GitDiff;
        app.is_asking_question = true;
        app.question_input = "Is there a race condition here?".to_string();

        app.submit_question_to_agent();

        assert!(!app.is_asking_question);
        assert!(app.question_input.is_empty());
        assert_eq!(app.active_tab, ActiveTab::GitDiff);
        assert!(app.status_message.contains("Question submitted to Primary AI"));
        assert_eq!(app.pointwise_questions.len(), 1);
        assert_eq!(app.pointwise_questions[0].question, "Is there a race condition here?");
    }

    #[test]
    fn test_artifacts_initialized() {
        let diff = GitDiff::demo();
        let app = App::new(diff, None);
        assert_eq!(app.artifact_pane_focus, ArtifactPaneFocus::FileList);
        assert_eq!(app.has_artifacts(), !app.artifacts.is_empty());
    }

    #[test]
    fn test_update_diff() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert_eq!(app.diff.files.len(), 4);

        let empty_diff = GitDiff::default();
        app.update_diff(empty_diff);
        assert_eq!(app.diff.files.len(), 0);
        assert!(app.status_message.contains("clean working tree"));
    }

    #[test]
    fn test_sync_claude_review_populates_classifications() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert!(app.classifications.is_empty());

        let fake_claude_review = r#"
```json
{
  "classifications": [
    {
      "hunk_id": "src/auth/session.rs#0",
      "level": "forte",
      "caveman_msg": "RAW DEREF POINTER! DANGEROUS!",
      "explanation": "Why: pointer deref without check\nFix: use Option"
    }
  ]
}
```
"#;

        let parsed = crate::ai_engine::extract_classifications_from_review(fake_claude_review, &app.diff);
        assert_eq!(parsed.len(), 1);
        app.set_classifications(parsed);

        assert_eq!(app.classifications.len(), 1);
        let c = app.classifications.get("src/auth/session.rs#0").unwrap();
        assert_eq!(c.level, ComplexityLevel::Forte);
        assert_eq!(c.caveman_msg.as_deref(), Some("RAW DEREF POINTER! DANGEROUS!"));
    }

    #[test]
    fn test_herdr_state_tracking() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert_eq!(app.last_herdr_state.as_deref(), Some("idle"));
        assert!(app.last_herdr_report_time.is_some());

        app.update_herdr_state();
        assert!(app.last_herdr_report_time.is_some());
    }

    #[test]
    fn test_get_active_tooltip_across_hunk_lines() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        let demo_classifications = ClassificationResponse::demo();
        app.set_classifications(demo_classifications.classifications);

        app.code_cursor_idx = 0;
        let t_header = app.get_active_tooltip();
        assert!(t_header.is_some());
        assert_eq!(t_header.unwrap().kind, TooltipKind::High);

        app.code_cursor_idx = 1;
        let t_line = app.get_active_tooltip();
        assert!(t_line.is_some());
        assert_eq!(t_line.unwrap().kind, TooltipKind::High);
    }

    #[test]
    fn test_toggle_trees() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert!(app.show_diff_tree);
        assert!(app.show_artifact_tree);

        app.toggle_diff_tree();
        assert!(!app.show_diff_tree);
        assert_eq!(app.diff_pane_focus, DiffPaneFocus::CodeView);

        app.toggle_diff_tree();
        assert!(app.show_diff_tree);

        app.toggle_artifact_tree();
        assert!(!app.show_artifact_tree);
        assert_eq!(app.artifact_pane_focus, ArtifactPaneFocus::DocumentView);

        app.toggle_artifact_tree();
        assert!(app.show_artifact_tree);
    }

    #[test]
    fn test_trigger_antigravity_validation() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.active_tab = ActiveTab::GitDiff;
        assert_eq!(app.active_tab, ActiveTab::GitDiff);

        app.trigger_antigravity_validation();
        assert!(
            app.status_message.contains("Validation prompt")
                || app.status_message.contains("Primary AI")
        );
    }

    #[test]
    fn test_scroll_help() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert_eq!(app.help_scroll_offset, 0);

        app.scroll_help(5);
        assert_eq!(app.help_scroll_offset, 5);

        app.scroll_help(3);
        assert_eq!(app.help_scroll_offset, 8);

        app.scroll_help(-4);
        assert_eq!(app.help_scroll_offset, 4);

        app.scroll_help(-10);
        assert_eq!(app.help_scroll_offset, 0);
    }

    #[test]
    fn test_app_new_with_claude_primary() {
        let diff = GitDiff::demo();
        let cfg = crate::config::PluginConfig {
            primary_agent: crate::config::AgentKind::Claude,
            review_agent: crate::config::AgentKind::Agy,
            primary_pane_id: Some("w1:p1".to_string()),
            review_pane_id: Some("w1:p2".to_string()),
            custom_theme: None,
            placement: crate::config::DiffPlacement::Split,
        };
        let mut app = App::new_with_config(diff, None, cfg);
        assert_eq!(app.active_tab, ActiveTab::GitDiff);
        assert_eq!(app.config.primary_agent, crate::config::AgentKind::Claude);
        assert_eq!(app.primary_pane_id.as_deref(), Some("w1:p1"));
        assert_eq!(app.review_pane_id.as_deref(), Some("w1:p2"));

        app.trigger_review();
        assert!(app.status_message.contains("review") || app.status_message.contains("Review"));
    }

    #[test]
    fn test_agent_picker_with_detected_agents() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.detected_agents = vec![
            crate::herdr::DetectedAgent {
                terminal_id: None,
                agent: "agy".to_string(),
                agent_status: "working".to_string(),
                workspace_id: None,
                tab_id: None,
                pane_id: "w1:p10".to_string(),
                focused: true,
                cwd: Some("/path/to/repo".to_string()),
                foreground_cwd: None,
                agent_session: None,
            },
            crate::herdr::DetectedAgent {
                terminal_id: None,
                agent: "claude".to_string(),
                agent_status: "idle".to_string(),
                workspace_id: None,
                tab_id: None,
                pane_id: "w1:p11".to_string(),
                focused: false,
                cwd: Some("/path/to/repo".to_string()),
                foreground_cwd: None,
                agent_session: None,
            },
        ];

        app.open_agent_picker();
        assert!(app.show_agent_picker);
        assert_eq!(app.agent_picker_target, AgentPickerTarget::Primary);
        assert_eq!(app.agent_picker_idx, 0);

        // Select second agent as Primary
        app.agent_picker_next();
        assert_eq!(app.agent_picker_idx, 1);
        app.confirm_agent_picker();
        assert_eq!(app.primary_pane_id.as_deref(), Some("w1:p11"));
        assert_eq!(app.config.primary_agent, crate::config::AgentKind::Claude);
        assert!(!app.show_agent_picker);
    }

    #[test]
    fn test_comment_framing_headers_and_footers() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        let demo_classifications = ClassificationResponse::demo();
        app.set_classifications(demo_classifications.classifications);

        // Verify that CommentHeader exists and precedes the DiffLines
        let header_idx = app
            .code_lines
            .iter()
            .position(|l| matches!(l, CodeLineDisplay::CommentHeader { .. }));
        assert!(header_idx.is_some(), "Expected CommentHeader for classified hunk");

        // Verify that CommentFooter exists and follows the DiffLines
        let footer_idx = app
            .code_lines
            .iter()
            .position(|l| matches!(l, CodeLineDisplay::CommentFooter { .. }));
        assert!(footer_idx.is_some(), "Expected CommentFooter for classified hunk");
        assert!(header_idx.unwrap() < footer_idx.unwrap(), "Header must appear before footer");

        // Verify that DiffLines inside the comment block have in_comment_block == true
        let commented_diff_line = app.code_lines[header_idx.unwrap() + 1..footer_idx.unwrap()]
            .iter()
            .find(|l| matches!(l, CodeLineDisplay::DiffLine { in_comment_block: true, .. }));
        assert!(commented_diff_line.is_some(), "Diff lines within comment block must have in_comment_block == true");

        // Verify tooltip works when cursor is directly on CommentHeader
        app.code_cursor_idx = header_idx.unwrap();
        let tooltip = app.get_active_tooltip();
        assert!(tooltip.is_some(), "Tooltip must be accessible from CommentHeader");

        // Verify tooltip works when cursor is directly on CommentFooter
        app.code_cursor_idx = footer_idx.unwrap();
        let tooltip = app.get_active_tooltip();
        assert!(tooltip.is_some(), "Tooltip must be accessible from CommentFooter");
    }

    #[test]
    fn test_active_antigravity_conversation_persistence() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert_eq!(app.active_antigravity_conversation_id, None);

        // Manually set a mock conversation ID
        app.active_antigravity_conversation_id = Some("12345678-1234-1234-1234-123456789abc".to_string());
        app.last_agy_pid = Some(99999);

        // Calling get_or_detect_active_agy_conversation_id preserves the cached ID
        let cached = app.get_or_detect_active_agy_conversation_id();
        assert_eq!(cached, Some("12345678-1234-1234-1234-123456789abc".to_string()));
    }

    #[test]
    fn test_confirm_agent_picker_with_antigravity_session() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.detected_agents = vec![
            crate::herdr::DetectedAgent {
                terminal_id: None,
                agent: "agy".to_string(),
                agent_status: "working".to_string(),
                workspace_id: None,
                tab_id: None,
                pane_id: "w1W:p2".to_string(),
                focused: false,
                cwd: Some("/path/to/repo".to_string()),
                foreground_cwd: None,
                agent_session: Some(serde_json::json!({
                    "agent": "agy",
                    "kind": "id",
                    "source": "herdr:antigravity_cli",
                    "value": "4250756d-9893-4e16-af4f-484d8116e003"
                })),
            },
        ];

        app.open_agent_picker();
        assert_eq!(app.agent_picker_idx, 0);
        app.confirm_agent_picker();

        assert_eq!(app.config.primary_agent, crate::config::AgentKind::Agy);
        assert_eq!(app.primary_pane_id.as_deref(), Some("w1W:p2"));
        assert_eq!(
            app.active_antigravity_conversation_id.as_deref(),
            Some("4250756d-9893-4e16-af4f-484d8116e003")
        );
        assert!(!app.show_agent_picker);
        // Since session 4250756d-... has artifacts on disk, it automatically loads them and switches to Artifacts
        if app.has_artifacts() {
            assert_eq!(app.active_tab, ActiveTab::Artifacts);
        }
    }

    #[test]
    fn test_app_new_with_config_resolves_workspace_agent_and_artifacts() {
        let diff = GitDiff::demo();
        let cfg = crate::config::PluginConfig {
            primary_agent: crate::config::AgentKind::Agy,
            review_agent: crate::config::AgentKind::Claude,
            primary_pane_id: None,
            review_pane_id: None,
            custom_theme: None,
            placement: crate::config::DiffPlacement::Split,
        };
        let app = App::new_with_config(diff, Some("/Users/kaleb/work/herdr-interactive-diff".to_string()), cfg);
        assert_eq!(app.config.primary_agent, crate::config::AgentKind::Agy);
        if let Some(ref conv_id) = app.active_antigravity_conversation_id {
            assert!(!conv_id.is_empty());
            assert!(!app.artifacts.is_empty(), "Artifacts should be loaded for conversation {}", conv_id);
        }
    }
}
