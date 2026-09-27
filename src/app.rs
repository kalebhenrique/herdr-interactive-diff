use std::collections::HashMap;
use std::path::Path;
use crate::ai_engine::{ComplexityLevel, HunkClassification};
use crate::git::{DiffHunk, GitDiff, LineOrigin};
use crate::herdr::HerdrClient;
use crate::markdown_renderer::{ArtifactItem, discover_artifacts};
use crate::terminal_session::TerminalSession;

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
    Agents,    // Tab 1: Interactive Terminal Sessions (Primary + Review Split)
    GitDiff,   // Tab 2: File drawer + Diff / Full File Viewer
    Artifacts, // Tab 3: Gemini / Antigravity Markdown Artifacts viewer
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactPaneFocus {
    FileList,     // Lateral drawer with .md files
    DocumentView, // Document viewer and scroll area
}

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

    // Interactive Terminal Sessions (Tab 1: Agents)
    pub config: crate::config::WeaversConfig,
    pub agent_focus: AgentFocus,
    pub agy_session: Option<TerminalSession>,
    pub claude_session: Option<TerminalSession>,
    pub show_claude: bool, // Claude active status (for backwards compatibility)
    pub show_review_agent: bool,

    // Tab 2: Git Diff & File Tree
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

    // Tab 3: Artifacts
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

    // Herdr Visual Theme & Identity
    pub palette: crate::theme::Palette,

    // Agent Hub & Herdr Splits (Tab 1)
    pub agent_selector_open: bool,
    pub agent_selector_idx: usize,
    pub selecting_for_review: bool,
    pub active_herdr_agent_pane: Option<String>,
    pub active_herdr_review_pane: Option<String>,

    // AI Picker Modal (Ctrl+A)
    pub show_agent_picker: bool,
    pub agent_picker_target: AgentPickerTarget,
    pub agent_picker_idx: usize,

    // Mouse selection & Drag
    pub mouse_drag_start: Option<(u16, u16)>,
    pub mouse_drag_end: Option<(u16, u16)>,
    pub selected_text: Option<String>,
}

impl App {
    #[allow(dead_code)]
    pub fn new(diff: GitDiff, repo_path: Option<String>) -> Self {
        Self::new_with_config(diff, repo_path, crate::config::load_config())
    }

