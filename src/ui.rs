use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
    Frame,
};

use crate::ai_engine::ComplexityLevel;
use crate::app::{
    ActiveTab, App, ArtifactPaneFocus, CodeLineDisplay, DiffPaneFocus, TooltipData, TooltipKind,
};
use crate::git::LineOrigin;

/// Renders the complete Weavers interface in English with Nerd Fonts
pub fn render_ui(f: &mut Frame, app: &mut App, list_state: &mut ListState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Bar (Tabs + weavers repo info on right)
            Constraint::Min(5),    // Main content (Git Diff or Artifacts)
        ])
        .split(f.area());

    render_top_bar(f, app, chunks[0]);

    match app.active_tab {
        ActiveTab::GitDiff => render_git_diff_view(f, app, list_state, chunks[1]),
        ActiveTab::Artifacts => render_artifacts_view(f, app, chunks[1]),
    }

    // AI Question overlay input bar when user presses '?'
    if app.is_asking_question {
        let question_area = Rect {
            x: chunks[1].x + 2,
            y: chunks[1].y + chunks[1].height.saturating_sub(4),
            width: chunks[1].width.saturating_sub(4),
            height: 3,
        };
        f.render_widget(Clear, question_area);
        render_question_input_bar(f, app, question_area);
    }

    // AI Picker Modal (Ctrl+A)
    if app.show_agent_picker {
        render_agent_picker_modal(f, app);
    }

    // Help Modal with Keybindings (Ctrl+H or F12)
    if app.show_help {
        render_help_modal(f, app);
    }
}

/// Top bar with tab selector on left and "herdr-interactive-diff • repo • Ctrl+H for help" on the right.
fn render_top_bar(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.palette;
    let show_artifacts_tab = app.has_artifacts() || app.active_tab == ActiveTab::Artifacts;
    let titles = if show_artifacts_tab {
        vec![" [1] 󰊢 Git Diff ", " [2] 󰈙 Artifacts "]
    } else {
        vec![" [1] 󰊢 Git Diff "]
    };
    let selected_index = match app.active_tab {
        ActiveTab::GitDiff => 0,
        ActiveTab::Artifacts => 1,
    };

    let repo_info = if area.width >= 75 {
        if let Some(path) = &app.repo_path {
            let repo_name = path.split('/').rfind(|s| !s.is_empty()).unwrap_or(path);
            Some((
                format!("󰊢 herdr-interactive-diff • {}", repo_name),
                " • Ctrl+H for help ",
            ))
        } else {
            Some((
                "󰊢 herdr-interactive-diff".to_string(),
                " • Ctrl+H for help ",
            ))
        }
    } else if area.width >= 45 {
        Some(("󰊢 diff".to_string(), " • Ctrl+H "))
    } else {
        None
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pal.surface1));
    f.render_widget(block, area);

    let inner_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2).min(1),
    };

    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    let min_tabs_w = if app.has_artifacts() { 48 } else { 34 };

    if let Some((repo_str, help_str)) = repo_info {
        let total_chars = repo_str.chars().count() + help_str.chars().count();
        let info_len = total_chars as u16;
        if inner_area.width >= min_tabs_w + info_len {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(min_tabs_w), Constraint::Length(info_len)])
                .split(inner_area);

            let tabs = Tabs::new(titles)
                .select(selected_index)
                .style(Style::default().fg(pal.subtext0))
                .highlight_style(
                    Style::default()
                        .fg(pal.panel_bg)
                        .bg(pal.accent)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_widget(tabs, chunks[0]);

            let info_p = Paragraph::new(Line::from(vec![
                Span::styled(
                    repo_str,
                    Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(help_str, Style::default().fg(pal.subtext0)),
            ]))
            .alignment(ratatui::layout::Alignment::Right);
            f.render_widget(info_p, chunks[1]);
            return;
        }
    }

    let tabs = Tabs::new(titles)
        .select(selected_index)
        .style(Style::default().fg(pal.subtext0))
        .highlight_style(
            Style::default()
                .fg(pal.panel_bg)
                .bg(pal.accent)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, inner_area);
}

/// Tab 1: Git Diff view with file drawer + diff/full-file viewer + floating tooltips
fn render_git_diff_view(f: &mut Frame, app: &mut App, list_state: &mut ListState, area: Rect) {
    if app.diff.files.is_empty() {
        let repo_name = app
            .repo_path
            .as_deref()
            .and_then(|p| p.split('/').rfind(|s| !s.is_empty()))
            .unwrap_or("Local Repository");

        let empty_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "   ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("Repository: {}", repo_name),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  󰄬 Clean working tree — no uncommitted changes detected.",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(Span::styled(
                "    There are no modified files or staged diffs in this repository.",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Span::styled(
                "  Clean Working Tree ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));

        let p = Paragraph::new(empty_lines).block(block);
        f.render_widget(p, area);
        return;
    }

    if app.show_diff_tree {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(20)])
            .split(area);

        // 1. Lateral File Drawer
        render_snacks_file_drawer(f, app, chunks[0]);

        // 2. Code / Diff Viewer
        let code_area = chunks[1];
        render_code_view(f, app, list_state, code_area);

        // 3. Floating Tooltip for Review Comments & Questions
        if app.show_tooltip {
            render_floating_tooltip(f, app, code_area);
        }
    } else {
        // Full width Code / Diff Viewer
        let code_area = area;
        render_code_view(f, app, list_state, code_area);

        // Floating Tooltip for Review Comments & Questions
        if app.show_tooltip {
            render_floating_tooltip(f, app, code_area);
        }
    }
}

/// Renders file drawer with status badges [A], [M], [D]
fn render_snacks_file_drawer(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.diff_pane_focus == DiffPaneFocus::FileList;
    let border_color = if is_focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let title = format!("  Files ({}) ", app.diff.files.len());

    let mut items = Vec::new();
    for (idx, file) in app.diff.files.iter().enumerate() {
        let is_selected = idx == app.selected_file_idx;
        let prefix = if is_selected { "▶ " } else { "  " };

        let (badge_fg, badge_bg) = match file.status {
            crate::git::FileStatus::Added => (Color::Black, Color::Green),
            crate::git::FileStatus::Modified => (Color::Black, Color::Yellow),
            crate::git::FileStatus::Deleted => (Color::White, Color::Red),
        };

        let file_name = file.file_name();
        let stats_str = format!("+{} -{}", file.additions, file.deletions);

        let item = Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(" {} ", file.status.badge()),
                Style::default()
                    .fg(badge_fg)
                    .bg(badge_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                file_name,
                if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
            Span::styled(
                format!(" {:>7}", stats_str),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        items.push(ListItem::new(item));
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                title,
                Style::default().add_modifier(Modifier::BOLD),
            )),
    );

    f.render_widget(list, area);
}

/// Renders the code view panel (Diff Only or Full File)
fn render_code_view(f: &mut Frame, app: &App, list_state: &mut ListState, area: Rect) {
    let is_focused = app.diff_pane_focus == DiffPaneFocus::CodeView;
    let border_color = if is_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let file_path = app
        .diff
        .files
        .get(app.selected_file_idx)
        .map(|f| f.new_path.as_str())
        .unwrap_or("No file");

    let title = format!("  {} ", file_path);

    let mut items = Vec::new();
    for row in &app.code_lines {
        match row {
            CodeLineDisplay::HunkHeader { header, .. } => {
                let text = Line::from(vec![
                    Span::styled("    @@ ", Style::default().fg(Color::Magenta)),
                    Span::styled(header.clone(), Style::default().fg(Color::DarkGray)),
                ]);
                items.push(ListItem::new(text));
            }
            CodeLineDisplay::CommentHeader { level, title, .. } => {
                let (icon, badge_text, border_color, bg_color) = match level {
                    ComplexityLevel::Forte => {
                        ("󰀪", "HIGH", Color::LightRed, Color::Rgb(45, 15, 18))
                    }
                    ComplexityLevel::Media => {
                        ("󰀦", "MEDIUM", Color::Yellow, Color::Rgb(45, 38, 10))
                    }
                    ComplexityLevel::Curiosidade => {
                        ("󰌵", "LOW", Color::LightCyan, Color::Rgb(15, 30, 48))
                    }
                    ComplexityLevel::Normal => ("󰋼", "LOW", Color::Cyan, Color::Rgb(20, 25, 35)),
                };

                let prefix = format!("    ╭── {} [{}] ", icon, badge_text);
                let title_clean = title.replace('\n', " ");
                let prefix_chars = prefix.chars().count();
                let max_title_len = (area.width as usize).saturating_sub(prefix_chars + 6);
                let display_title = if max_title_len > 3 {
                    truncate_str(&title_clean, max_title_len)
                } else {
                    title_clean
                };
                let used_len = prefix_chars + display_title.chars().count() + 1;
                let rule_len = (area.width as usize).saturating_sub(used_len + 4).max(3);
                let rule = "─".repeat(rule_len);

                let text = Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(border_color)
                            .bg(bg_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", display_title),
                        Style::default()
                            .fg(Color::White)
                            .bg(bg_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(rule, Style::default().fg(border_color).bg(bg_color)),
                ]);
                items.push(ListItem::new(text));
            }
            CodeLineDisplay::CommentFooter {
                level, fix_or_hint, ..
            } => {
                let (icon, border_color, bg_color) = match level {
                    ComplexityLevel::Forte => ("󰌵", Color::LightRed, Color::Rgb(45, 15, 18)),
                    ComplexityLevel::Media => ("󰌵", Color::Yellow, Color::Rgb(45, 38, 10)),
                    ComplexityLevel::Curiosidade => ("󰌵", Color::LightCyan, Color::Rgb(15, 30, 48)),
                    ComplexityLevel::Normal => ("󰌵", Color::Cyan, Color::Rgb(20, 25, 35)),
                };

                let hint_clean = fix_or_hint.replace('\n', " ");
                let prefix = format!("    ╰── {} ", icon);
                let prefix_chars = prefix.chars().count();
                let max_hint_len = (area.width as usize).saturating_sub(prefix_chars + 6);
                let display_hint = if max_hint_len > 3 {
                    truncate_str(&hint_clean, max_hint_len)
                } else {
                    hint_clean
                };
                let used_len = prefix_chars + display_hint.chars().count() + 1;
                let rule_len = (area.width as usize).saturating_sub(used_len + 4).max(3);
                let rule = "─".repeat(rule_len);

                let text = Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(border_color)
                            .bg(bg_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{} ", display_hint),
                        Style::default().fg(border_color).bg(bg_color),
                    ),
                    Span::styled(rule, Style::default().fg(border_color).bg(bg_color)),
                ]);
                items.push(ListItem::new(text));
            }
            CodeLineDisplay::DiffLine {
                origin,
                content,
                old_lineno,
                new_lineno,
                level,
                has_caveman,
                has_curiosity,
                in_comment_block,
                ..
            } => {
                let lineno = new_lineno.or(*old_lineno);
                let lineno_str = lineno
                    .map(|n| format!("{:>4}", n))
                    .unwrap_or_else(|| "    ".to_string());

                let (sign, base_fg) = match origin {
                    LineOrigin::Addition => ("+", Color::Green),
                    LineOrigin::Deletion => ("-", Color::Red),
                    LineOrigin::Context => (" ", Color::Gray),
                };

                let (gutter_border, bg_tint) = if *in_comment_block {
                    match level {
                        ComplexityLevel::Forte => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::LightRed)
                                    .bg(Color::Rgb(35, 15, 18))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(35, 15, 18)),
                        ),
                        ComplexityLevel::Media => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .bg(Color::Rgb(35, 30, 10))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(35, 30, 10)),
                        ),
                        ComplexityLevel::Curiosidade => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::LightCyan)
                                    .bg(Color::Rgb(15, 25, 40))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(15, 25, 40)),
                        ),
                        ComplexityLevel::Normal => (
                            Span::styled(
                                "    │ ",
                                Style::default().fg(Color::Cyan).bg(Color::Rgb(20, 25, 35)),
                            ),
                            Some(Color::Rgb(20, 25, 35)),
                        ),
                    }
                } else {
                    (Span::styled("      ", Style::default()), None)
                };

                let mut num_style = Style::default().fg(base_fg);
                if let Some(bg) = bg_tint {
                    num_style = num_style.bg(bg);
                }

                let mut line_style = Style::default().fg(base_fg);
                if let Some(bg) = bg_tint {
                    line_style = line_style.bg(bg);
                } else if *level == ComplexityLevel::Media {
                    line_style = line_style.bg(Color::Rgb(60, 50, 10));
                } else if *level == ComplexityLevel::Curiosidade {
                    line_style = line_style.bg(Color::Rgb(20, 35, 70));
                }

                let mut spans = vec![
                    gutter_border,
                    Span::styled(format!("{} {} ", lineno_str, sign), num_style),
                    Span::styled(content.clone(), line_style),
                ];

                if *has_caveman {
                    spans.push(Span::styled(
                        "  󰀪 [HIGH]",
                        Style::default()
                            .fg(Color::LightRed)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                if *has_curiosity {
                    spans.push(Span::styled(
                        "  󰌵 [LOW]",
                        Style::default()
                            .fg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                items.push(ListItem::new(Line::from(spans)));
            }
            CodeLineDisplay::FullFileLine {
                lineno,
                content,
                is_changed,
                level,
                has_caveman,
                has_curiosity,
                in_comment_block,
                ..
            } => {
                let lineno_str = format!("{:>4} │ ", lineno);
                let gutter_style = if *is_changed {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let (margin_border, bg_tint) = if *in_comment_block {
                    match level {
                        ComplexityLevel::Forte => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::LightRed)
                                    .bg(Color::Rgb(35, 15, 18))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(35, 15, 18)),
                        ),
                        ComplexityLevel::Media => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .bg(Color::Rgb(35, 30, 10))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(35, 30, 10)),
                        ),
                        ComplexityLevel::Curiosidade => (
                            Span::styled(
                                "    │ ",
                                Style::default()
                                    .fg(Color::LightCyan)
                                    .bg(Color::Rgb(15, 25, 40))
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Some(Color::Rgb(15, 25, 40)),
                        ),
                        ComplexityLevel::Normal => (
                            Span::styled(
                                "    │ ",
                                Style::default().fg(Color::Cyan).bg(Color::Rgb(20, 25, 35)),
                            ),
                            Some(Color::Rgb(20, 25, 35)),
                        ),
                    }
                } else {
                    (Span::styled("      ", Style::default()), None)
                };

                let mut num_style = gutter_style;
                if let Some(bg) = bg_tint {
                    num_style = num_style.bg(bg);
                }

                let mut line_style = if *is_changed {
                    Style::default().fg(Color::LightGreen)
                } else {
                    Style::default().fg(Color::White)
                };

                if let Some(bg) = bg_tint {
                    line_style = line_style.bg(bg);
                } else if *level == ComplexityLevel::Media {
                    line_style = line_style.bg(Color::Rgb(60, 50, 10));
                } else if *level == ComplexityLevel::Curiosidade {
                    line_style = line_style.bg(Color::Rgb(20, 35, 70));
                }

                let mut spans = vec![
                    margin_border,
                    Span::styled(lineno_str, num_style),
                    Span::styled(content.clone(), line_style),
                ];

                if *has_caveman {
                    spans.push(Span::styled(
                        "  󰀪 [HIGH]",
                        Style::default()
                            .fg(Color::LightRed)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                if *has_curiosity {
                    spans.push(Span::styled(
                        "  󰌵 [LOW]",
                        Style::default()
                            .fg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                items.push(ListItem::new(Line::from(spans)));
            }
        }
    }

    let code_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    title,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
        )
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(35, 40, 50))
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(code_list, area, list_state);
}

/// Renders structured markdown lines for floating review card
pub fn get_tooltip_rendered_lines(tooltip: &TooltipData, inner_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let avail_w = inner_width.max(20);

    // 1. Summary Banner with icon and distinct color matching severity
    let (icon, prefix_color) = match tooltip.kind {
        TooltipKind::High => ("󰀪 ", Color::LightRed),
        TooltipKind::Medium => ("󰀦 ", Color::Yellow),
        TooltipKind::Low => ("󰌵 ", Color::LightCyan),
    };

    let summary_chunks =
        crate::markdown_renderer::wrap_text(&tooltip.summary, avail_w.saturating_sub(4));
    for (idx, chunk) in summary_chunks.into_iter().enumerate() {
        if idx == 0 {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {} ", icon),
                    Style::default()
                        .fg(prefix_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    chunk,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    chunk,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }

    lines.push(Line::from(Span::styled(
        "─".repeat(avail_w.min(76)),
        Style::default().fg(Color::DarkGray),
    )));

    // 2. Details rendered with full responsive Markdown formatting
    let md_lines = crate::markdown_renderer::parse_markdown_with_width(&tooltip.details, avail_w);
    lines.extend(md_lines);

    // 3. Optional Hint / Actionable tip if present and not redundant
    if let Some(ref hint) = tooltip.hint {
        if !tooltip.details.contains(hint) {
            lines.push(Line::from(""));
            let hint_md = format!("> [!TIP] Dica: {}", hint);
            let hint_lines = crate::markdown_renderer::parse_markdown_with_width(&hint_md, avail_w);
            lines.extend(hint_lines);
        }
    }

    lines
}

/// Calculates floating tooltip geometry (Rect and rendered lines) responsive to screen and content
pub fn calculate_tooltip_geometry(
    app: &App,
    code_area: Rect,
) -> Option<(Rect, Vec<Line<'static>>)> {
    let tooltip = app.get_active_tooltip()?;

    let max_allowed_w = code_area.width.saturating_sub(4).max(24);
    let card_width = 86.min(max_allowed_w).max(42.min(max_allowed_w));
    let inner_width = (card_width.saturating_sub(4)) as usize;

    let lines = get_tooltip_rendered_lines(&tooltip, inner_width);
    let total_lines = lines.len() as u16;

    // A altura deve acomodar todo o conteúdo + 2 linhas de bordas
    let needed_height = total_lines.saturating_add(2);
    let max_allowed_h = code_area.height.saturating_sub(2).max(6);
    let card_height = needed_height.min(max_allowed_h).max(6);

    let rel_cursor_y = app.code_cursor_idx.saturating_sub(app.code_scroll_offset) as u16;
    let line_y = code_area.y + 1 + rel_cursor_y;

    let space_below = code_area.bottom().saturating_sub(line_y + 1);
    let space_above = line_y.saturating_sub(code_area.y + 1);

    let card_y = if space_below >= card_height {
        line_y + 1
    } else if space_above >= card_height {
        line_y.saturating_sub(card_height)
    } else if space_below >= space_above {
        line_y + 1
    } else {
        line_y.saturating_sub(card_height)
    };

    let clamped_y = card_y
        .max(code_area.y + 1)
        .min(code_area.bottom().saturating_sub(card_height));

    let card_x = (code_area.x + 3).min(code_area.right().saturating_sub(card_width + 1));

    let rect = Rect {
        x: card_x,
        y: clamped_y,
        width: card_width,
        height: card_height,
    };

    Some((rect, lines))
}

/// Helper public for mouse event hit testing and layout alignment
pub fn calculate_tooltip_rect(app: &App, term_area: Rect) -> Option<Rect> {
    let main_area = Rect {
        x: term_area.x,
        y: term_area.y + 3,
        width: term_area.width,
        height: term_area.height.saturating_sub(3),
    };
    let code_area = if app.show_diff_tree {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(34), Constraint::Min(20)])
            .split(main_area);
        chunks[1]
    } else {
        main_area
    };

    let (rect, _) = calculate_tooltip_geometry(app, code_area)?;
    Some(rect)
}

/// Renders floating tooltip anchored directly at cursor line position with dynamic dimensions
fn render_floating_tooltip(f: &mut Frame, app: &App, code_area: Rect) {
    let (tooltip_rect, lines) = match calculate_tooltip_geometry(app, code_area) {
        Some(val) => val,
        None => return,
    };

    let tooltip = match app.get_active_tooltip() {
        Some(t) => t,
        None => return,
    };

    f.render_widget(Clear, tooltip_rect);

    let (border_color, header_fg) = match tooltip.kind {
        TooltipKind::High => (Color::LightRed, Color::LightRed),
        TooltipKind::Medium => (Color::Yellow, Color::Yellow),
        TooltipKind::Low => (Color::LightCyan, Color::LightCyan),
    };

    let total_lines = lines.len();
    let visible_rows = tooltip_rect.height.saturating_sub(2) as usize;
    let is_scrollable = total_lines > visible_rows;
    let max_scroll = total_lines.saturating_sub(visible_rows);
    let scroll = app.tooltip_scroll_offset.min(max_scroll);

    let mut bottom_spans = vec![Span::styled(
        " [Esc/Space/Enter] Close ",
        Style::default().fg(Color::DarkGray),
    )];
    if is_scrollable {
        bottom_spans.push(Span::styled(
            format!(" [↑/↓ Scroll {}/{}] ", scroll + 1, total_lines),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" {} ", tooltip.title),
            Style::default().fg(header_fg).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(bottom_spans).alignment(ratatui::layout::Alignment::Right));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((scroll as u16, 0));

    f.render_widget(paragraph, tooltip_rect);
}

/// Tab 3: Artifacts viewer for Gemini / Antigravity Markdown artifacts
fn render_artifacts_view(f: &mut Frame, app: &mut App, area: Rect) {
    if app.artifacts.is_empty() {
        let empty_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "   ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "No Gemini / Antigravity artifacts found for active session.",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Artifacts are automatically saved by Antigravity in:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  ~/.gemini/antigravity-cli/brain/<active_conversation>/*.md",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  • Press ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "'r'",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " to reload artifacts from disk.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .title(Span::styled(
                "  HERDR • ARTIFACTS ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));

        let p = Paragraph::new(empty_lines).block(block);
        f.render_widget(p, area);
        return;
    }

    if app.show_artifact_tree {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(36), Constraint::Min(20)])
            .split(area);

        // 1. Artifact list drawer
        render_artifacts_drawer(f, app, chunks[0]);

        // 2. Document markdown viewer
        render_artifact_document_view(f, app, chunks[1]);
    } else {
        // Full width Document markdown viewer
        render_artifact_document_view(f, app, area);
    }
}

/// Lateral drawer with discovered artifact files
fn render_artifacts_drawer(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.artifact_pane_focus == ArtifactPaneFocus::FileList;
    let border_color = if is_focused {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let title = format!("  Artifacts ({}) ", app.artifacts.len());

    let mut items = Vec::new();
    for (idx, artifact) in app.artifacts.iter().enumerate() {
        let is_selected = idx == app.selected_artifact_idx;
        let prefix = if is_selected { "▶ " } else { "  " };

        let icon = match artifact.file_name.as_str() {
            "implementation_plan.md" => "󰆼 ",
            "walkthrough.md" => "󰄬 ",
            "task.md" => "󰄲 ",
            _ => " ",
        };

        let file_name = truncate_str(&artifact.file_name, 18);

        let size_kb = (artifact.size_bytes as f64) / 1024.0;
        let size_str = if size_kb < 1.0 {
            format!("{} B", artifact.size_bytes)
        } else {
            format!("{:.1} KB", size_kb)
        };

        let badge = if let Some(ref conv) = artifact.conversation_id {
            let short: String = conv.chars().take(6).collect();
            format!("[{}]", short)
        } else {
            "[local]".to_string()
        };

        let (name_style, badge_style) = if is_selected {
            (
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Cyan),
            )
        } else {
            (
                Style::default().fg(Color::White),
                Style::default().fg(Color::DarkGray),
            )
        };

        let line = Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(icon, Style::default().fg(Color::LightBlue)),
            Span::styled(format!("{:<18}", file_name), name_style),
            Span::styled(
                format!("{:>7} ", size_str),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(badge, badge_style),
        ]);

        items.push(ListItem::new(line));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

/// High-resolution markdown document viewer
fn render_artifact_document_view(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.artifact_pane_focus == ArtifactPaneFocus::DocumentView;
    let border_color = if is_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    if app.artifacts.is_empty() || app.selected_artifact_idx >= app.artifacts.len() {
        return;
    }

    let block_proto = Block::default().borders(Borders::ALL);
    let inner_preview = block_proto.inner(area);
    let doc_width = inner_preview.width as usize;

    let artifact = &mut app.artifacts[app.selected_artifact_idx];
    if artifact.last_rendered_width != doc_width && doc_width >= 10 {
        artifact.rendered_lines =
            crate::markdown_renderer::parse_markdown_with_width(&artifact.raw_content, doc_width);
        artifact.last_rendered_width = doc_width;
    }

    let total_lines = artifact.rendered_lines.len();
    let scroll_info = if app.artifact_scroll_offset > 0 {
        format!(
            " • L: {}/{} [Scroll +{}]",
            app.artifact_scroll_offset + 1,
            total_lines,
            app.artifact_scroll_offset
        )
    } else {
        format!(" • {} lines", total_lines)
    };

    let path_display = artifact.path.display().to_string();
    let title = format!("  {}{} ", artifact.file_name, scroll_info);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            title,
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        ))
        .title(
            Line::from(vec![Span::styled(
                format!(" {} ", path_display),
                Style::default().fg(Color::DarkGray),
            )])
            .alignment(ratatui::layout::Alignment::Right),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    let p = Paragraph::new(artifact.rendered_lines.clone())
        .wrap(ratatui::widgets::Wrap { trim: false })
        .scroll((app.artifact_scroll_offset as u16, 0));
    f.render_widget(p, inner);
}

/// Help Modal with Keybindings (Ctrl+H or F12)
fn render_help_modal(f: &mut Frame, app: &App) {
    let pal = app.palette;
    let area = centered_rect(76, 85, f.area());
    f.render_widget(Clear, area);

    let lines = vec![
        Line::from(vec![Span::styled(
            "󰌌 GLOBAL NAVIGATION & SHORTCUTS",
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )]),
        Line::from("──────────────────────────────────────────────────────────────────"),
        Line::from(vec![
            Span::styled(
                "  Mouse Click    ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Click tabs at top ([1] Git Diff / [2] Artifacts)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  1 / F1         ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Switch to [1] Git Diff"),
        ]),
        Line::from(vec![
            Span::styled(
                "  2 / F2         ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Switch to [2] Artifacts (when available)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+T / Tab   ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cycle between active tabs"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+A         ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Select Primary and Review AI from active Herdr agents"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+R         ",
                Style::default().fg(pal.mauve).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Submit 5-Lens Code Review to Review AI pane"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+S         ",
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Submit Review AI output to Primary AI for validation"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+H         ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle this help menu"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Ctrl+Q / q     ",
                Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Quit Herdr Interactive Diff"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "󰚩 HERDR DECOUPLED AGENTS WORKFLOW",
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        )]),
        Line::from("──────────────────────────────────────────────────────────────────"),
        Line::from(vec![
            Span::styled(
                "  <prefix>+f     ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Open herdr-interactive-diff on the left split"),
        ]),
        Line::from(vec![
            Span::styled(
                "  <prefix>+a     ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Focus Primary AI pane in Herdr"),
        ]),
        Line::from(vec![
            Span::styled(
                "  <prefix>+c     ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Focus Review AI pane in Herdr"),
        ]),
        Line::from(vec![
            Span::styled(
                "  <prefix>+r     ",
                Style::default().fg(pal.mauve).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Trigger 5-Lens Code Review (submits diff to Review AI)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  <prefix>+s     ",
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Trigger Anti-Overengineering Validation (submits review to Primary AI)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  ?              ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Ask Primary AI a question about selected diff hunk"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "󰊢 TAB 1: GIT DIFF & REVIEW COMMENTS",
            Style::default().fg(pal.mauve).add_modifier(Modifier::BOLD),
        )]),
        Line::from("──────────────────────────────────────────────────────────────────"),
        Line::from(vec![
            Span::styled(
                "  Mouse Click    ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Click file in drawer to inspect, or click line for review comments"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Mouse Drag     ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Select code block to copy to system clipboard"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Cmd+C / Ctrl+C ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Copy current selection or line to system clipboard (pbcopy)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  ?              ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Ask Primary AI a question about selected diff hunk"),
        ]),
        Line::from(vec![
            Span::styled(
                "  j / k / ↑ / ↓  ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate files (in drawer) or lines (in code viewer)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  f              ",
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle between 'Diff Only' and 'Full File' view mode"),
        ]),
        Line::from(vec![
            Span::styled(
                "  e / E          ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle file tree drawer"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Space / Enter  ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Toggle floating review tooltip (Caveman / Insight)"),
        ]),
        Line::from(vec![
            Span::styled(
                "  r / F5         ",
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Reload Git Diff from disk"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "󰈙 TAB 2: WORKSPACE & AI ARTIFACTS",
            Style::default().fg(pal.teal).add_modifier(Modifier::BOLD),
        )]),
        Line::from("──────────────────────────────────────────────────────────────────"),
        Line::from(vec![
            Span::styled(
                "  Mouse Click    ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Click artifact in drawer to open, click document to scroll"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Mouse Drag     ",
                Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Select document text to copy to system clipboard"),
        ]),
        Line::from(vec![
            Span::styled(
                "  j / k / ↑ / ↓  ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Navigate artifacts or scroll document"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Enter / l / →  ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Focus document reading view"),
        ]),
        Line::from(vec![
            Span::styled(
                "  Esc / h / ←    ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Return focus to artifact drawer"),
        ]),
        Line::from(vec![
            Span::styled(
                "  r / F5         ",
                Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Reload artifacts from workspace, Herdr state, and AI sessions"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[Esc / Enter / Ctrl+H] ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Close help menu", Style::default().fg(pal.subtext0)),
        ]),
    ];

    let total_lines = lines.len();
    let scroll_offset = app.help_scroll_offset;
    let scroll_info = if total_lines > 0 {
        format!(
            " • [Line {}/{}] [j/k/↑/↓: Scroll]",
            scroll_offset.min(total_lines) + 1,
            total_lines
        )
    } else {
        String::new()
    };

    let title = format!(
        " 󰋖 HERDR INTERACTIVE DIFF • KEYBINDINGS GUIDE{} ",
        scroll_info
    );

    let modal = Paragraph::new(lines)
        .scroll((scroll_offset as u16, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(pal.accent))
                .title(Span::styled(
                    title,
                    Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                )),
        );

    f.render_widget(modal, area);
}

/// Helper to render centered overlay rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Bottom bar for on-demand diff questioning
fn render_question_input_bar(f: &mut Frame, app: &App, area: Rect) {
    let pal = app.palette;
    let file = app.diff.files.get(app.selected_file_idx);
    let file_name = file.map(|f| f.file_name()).unwrap_or("file");

    let prompt_line = Line::from(vec![
        Span::styled(
            format!(" 󰌵 Question regarding [{}] > ", file_name),
            Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            &app.question_input,
            Style::default().fg(pal.text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("█", Style::default().fg(pal.accent)),
    ]);

    let input_widget = Paragraph::new(prompt_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(pal.yellow))
            .title(Span::styled(
                " 󰚩 ASK AI ABOUT THIS CODE • [Enter: Send to Agent | Esc: Cancel] ",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            )),
    );

    f.render_widget(input_widget, area);
}

/// Helper to wrap long lines
#[allow(dead_code)]
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    crate::markdown_renderer::wrap_text(text, max_width)
}

/// Truncates a string to at most `max_chars` unicode characters, appending '…' if truncated.
pub fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut res: String = s.chars().take(keep).collect();
    res.push('…');
    res
}

/// Renders the AI selection modal (Ctrl+A)
fn render_agent_picker_modal(f: &mut Frame, app: &App) {
    let pal = app.palette;
    let area = f.area();

    let width = 78.min(area.width.saturating_sub(4));
    let height = 20.min(area.height.saturating_sub(4));

    let modal_area = Rect {
        x: (area.width.saturating_sub(width)) / 2,
        y: (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    f.render_widget(Clear, modal_area);

    let is_primary = app.agent_picker_target == crate::app::AgentPickerTarget::Primary;
    let title_text = if is_primary {
        "Assign Primary AI (Herdr Agent / Pane)"
    } else {
        "Assign Review AI (Herdr Agent / Pane)"
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pal.accent))
        .title(Span::styled(
            format!(" 󰚩 {} ", title_text),
            Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(
            Line::from(vec![
                Span::styled(
                    " [Tab] ",
                    Style::default().fg(pal.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Role ", Style::default().fg(pal.subtext0)),
                Span::styled("• ", Style::default().fg(pal.overlay0)),
                Span::styled(
                    "[Enter] ",
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Assign ", Style::default().fg(pal.subtext0)),
                Span::styled("• ", Style::default().fg(pal.overlay0)),
                Span::styled(
                    "[Esc] ",
                    Style::default().fg(pal.red).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Close ", Style::default().fg(pal.subtext0)),
            ])
            .alignment(ratatui::layout::Alignment::Right),
        );

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let has_footer = inner.height >= 7;
    let chunks = if has_footer {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Target switcher row
                Constraint::Min(3),    // List of detected agents
                Constraint::Length(1), // Footer actions bar
            ])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Target switcher row
                Constraint::Min(3),    // List of detected agents
            ])
            .split(inner)
    };

    // Target switcher row
    let primary_style = if is_primary {
        Style::default()
            .fg(pal.panel_bg)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.subtext0)
    };
    let review_style = if !is_primary {
        Style::default()
            .fg(pal.panel_bg)
            .bg(pal.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(pal.subtext0)
    };

    let primary_indicator = if is_primary { "▶ " } else { "  " };
    let review_indicator = if !is_primary { "▶ " } else { "  " };

    let primary_desc = match &app.primary_pane_id {
        Some(pane) => format!(
            "pane {} ({})",
            pane,
            app.config.primary_agent.display_name()
        ),
        None => format!("{} (auto)", app.config.primary_agent.display_name()),
    };
    let review_desc = match &app.review_pane_id {
        Some(pane) => format!("pane {} ({})", pane, app.config.review_agent.display_name()),
        None => format!("{} (auto)", app.config.review_agent.display_name()),
    };

    let target_line = Line::from(vec![
        Span::styled(
            format!(" {}{}: {} ", primary_indicator, "Primary AI", primary_desc),
            primary_style,
        ),
        Span::raw("   "),
        Span::styled(
            format!(" {}{}: {} ", review_indicator, "Review AI", review_desc),
            review_style,
        ),
    ]);
    f.render_widget(Paragraph::new(target_line), chunks[0]);

    if app.detected_agents.is_empty() {
        let empty_msg = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No active AI agents detected in Herdr.",
                Style::default().fg(pal.yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  Launch an agent (e.g. agy, claude) in a Herdr pane or check socket.",
                Style::default().fg(pal.subtext0),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Defaulting to configured: Primary = agy, Review = claude.",
                Style::default().fg(pal.overlay0),
            )),
        ];
        f.render_widget(Paragraph::new(empty_msg), chunks[1]);
    } else {
        let items: Vec<ListItem> = app
            .detected_agents
            .iter()
            .enumerate()
            .map(|(idx, agent)| {
                let is_cursor = idx == app.agent_picker_idx;
                let is_selected_primary = app.primary_pane_id.as_deref() == Some(&agent.pane_id);
                let is_selected_review = app.review_pane_id.as_deref() == Some(&agent.pane_id);

                let status_symbol =
                    if agent.agent_status == "active" || agent.agent_status.contains("busy") {
                        "●"
                    } else {
                        "○"
                    };

                let mut role_tags = Vec::new();
                if is_selected_primary {
                    role_tags.push("[PRIMARY]");
                }
                if is_selected_review {
                    role_tags.push("[REVIEW]");
                }
                let role_str = if role_tags.is_empty() {
                    String::new()
                } else {
                    format!(" {}", role_tags.join(" "))
                };

                let cwd_display = agent
                    .cwd
                    .as_deref()
                    .or(agent.foreground_cwd.as_deref())
                    .map(|p| {
                        let name = p.split('/').rfind(|s| !s.is_empty()).unwrap_or(p);
                        format!(" ({})", name)
                    })
                    .unwrap_or_default();

                let line_str = format!(
                    "{} {} [pane {}] - {}{}{}",
                    status_symbol,
                    agent.agent,
                    agent.pane_id,
                    agent.agent_status,
                    cwd_display,
                    role_str
                );

                let style = if is_cursor {
                    Style::default()
                        .fg(pal.panel_bg)
                        .bg(pal.accent)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected_primary {
                    Style::default().fg(pal.green).add_modifier(Modifier::BOLD)
                } else if is_selected_review {
                    Style::default().fg(pal.mauve).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(pal.text)
                };

                ListItem::new(Line::from(Span::styled(line_str, style)))
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitDiff;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_render_top_bar_with_artifacts_and_without() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.active_tab = ActiveTab::GitDiff;

        let mut list_state = ListState::default();

        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        // Contains [1] and Git Diff and Ctrl+H for help, but not Artifacts when empty
        assert!(content.contains("[1]"));
        assert!(content.contains("Git Diff"));
        assert!(content.contains("Ctrl+H for help"));
        assert!(!content.contains("Artifacts"));

        // When artifacts present:
        let dummy_lines = crate::markdown_renderer::parse_markdown_to_lines("# Plan");
        app.artifacts.push(crate::markdown_renderer::ArtifactItem {
            file_name: "plan.md".to_string(),
            path: std::path::PathBuf::from("plan.md"),
            conversation_id: Some("conv-1".to_string()),
            size_bytes: 120,
            modified_str: "now".to_string(),
            raw_content: "# Plan".to_string(),
            rendered_lines: dummy_lines,
            last_rendered_width: 80,
        });

        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer2 = terminal.backend().buffer();
        let content2 = format!("{:?}", buffer2);
        assert!(content2.contains("[1]"));
        assert!(content2.contains("Git Diff"));
        assert!(content2.contains("[2]"));
        assert!(content2.contains("Artifacts"));
    }

    #[test]
    fn test_render_help_modal() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.show_help = true;

        let mut list_state = ListState::default();

        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        assert!(content.contains("KEYBINDINGS GUIDE"));
        assert!(content.contains("Ctrl+T"));
        assert!(content.contains("Ctrl+R"));
        assert!(content.contains("Ctrl+S"));
        assert!(content.contains("HERDR INTERACTIVE DIFF"));
        assert!(content.contains("Git Diff"));
    }

    #[test]
    fn test_render_header_top_right_and_no_footer() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, Some("/Users/kaleb/work/my-project".to_string()));
        app.active_tab = ActiveTab::GitDiff;

        let mut list_state = ListState::default();

        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        let line_0: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
        let line_1: String = (0..120).map(|x| buffer[(x, 1)].symbol()).collect();
        assert!(
            !line_0.contains("herdr-interactive-diff"),
            "Top border (line 0) must not contain repo title: {}",
            line_0
        );
        assert!(
            line_1.contains("herdr-interactive-diff • my-project"),
            "Inner line 1 must contain repo info: {}",
            line_1
        );
        assert!(
            line_1.contains("Ctrl+H for help"),
            "Inner line 1 must contain Ctrl+H for help: {}",
            line_1
        );
    }

    #[test]
    fn test_render_git_diff_code_view_title_only_path() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.active_tab = ActiveTab::GitDiff;
        app.diff_pane_focus = DiffPaneFocus::CodeView;

        let mut list_state = ListState::default();

        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        assert!(content.contains("src/auth/session.rs"));
        assert!(!content.contains("'f': alternar"));
        assert!(!content.contains("Tooltip]"));
        assert!(!content.contains("Voltar Tree"));
    }

    #[test]
    fn test_render_artifacts_view() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);

        // Case 1: Empty artifacts view displays clean informational screen
        app.artifacts.clear();
        terminal
            .draw(|f| {
                render_artifacts_view(f, &mut app, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content_no_artifacts = format!("{:?}", buffer);
        assert!(content_no_artifacts.contains("No Gemini / Antigravity artifacts"));
        assert!(content_no_artifacts.contains("HERDR • ARTIFACTS"));

        // Case 2: With artifacts -> renders [2] Artifacts tab and file in drawer
        let dummy_lines = crate::markdown_renderer::parse_markdown_to_lines("# Test Plan\nContent");
        app.artifacts.push(crate::markdown_renderer::ArtifactItem {
            file_name: "plan.md".to_string(),
            path: std::path::PathBuf::from("plan.md"),
            conversation_id: Some("conv-1".to_string()),
            size_bytes: 120,
            modified_str: "now".to_string(),
            raw_content: "# Test Plan".to_string(),
            rendered_lines: dummy_lines,
            last_rendered_width: 80,
        });
        app.active_tab = ActiveTab::Artifacts;
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer2 = terminal.backend().buffer();
        let content_with_artifacts = format!("{:?}", buffer2);
        assert!(content_with_artifacts.contains("Artifacts"));
        assert!(content_with_artifacts.contains("plan.md"));
    }

    #[test]
    fn test_render_top_bar_narrow_width_preserves_artifacts_tab() {
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(
            diff,
            Some("/Users/kaleb/work/very-long-project-name".to_string()),
        );
        let dummy_lines = crate::markdown_renderer::parse_markdown_to_lines("# Test Plan");
        app.artifacts.push(crate::markdown_renderer::ArtifactItem {
            file_name: "implementation_plan.md".to_string(),
            path: std::path::PathBuf::from("plan.md"),
            conversation_id: Some("conv-1".to_string()),
            size_bytes: 120,
            modified_str: "now".to_string(),
            raw_content: "# Test Plan".to_string(),
            rendered_lines: dummy_lines,
            last_rendered_width: 90,
        });

        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        // Even at narrow width, both tabs must be rendered without truncation or dropping
        assert!(content.contains("[1]"), "Must have tab 1 index");
        assert!(content.contains("Git Diff"), "Must have tab 1 title");
        assert!(content.contains("[2]"), "Must have tab 2 index");
        assert!(content.contains("Artifacts"), "Must have tab 2 title");
    }

    #[test]
    fn test_render_code_view_comment_framed_boxes() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        let demo_classifications = crate::ai_engine::ClassificationResponse::demo();
        app.set_classifications(demo_classifications.classifications);
        app.active_tab = ActiveTab::GitDiff;
        app.diff_pane_focus = DiffPaneFocus::CodeView;

        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        // Header border ╭── and Footer border ╰── must be visible
        assert!(content.contains("╭──"), "Comment box must open with ╭──");
        assert!(content.contains("╰──"), "Comment box must close with ╰──");
        assert!(content.contains("│"), "Comment box must frame lines with │");
        assert!(
            content.contains("[HIGH]"),
            "Comment header must use [HIGH] badge"
        );
        assert!(
            !content.contains("HIGH PRIORITY"),
            "Must not use old HIGH PRIORITY label"
        );
    }

    #[test]
    fn test_render_floating_tooltip_dynamic_growth_and_wrap() {
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        let demo_classifications = crate::ai_engine::ClassificationResponse::demo();
        app.set_classifications(demo_classifications.classifications);
        app.active_tab = ActiveTab::GitDiff;
        app.diff_pane_focus = DiffPaneFocus::CodeView;

        // Position cursor at caveman hunk
        let caveman_idx = app.code_lines.iter().position(|l| {
            matches!(
                l,
                CodeLineDisplay::DiffLine {
                    has_caveman: true,
                    ..
                }
            )
        });
        assert!(caveman_idx.is_some());
        app.code_cursor_idx = caveman_idx.unwrap();
        app.show_tooltip = true;

        // Verify calculate_tooltip_geometry produces dynamic dimensions > 8 height
        let code_area = Rect::new(34, 3, 86, 57);
        let (rect, lines) = calculate_tooltip_geometry(&app, code_area).unwrap();
        assert!(
            rect.height >= 8,
            "Tooltip height should dynamically grow, got {}",
            rect.height
        );
        assert!(rect.width <= 86, "Tooltip width should fit in code area");
        assert!(!lines.is_empty(), "Tooltip should contain rendered lines");

        // Verify tooltip rendering
        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        assert!(
            content.contains("REVIEW COMMENT"),
            "Tooltip title must be rendered"
        );
        assert!(
            content.contains("PONT NO CHECK"),
            "Tooltip summary must be rendered"
        );
        assert!(
            content.contains("Close"),
            "Close shortcut hint must be rendered"
        );
    }

    #[test]
    fn test_tooltip_kinds_severity_styling() {
        let t_high = TooltipData {
            title: "REVIEW COMMENT [HIGH]".to_string(),
            kind: TooltipKind::High,
            summary: "Crash danger".to_string(),
            details: "Fix this null pointer".to_string(),
            hint: None,
        };
        let lines_high = get_tooltip_rendered_lines(&t_high, 60);
        assert!(!lines_high.is_empty());

        let t_med = TooltipData {
            title: "REVIEW COMMENT [MEDIUM]".to_string(),
            kind: TooltipKind::Medium,
            summary: "Performance warning".to_string(),
            details: "Unnecessary lock".to_string(),
            hint: None,
        };
        let lines_med = get_tooltip_rendered_lines(&t_med, 60);
        assert!(!lines_med.is_empty());

        let t_low = TooltipData {
            title: "REVIEW COMMENT [LOW]".to_string(),
            kind: TooltipKind::Low,
            summary: "Style suggestion".to_string(),
            details: "Consider iterator".to_string(),
            hint: None,
        };
        let lines_low = get_tooltip_rendered_lines(&t_low, 60);
        assert!(!lines_low.is_empty());
    }

    #[test]
    fn test_utf8_char_boundary_comment_framing_and_truncate() {
        // String with em-dashes (—), Portuguese accents (ã, é, ç), and emojis (🚀)
        let s = "Refatoração de código — validação com IA e execução de testes 🚀 com sucesso!";

        // Truncate at every single character boundary up to length
        for len in 0..=s.chars().count() + 5 {
            let res = truncate_str(s, len);
            assert!(res.chars().count() <= len);
        }

        // Test rendering CommentHeader and CommentFooter with multibyte characters
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.active_tab = ActiveTab::GitDiff;
        app.diff_pane_focus = DiffPaneFocus::CodeView;
        app.show_diff_tree = false;

        // Force a comment line with em-dashes
        app.code_lines.insert(
            0,
            crate::app::CodeLineDisplay::CommentHeader {
                hunk_id: "hunk-0".to_string(),
                level: crate::ai_engine::ComplexityLevel::Forte,
                title: "REVISÃO — TESTE DE CARACTERES MULTIBYTE".to_string(),
            },
        );
        app.code_lines.insert(
            1,
            crate::app::CodeLineDisplay::CommentFooter {
                hunk_id: "hunk-0".to_string(),
                level: crate::ai_engine::ComplexityLevel::Forte,
                fix_or_hint: "Sugestão de correção — rodapé seguro".to_string(),
            },
        );

        let mut list_state = ListState::default();
        // Must NOT panic with "not a char boundary; it is inside '—'"
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);
        assert!(content.contains("REVISÃO"));
    }

    #[test]
    fn test_render_agent_picker_modal_layout() {
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let diff = GitDiff::demo();
        let mut app = App::new(diff, None);
        app.show_agent_picker = true;
        app.agent_picker_target = crate::app::AgentPickerTarget::Primary;

        let mut list_state = ListState::default();
        terminal
            .draw(|f| {
                render_ui(f, &mut app, &mut list_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content = format!("{:?}", buffer);

        // Title should be present on top
        assert!(
            content.contains("Assign Primary AI"),
            "Must contain modal title"
        );
        // Footer shortcuts should be rendered cleanly at the bottom
        assert!(
            content.contains("Switch Role"),
            "Must contain Switch Role action"
        );
        assert!(content.contains("Assign"), "Must contain Assign action");
        assert!(content.contains("Close"), "Must contain Close action");
    }
}