    pub fn new_with_config(diff: GitDiff, repo_path: Option<String>, config: crate::config::WeaversConfig) -> Self {
        let cwd = repo_path.as_deref();

        // Initially only the primary agent is spawned; the review agent is spawned on demand via Ctrl+R.
        // In unit tests, avoid spawning background CLI daemons and connecting to Herdr.
        let mut agy_session = None;
        let mut claude_session = None;

        if !cfg!(test) {
            match config.primary_agent {
                crate::config::AgentKind::Agy => {
                    agy_session = TerminalSession::spawn(
                        "agy",
                        &["--dangerously-skip-permissions"],
                        "AntigravityCLI",
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                crate::config::AgentKind::Claude => {
                    claude_session = TerminalSession::spawn(
                        "claude",
                        &["--dangerously-skip-permissions"],
                        "ClaudeCode",
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                other => {
                    agy_session = TerminalSession::spawn(
                        other.command_bin(),
                        &[],
                        other.display_name(),
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
            }
        }

        let initial_focus: AgentFocus = config.primary_agent.into();

        let herdr = if cfg!(test) {
            None
        } else {
            let h_opt = HerdrClient::try_detect();
            if let Some(h) = &h_opt {
                match config.primary_agent {
                    crate::config::AgentKind::Agy => {
                        let is_rev = config.review_agent == crate::config::AgentKind::Agy;
                        let agy_pid = agy_session.as_ref().and_then(|s| s.process_id());
                        let active_conv_id = crate::markdown_renderer::detect_active_antigravity_conversation_id(cwd, None, agy_pid);
                        h.report_active_agent(
                            "agy",
                            crate::herdr::AgentState::Idle,
                            Some("Antigravity CLI"),
                            active_conv_id.as_deref(),
                            is_rev,
                        );
                    }
                    crate::config::AgentKind::Claude => {
                        let is_rev = config.review_agent == crate::config::AgentKind::Claude;
                        let claude_sess = crate::markdown_renderer::detect_active_claude_session_id(cwd);
                        h.report_active_agent(
                            "claude",
                            crate::herdr::AgentState::Idle,
                            Some("Claude Code"),
                            claude_sess.as_deref(),
                            is_rev,
                        );
                    }
                    other => {
                        let is_rev = config.review_agent == other;
                        h.report_active_agent(
                            other.as_str(),
                            crate::herdr::AgentState::Idle,
                            Some(other.display_name()),
                            None,
                            is_rev,
                        );
                    }
                }
            }
            h_opt
        };

        let classifications = HashMap::new();
        let agy_pid = agy_session.as_ref().and_then(|s| s.process_id());
        let active_antigravity_conversation_id = crate::markdown_renderer::detect_active_antigravity_conversation_id(
            repo_path.as_deref(),
            None,
            agy_pid,
        );
        let artifacts = crate::markdown_renderer::discover_artifacts(
            repo_path.as_deref(),
            None,
            agy_pid,
        );

        let is_primary_claude = config.primary_agent == crate::config::AgentKind::Claude;
        let rev_name = config.review_agent.as_str();

        let palette = if let Some(ref custom) = config.custom_theme {
            crate::theme::Palette::from_name(custom).unwrap_or_else(crate::theme::detect_herdr_theme)
        } else {
            crate::theme::detect_herdr_theme()
        };

        let mut app = Self {
            diff,
            active_tab: ActiveTab::Agents,
            agent_focus: initial_focus,
            agy_session,
            claude_session,
            show_claude: is_primary_claude,
            show_review_agent: false,
            config,
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
            classifications,
            is_classifying: false,
            artifacts,
            active_antigravity_conversation_id,
            last_agy_pid: agy_pid,
            last_conv_detect_time: std::time::Instant::now(),
            show_artifact_tree: true,
            selected_artifact_idx: 0,
            artifact_scroll_offset: 0,
            artifact_cursor_idx: 0,
            artifact_pane_focus: ArtifactPaneFocus::FileList,
            pointwise_questions: Vec::new(),
            status_message: format!(
                "herdr-interactive-diff ready. [1-3] Tabs | [Ctrl+R] Split Review ({}) | [Ctrl+A] Switch AI | [Ctrl+H] Help",
                rev_name
            ),
            should_quit: false,
            herdr,
            last_herdr_state: Some("idle".to_string()),
            last_herdr_snapshot: None,
            last_herdr_report_time: Some(std::time::Instant::now()),
            palette,
            agent_selector_open: false,
            agent_selector_idx: 0,
            selecting_for_review: false,
            active_herdr_agent_pane: None,
            active_herdr_review_pane: None,
            show_agent_picker: false,
            agent_picker_target: AgentPickerTarget::Primary,
            agent_picker_idx: 0,
            mouse_drag_start: None,
            mouse_drag_end: None,
            selected_text: None,
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
        let current_pid = self.agy_session.as_ref().and_then(|s| s.process_id());
        let screen_text = self.agy_session.as_ref().map(|s| s.read_screen_text());
        let detected = crate::markdown_renderer::detect_active_antigravity_conversation_id(
            self.repo_path.as_deref(),
            screen_text.as_deref(),
            current_pid,
        );
        if let Some(new_id) = detected {
            self.active_antigravity_conversation_id = Some(new_id);
            self.last_agy_pid = current_pid;
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
        if self.selected_file_idx >= self.diff.files.len() {
            self.selected_file_idx = self.diff.files.len().saturating_sub(1);
        }
        self.code_cursor_idx = 0;
        self.code_scroll_offset = 0;
        self.show_tooltip = false;
        self.rebuild_code_lines();

        if self.diff.files.is_empty() {
            self.status_message = "Git diff updated: clean working tree (no changes).".to_string();
        } else {
            self.status_message = format!("Git diff updated: {} file(s) modified.", self.diff.files.len());
        }
    }

    /// Checks whether two agent panels should be displayed simultaneously
    pub fn is_dual_agent_active(&self) -> bool {
        self.show_review_agent
    }

    /// Alterna exibição do painel da IA de review em split (Ctrl+R)
    pub fn toggle_review_agent(&mut self) {
        if self.show_review_agent {
            self.show_review_agent = false;
            self.agent_focus = AgentFocus::Antigravity;
            self.status_message = "Review AI pane closed. Primary AI focused (100% width).".to_string();
        } else {
            self.show_review_agent = true;
            if self.claude_session.is_none() {
                self.launch_or_trigger_review();
            }
            self.agent_focus = AgentFocus::Claude;
            self.status_message = format!(
                "Review AI ({}) opened in split view (50/50). [Ctrl+O/Tab] Switch focus | [Ctrl+R] Close",
                self.config.review_agent.display_name()
            );
        }
    }

    /// Launches Review Agent in split view if not already running, and triggers review
    #[allow(dead_code)]
    pub fn launch_or_trigger_review(&mut self) {
        self.show_review_agent = true;

        let cwd = self.repo_path.as_deref();
        match self.config.review_agent {
            crate::config::AgentKind::Claude => {
                self.show_claude = true;
                if !cfg!(test) && self.claude_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.claude_session = TerminalSession::spawn(
                        "claude",
                        &["--dangerously-skip-permissions"],
                        "ClaudeCode",
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                self.agent_focus = AgentFocus::Claude;
                if let Some(h) = &mut self.herdr {
                    let claude_sess = crate::markdown_renderer::detect_active_claude_session_id(cwd);
                    h.report_claude(crate::herdr::AgentState::Working, Some("Claude Code starting"), claude_sess.as_deref(), true);
                }
            }
            crate::config::AgentKind::Agy => {
                if !cfg!(test) && self.agy_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.agy_session = TerminalSession::spawn(
                        "agy",
                        &["--dangerously-skip-permissions"],
                        "AntigravityCLI",
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                self.agent_focus = AgentFocus::Antigravity;
                if let Some(h) = &mut self.herdr {
                    let agy_pid = self.agy_session.as_ref().and_then(|sess| sess.process_id());
                    let active_conv_id = crate::markdown_renderer::detect_active_antigravity_conversation_id(cwd, None, agy_pid);
                    h.report_agy(crate::herdr::AgentState::Working, Some("Antigravity CLI starting"), active_conv_id.as_deref(), true);
                }
            }
            other => {
                if !cfg!(test) && self.claude_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.claude_session = TerminalSession::spawn(
                        other.command_bin(),
                        &[],
                        other.display_name(),
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                self.agent_focus = AgentFocus::Claude;
                self.status_message = format!("Review agent {} started in split.", other.display_name());
            }
        }

        if !cfg!(test) {
            self.trigger_review();
        }
    }

    /// Backward-compatible alias for launch_or_trigger_review
    #[allow(dead_code)]
    pub fn launch_or_trigger_claude(&mut self) {
        self.launch_or_trigger_review();
    }

    /// Scrolls the Help modal content up or down
    pub fn scroll_help(&mut self, delta: isize) {
        if delta < 0 {
            self.help_scroll_offset = self.help_scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.help_scroll_offset = self.help_scroll_offset.saturating_add(delta as usize);
        }
    }

    /// Restarts focused agent terminal
    #[allow(dead_code)]
    pub fn restart_focused_agent(&mut self) {
        match self.agent_focus {
            AgentFocus::Antigravity => self.restart_antigravity(),
            AgentFocus::Claude => self.restart_claude(),
        }
    }

    /// Envia bytes brutos (teclas digitadas) para a sessão do agente atualmente em foco
    #[allow(dead_code)]
    pub fn send_key_to_agent(&self, bytes: &[u8]) {
        match self.agent_focus {
            AgentFocus::Antigravity => {
                if let Some(s) = &self.agy_session {
                    s.send_bytes(bytes);
                }
            }
            AgentFocus::Claude => {
                if let Some(s) = &self.claude_session {
                    s.send_bytes(bytes);
                }
            }
        }
    }

    /// Envia texto com Bracketed Paste Mode de forma atômica para a sessão do agente atualmente em foco
    pub fn paste_text_to_agent(&self, text: &str) {
        match self.agent_focus {
            AgentFocus::Antigravity => {
                if let Some(s) = &self.agy_session {
                    s.paste_text(text);
                }
            }
            AgentFocus::Claude => {
                if let Some(s) = &self.claude_session {
                    s.paste_text(text);
                }
            }
        }
    }

    /// Switches focus between Primary and Review agents (when both are active)
    #[allow(dead_code)]
    pub fn toggle_agent_focus(&mut self) {
        if !self.is_dual_agent_active() {
            let rev_name = self.config.review_agent.display_name();
            self.status_message = format!("Review agent ({}) is not active. Press Ctrl+R to launch.", rev_name);
            return;
        }
        self.agent_focus = match self.agent_focus {
            AgentFocus::Antigravity => AgentFocus::Claude,
            AgentFocus::Claude => AgentFocus::Antigravity,
        };
        self.sync_herdr_agent_state("idle");
    }

    /// Opens AI Picker Modal (Ctrl+A)
    pub fn open_agent_picker(&mut self) {
        self.show_agent_picker = true;
        let current = match self.agent_picker_target {
            AgentPickerTarget::Primary => self.config.primary_agent,
            AgentPickerTarget::Review => self.config.review_agent,
        };
        self.agent_picker_idx = crate::config::ALL_AGENTS
            .iter()
            .position(|&a| a == current)
            .unwrap_or(0);
        self.status_message = "Select AI Agent [↑/↓ Navigate | Tab Switch Primary/Review | Enter Confirm | Esc Close]".to_string();
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
        let current = match self.agent_picker_target {
            AgentPickerTarget::Primary => self.config.primary_agent,
            AgentPickerTarget::Review => self.config.review_agent,
        };
        self.agent_picker_idx = crate::config::ALL_AGENTS
            .iter()
            .position(|&a| a == current)
            .unwrap_or(0);
    }

    /// Selects next AI in list
    pub fn agent_picker_next(&mut self) {
        if self.agent_picker_idx + 1 < crate::config::ALL_AGENTS.len() {
            self.agent_picker_idx += 1;
        } else {
            self.agent_picker_idx = 0;
        }
    }

    /// Selects previous AI in list
    pub fn agent_picker_prev(&mut self) {
        if self.agent_picker_idx > 0 {
            self.agent_picker_idx -= 1;
        } else {
            self.agent_picker_idx = crate::config::ALL_AGENTS.len().saturating_sub(1);
        }
    }

    /// Confirms AI selection for current target and restarts session if needed
    pub fn confirm_agent_picker(&mut self) {
        let selected = crate::config::ALL_AGENTS[self.agent_picker_idx];
        let cwd = self.repo_path.as_deref();
        match self.agent_picker_target {
            AgentPickerTarget::Primary => {
                self.config.primary_agent = selected;
                if !cfg!(test) {
                    let cmd = selected.command_bin();
                    let args: &[&str] = if selected == crate::config::AgentKind::Agy || selected == crate::config::AgentKind::Claude {
                        &["--dangerously-skip-permissions"]
                    } else {
                        &[]
                    };
                    self.agy_session = TerminalSession::spawn(
                        cmd,
                        args,
                        selected.display_name(),
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                self.status_message = format!("Primary AI switched to {}.", selected.display_name());
            }
            AgentPickerTarget::Review => {
                self.config.review_agent = selected;
                if self.show_review_agent && !cfg!(test) {
                    let cmd = selected.command_bin();
                    let args: &[&str] = if selected == crate::config::AgentKind::Agy || selected == crate::config::AgentKind::Claude {
                        &["--dangerously-skip-permissions"]
                    } else {
                        &[]
                    };
                    self.claude_session = TerminalSession::spawn(
                        cmd,
                        args,
                        selected.display_name(),
                        24,
                        80,
                        cwd,
                    )
                    .ok();
                }
                self.status_message = format!("Review AI switched to {}.", selected.display_name());
            }
        }
        self.show_agent_picker = false;
    }

    /// Triggers automated 5-lens review on the configured review agent
    pub fn trigger_review(&mut self) {
        self.show_review_agent = true;

        match self.config.review_agent {
            crate::config::AgentKind::Claude => {
                self.show_claude = true;
                if self.claude_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_claude();
                }
                if let Some(session) = &self.claude_session {
                    let prompt = crate::ai_engine::build_claude_review_prompt(&self.diff.raw);
                    session.paste_command(&prompt);
                    self.agent_focus = AgentFocus::Claude;
                    self.sync_herdr_agent_state("working");
                    self.status_message = "5-lens automated review triggered on Claude Code!".to_string();
                } else {
                    self.status_message = "Claude Code terminal could not be started.".to_string();
                }
            }
            crate::config::AgentKind::Agy => {
                if self.agy_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_antigravity();
                }
                if let Some(session) = &self.agy_session {
                    let prompt = crate::ai_engine::build_claude_review_prompt(&self.diff.raw);
                    session.paste_command(&prompt);
                    self.agent_focus = AgentFocus::Antigravity;
                    self.sync_herdr_agent_state("working");
                    self.status_message = "5-lens automated review triggered on Antigravity CLI!".to_string();
                } else {
                    self.status_message = "Antigravity CLI terminal could not be started.".to_string();
                }
            }
            other => {
                if let Some(session) = &self.claude_session {
                    let prompt = crate::ai_engine::build_claude_review_prompt(&self.diff.raw);
                    session.paste_command(&prompt);
                    self.agent_focus = AgentFocus::Claude;
                    self.sync_herdr_agent_state("working");
                    self.status_message = format!("5-lens automated review triggered on {}!", other.display_name());
                } else {
                    self.status_message = format!("Review triggered for {}.", other.display_name());
                }
            }
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
                crate::markdown_renderer::read_latest_claude_session_text(self.repo_path.as_deref())
                    .or_else(|| self.claude_session.as_ref().map(|s| s.read_screen_text()))
            }
            crate::config::AgentKind::Agy => self.agy_session.as_ref().map(|s| s.read_screen_text()),
            _ => self.claude_session.as_ref().map(|s| s.read_screen_text()),
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

    /// Triggers anti-overengineering validation on Primary Agent
    pub fn trigger_validation(&mut self) {
        match self.config.primary_agent {
            crate::config::AgentKind::Agy => {
                if self.agy_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_antigravity();
                }
            }
            crate::config::AgentKind::Claude => {
                if self.claude_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_claude();
                }
            }
            _ => {}
        }

        self.sync_review_to_diff();

        let review_text = match self.config.review_agent {
            crate::config::AgentKind::Claude => {
                crate::markdown_renderer::read_latest_claude_session_text(self.repo_path.as_deref())
                    .or_else(|| self.claude_session.as_ref().map(|s| s.read_screen_text()))
                    .unwrap_or_default()
            }
            crate::config::AgentKind::Agy => {
                self.agy_session.as_ref().map(|s| s.read_screen_text()).unwrap_or_default()
            }
            _ => {
                self.claude_session.as_ref().map(|s| s.read_screen_text()).unwrap_or_default()
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

        match self.config.primary_agent {
            crate::config::AgentKind::Agy => {
                if let Some(agy) = &self.agy_session {
                    agy.paste_command(&prompt);
                    self.active_tab = ActiveTab::Agents;
                    self.agent_focus = AgentFocus::Antigravity;
                    self.sync_herdr_agent_state("working");
                    self.status_message = "Review copied to Antigravity CLI (Anti-overengineering validation)!".to_string();
                } else {
                    self.status_message = "Antigravity CLI terminal is not active. Copied validation prompt to clipboard.".to_string();
                }
            }
            crate::config::AgentKind::Claude => {
                if let Some(claude) = &self.claude_session {
                    claude.paste_command(&prompt);
                    self.active_tab = ActiveTab::Agents;
                    self.agent_focus = AgentFocus::Claude;
                    self.sync_herdr_agent_state("working");
                    self.status_message = "Review copied to Claude Code (Anti-overengineering validation)!".to_string();
                } else {
                    self.status_message = "Claude Code terminal is not active. Copied validation prompt to clipboard.".to_string();
                }
            }
            other => {
                if let Some(sess) = &self.agy_session {
                    sess.paste_command(&prompt);
                    self.active_tab = ActiveTab::Agents;
                    self.agent_focus = AgentFocus::Antigravity;
                    self.sync_herdr_agent_state("working");
                    self.status_message = format!("Review copied to {} (Anti-overengineering validation)!", other.display_name());
                } else {
                    self.status_message = format!("Validation prompt copied to clipboard for {}!", other.display_name());
                }
            }
        }
    }

    /// Backward-compatible alias for trigger_validation
    pub fn trigger_antigravity_validation(&mut self) {
        self.trigger_validation();
    }

    /// Synchronizes active agent state with Herdr Workspace Manager
    pub fn sync_herdr_agent_state(&mut self, state: &str) {
        let s = match state {
            "working" => crate::herdr::AgentState::Working,
            "blocked" => crate::herdr::AgentState::Blocked,
            _ => crate::herdr::AgentState::Idle,
        };
        let is_dual = self.is_dual_agent_active();
        let primary_agent = self.config.primary_agent;
        let review_agent = self.config.review_agent;

        let (target_agent, is_reviewer) = match self.agent_focus {
            AgentFocus::Claude if is_dual && review_agent == crate::config::AgentKind::Claude => (review_agent, true),
            AgentFocus::Claude if primary_agent == crate::config::AgentKind::Claude => (primary_agent, false),
            AgentFocus::Antigravity if is_dual && review_agent != crate::config::AgentKind::Claude => (review_agent, true),
            _ => (primary_agent, false),
        };

        let active_conv_id = self.get_or_detect_active_agy_conversation_id();
        let claude_sess = crate::markdown_renderer::detect_active_claude_session_id(self.repo_path.as_deref());
        let session_id = match target_agent {
            crate::config::AgentKind::Agy => active_conv_id,
            crate::config::AgentKind::Claude => claude_sess,
            _ => None,
        };

        let msg = target_agent.display_name().to_string();
        if let Some(h) = &mut self.herdr {
            h.report_active_agent(target_agent.as_str(), s, Some(&msg), session_id.as_deref(), is_reviewer);
        }
        self.last_herdr_snapshot = Some(HerdrReportSnapshot {
            agent: target_agent.as_str().to_string(),
            state: s,
            message: Some(msg),
            session_id,
            is_reviewer,
        });
        self.last_herdr_state = Some(state.to_string());
        self.last_herdr_report_time = Some(std::time::Instant::now());
    }

    /// Dynamically updates status in Herdr (working, idle, or blocked) with state deduplication
    pub fn update_herdr_state(&mut self) {
        if self.herdr.is_none() {
            return;
        }

        let is_dual = self.is_dual_agent_active();
        let primary_agent = self.config.primary_agent;
        let review_agent = self.config.review_agent;

        // Detect operational state for the primary agent session
        let primary_session = if primary_agent == crate::config::AgentKind::Claude {
            self.claude_session.as_ref()
        } else {
            self.agy_session.as_ref()
        };
        let primary_state = primary_session
            .map(|s| crate::herdr::detect_session_state(s, primary_agent.as_str()))
            .unwrap_or(crate::herdr::AgentState::Idle);

        // Detect operational state for the reviewer agent session (if dual mode is active)
        let review_session = if is_dual {
            if review_agent == crate::config::AgentKind::Claude {
                self.claude_session.as_ref()
            } else {
                self.agy_session.as_ref()
            }
        } else {
            None
        };
        let review_state = review_session
            .map(|s| crate::herdr::detect_session_state(s, review_agent.as_str()))
            .unwrap_or(crate::herdr::AgentState::Idle);

        let active_conv_id = self.get_or_detect_active_agy_conversation_id();
        let claude_sess = crate::markdown_renderer::detect_active_claude_session_id(self.repo_path.as_deref());

        // Determine which agent has priority for Herdr status reporting without flicking:
        // 1. If an agent is Blocked (waiting for user input / approval), report that agent as Blocked
        // 2. If one agent is Working while the other is not, report that agent as Working
        // 3. Otherwise, report the currently focused agent with its state
        let (active_agent_kind, state, is_reviewer) = if review_state == crate::herdr::AgentState::Blocked && is_dual {
            (review_agent, review_state, true)
        } else if primary_state == crate::herdr::AgentState::Blocked {
            (primary_agent, primary_state, false)
        } else if review_state == crate::herdr::AgentState::Working && primary_state != crate::herdr::AgentState::Working && is_dual {
            (review_agent, review_state, true)
        } else if primary_state == crate::herdr::AgentState::Working && review_state != crate::herdr::AgentState::Working {
            (primary_agent, primary_state, false)
        } else {
            match self.agent_focus {
                AgentFocus::Claude if is_dual && review_agent == crate::config::AgentKind::Claude => {
                    (review_agent, review_state, true)
                }
                AgentFocus::Claude if primary_agent == crate::config::AgentKind::Claude => {
                    (primary_agent, primary_state, false)
                }
                AgentFocus::Antigravity if is_dual && review_agent != crate::config::AgentKind::Claude => {
                    (review_agent, review_state, true)
                }
                _ => (primary_agent, primary_state, false),
            }
        };

        let msg = match state {
            crate::herdr::AgentState::Blocked => format!("{} (needs input)", active_agent_kind.display_name()),
            crate::herdr::AgentState::Working => format!("{} (working)", active_agent_kind.display_name()),
            crate::herdr::AgentState::Idle => active_agent_kind.display_name().to_string(),
        };

        let session_id = match active_agent_kind {
            crate::config::AgentKind::Agy => active_conv_id,
            crate::config::AgentKind::Claude => claude_sess,
            _ => None,
        };

        let new_snapshot = HerdrReportSnapshot {
            agent: active_agent_kind.as_str().to_string(),
            state,
            message: Some(msg.clone()),
            session_id: session_id.clone(),
            is_reviewer,
        };

        let now = std::time::Instant::now();
        let should_send = match (&self.last_herdr_snapshot, self.last_herdr_report_time) {
            (Some(last), Some(last_time)) => {
                // Send if state/agent/session changed, or if 5-second heartbeat has elapsed
                *last != new_snapshot || now.duration_since(last_time) >= std::time::Duration::from_secs(5)
            }
            _ => true,
        };

        if should_send {
            if let Some(h) = &mut self.herdr {
                h.report_active_agent(&new_snapshot.agent, state, Some(&msg), session_id.as_deref(), is_reviewer);
            }
            self.last_herdr_snapshot = Some(new_snapshot);
            self.last_herdr_report_time = Some(now);
        }

        self.last_herdr_state = Some(state.as_str().to_string());
    }

    /// Cleans up Herdr registrations when exiting
    pub fn cleanup(&mut self) {
        if let Some(h) = &mut self.herdr {
            h.release_agents(&[
                self.config.primary_agent.as_str(),
                self.config.review_agent.as_str(),
            ]);
        }
    }

    /// Restarts Claude Code session
    pub fn restart_claude(&mut self) {
        self.show_claude = true;
        let cwd = self.repo_path.as_deref();
        self.claude_session = TerminalSession::spawn(
            "claude",
            &["--dangerously-skip-permissions"],
            "ClaudeCode",
            24,
            80,
            cwd,
        )
        .ok();
        self.agent_focus = AgentFocus::Claude;
        self.sync_herdr_agent_state("idle");
        self.status_message = "Claude Code restarted.".to_string();
    }

    /// Restarts Antigravity CLI session
    pub fn restart_antigravity(&mut self) {
        let cwd = self.repo_path.as_deref();
        self.agy_session = TerminalSession::spawn(
            "agy",
            &["--dangerously-skip-permissions"],
            "AntigravityCLI",
            24,
            80,
            cwd,
        )
        .ok();
        self.agent_focus = AgentFocus::Antigravity;
        self.sync_herdr_agent_state("idle");
        self.status_message = "Antigravity CLI restarted.".to_string();
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

    /// Submits question about selected diff hunk to focused AI agent
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

        match self.agent_focus {
            AgentFocus::Antigravity => {
                if self.agy_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_antigravity();
                }
                if let Some(agy) = &self.agy_session {
                    agy.send_command(&prompt);
                }
            }
            AgentFocus::Claude => {
                if self.claude_session.as_ref().map(|s| !s.is_alive()).unwrap_or(true) {
                    self.restart_claude();
                }
                if let Some(claude) = &self.claude_session {
                    claude.send_command(&prompt);
                }
            }
        }

        let agent_name = match self.agent_focus {
            AgentFocus::Antigravity => "Antigravity CLI",
            AgentFocus::Claude => "Claude Code",
        };

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
                    explanation: Some(format!("Question sent to {}:\n{}", agent_name, question)),
                    hint: None,
                }
            });
            let prev_expl = entry.explanation.clone().unwrap_or_default();
            if !prev_expl.contains(&question) {
                entry.explanation = Some(format!("{}\n\n󰌵 Question [?]: {}", prev_expl, question));
            }
        }
        self.rebuild_code_lines();

        self.status_message = format!("Question about '{}' sent to {}!", file_path, agent_name);
        self.question_input.clear();
        self.is_asking_question = false;
        self.active_tab = ActiveTab::Agents;
        self.sync_herdr_agent_state("working");
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
        if let Some(conv_id) = self.get_or_detect_active_agy_conversation_id() {
            self.artifacts = crate::markdown_renderer::discover_artifacts_for_session(&conv_id);
        } else {
            let screen_text = self.agy_session.as_ref().map(|s| s.read_screen_text());
            let agy_pid = self.agy_session.as_ref().and_then(|s| s.process_id());
            self.artifacts = discover_artifacts(self.repo_path.as_deref(), screen_text.as_deref(), agy_pid);
            if let Some(first) = self.artifacts.first().and_then(|a| a.conversation_id.clone()) {
                self.active_antigravity_conversation_id = Some(first);
            }
        }
        if self.artifacts.is_empty() {
            if self.active_tab == ActiveTab::Artifacts {
                self.active_tab = ActiveTab::GitDiff;
            }
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
        let screen_text = self.agy_session.as_ref().map(|s| s.read_screen_text());
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
            let agy_pid = self.agy_session.as_ref().and_then(|s| s.process_id());
            let list = discover_artifacts(self.repo_path.as_deref(), screen_text.as_deref(), agy_pid);
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
                if self.active_tab == ActiveTab::Artifacts {
                    self.active_tab = ActiveTab::Agents;
                }
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
        let app = App::new(diff, None);
        assert_eq!(app.active_tab, ActiveTab::Agents);
        assert_eq!(app.agent_focus, AgentFocus::Antigravity);
        assert!(!app.show_claude);
        assert!(app.claude_session.is_none());
    }

    #[test]
    fn test_launch_claude() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        assert!(!app.show_claude);
        app.launch_or_trigger_claude();
        assert!(app.show_claude);
        assert_eq!(app.agent_focus, AgentFocus::Claude);
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
        assert_eq!(app.active_tab, ActiveTab::Agents);
        assert!(app.status_message.contains("Question about"));
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

        app.sync_herdr_agent_state("working");
        assert_eq!(app.last_herdr_state.as_deref(), Some("working"));
    }

    #[test]
    fn test_herdr_state_deduplication() {
        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.herdr = Some(crate::herdr::HerdrClient::new(
            std::path::PathBuf::from("/nonexistent/herdr.sock"),
            "test:pane1".to_string(),
        ));

        // First update initializes snapshot
        app.update_herdr_state();
        let snap1 = app.last_herdr_snapshot.clone();
        assert!(snap1.is_some());
        let time1 = app.last_herdr_report_time;

        // Second immediate update without state change should be deduplicated (time and snapshot unchanged)
        app.update_herdr_state();
        let snap2 = app.last_herdr_snapshot.clone();
        assert_eq!(snap1, snap2);
        assert_eq!(time1, app.last_herdr_report_time);
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
            app.status_message.contains("Antigravity CLI")
                || app.status_message.contains("Claude review copied")
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
        let cfg = crate::config::WeaversConfig {
            primary_agent: crate::config::AgentKind::Claude,
            review_agent: crate::config::AgentKind::Agy,
            custom_theme: None,
        };
        let mut app = App::new_with_config(diff, None, cfg);
        assert_eq!(app.active_tab, ActiveTab::Agents);
        assert_eq!(app.agent_focus, AgentFocus::Claude);
        assert!(app.show_claude);
        assert!(!app.show_review_agent);
        assert!(!app.is_dual_agent_active());

        // When review is triggered, review agent (agy) activates
        app.launch_or_trigger_review();
        assert!(app.show_review_agent);
        assert_eq!(app.agent_focus, AgentFocus::Antigravity);
        assert!(app.is_dual_agent_active());
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
}
