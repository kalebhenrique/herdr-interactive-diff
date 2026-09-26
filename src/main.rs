mod ai_engine;
mod app;
pub mod clipboard;
mod completions;
mod config;
mod git;
mod herdr;
mod markdown_renderer;
mod pipeline;
mod terminal_session;
pub mod theme;
mod ui;

use std::io::stdout;
use std::time::Duration;
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEventKind,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use tokio::sync::mpsc;

use crate::ai_engine::{classify_diff_with_antigravity, ClassificationResponse};
use crate::app::{ActiveTab, AgentFocus, App, ArtifactPaneFocus, CodeLineDisplay, DiffPaneFocus};
use crate::git::GitDiff;
use crate::herdr::HerdrClient;

/// Asynchronous messages sent from background workers to UI
pub enum AsyncAction {
    ClassificationReady(Result<ClassificationResponse, String>),
    DiffUpdated(GitDiff),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set panic hook to restore terminal in case of unexpected errors
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        if let Some(mut h) = HerdrClient::try_detect() {
            h.release_all();
        }
        default_panic(info);
    }));

    // Auto-install zsh completions silently if standard directories are available
    completions::try_auto_install_zsh();

    let args: Vec<String> = std::env::args().collect();
    let mut is_demo = false;
    let mut target_dir: Option<String> = None;
    let mut set_primary: Option<String> = None;
    let mut set_review: Option<String> = None;
    let mut show_config = false;
    let mut run_setup = false;

    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-s" | "--start" => {
                if let Some(val) = iter.next() {
                    set_primary = Some(val);
                } else {
                    eprintln!("\n  Usage: herdr-interactive-diff -s <agent>\n");
                    return Ok(());
                }
            }
            "-r" | "--review" => {
                if let Some(val) = iter.next() {
                    set_review = Some(val);
                } else {
                    eprintln!("\n  Usage: herdr-interactive-diff -r <agent>\n");
                    return Ok(());
                }
            }
            "-c" | "--config" => {
                show_config = true;
            }
            "--setup" => {
                run_setup = true;
            }
            "--install-completions" => {
                completions::install_completions()?;
                return Ok(());
            }
            "--completions" => {
                let shell = iter.next().unwrap_or_else(|| "zsh".to_string());
                match shell.to_lowercase().as_str() {
                    "bash" => println!("{}", completions::BASH_COMPLETION),
                    "fish" => println!("{}", completions::FISH_COMPLETION),
                    _ => println!("{}", completions::ZSH_COMPLETION),
                }
                return Ok(());
            }
            "-h" | "--help" => {
                println!("\n  󰚩 HERDR INTERACTIVE DIFF • Herdr Plugin for AI Pair-Programming & Multi-Lens Review\n");
                println!("  USAGE:");
                println!("      herdr-interactive-diff [OPTIONS] [DIRECTORY]\n");
                println!("  OPTIONS:");
                println!("      -s, --start <AGENT>         Set primary agent to open on start (17 Herdr agents supported)");
                println!("      -r, --review <AGENT>        Set review agent for multi-lens review");
                println!("      -c, --config                Display current AI agent configuration");
                println!("      --setup                     Rerun interactive first-time AI setup wizard");
                println!("      --install-completions       Install shell autocomplete (zsh, bash, fish)");
                println!("      --completions <SHELL>       Generate completion script (zsh, bash, fish)");
                println!("      --demo                      Start in demo mode with mock data");
                println!("      -p, --path <DIR>            Specify working repository directory");
                println!("      -h, --help                  Display this help menu\n");
                println!("  SUPPORTED HERDR AGENTS:");
                println!("      pi, amp, claude, codex, copilot, devin, droid, kimi, opencode,");
                println!("      kilo, hermes, qodercli, qwen, cursor, mastracode, antigravity (agy), grok\n");
                return Ok(());
            }
            "--demo" => is_demo = true,
            "--path" | "-p" => {
                if let Some(p) = iter.next() {
                    target_dir = Some(p);
                }
            }
            _ if !arg.starts_with('-') && target_dir.is_none() => {
                target_dir = Some(arg);
            }
            _ => {}
        }
    }

    if show_config {
        config::print_config();
        return Ok(());
    }

    if run_setup {
        let _ = config::prompt_first_time_setup()?;
        return Ok(());
    }

    if set_primary.is_some() || set_review.is_some() {
        let mut cfg = config::load_config();
        if let Some(p_str) = set_primary {
            if let Some(agent) = config::AgentKind::parse(&p_str) {
                cfg.primary_agent = agent;
            } else {
                eprintln!("\n  ✖ Invalid agent '{}' for primary. Supported agents: pi, amp, claude, codex, copilot, devin, droid, kimi, opencode, kilo, hermes, qodercli, qwen, cursor, mastracode, agy, grok\n", p_str);
                return Ok(());
            }
        }
        if let Some(r_str) = set_review {
            if let Some(agent) = config::AgentKind::parse(&r_str) {
                cfg.review_agent = agent;
            } else {
                eprintln!("\n  ✖ Invalid agent '{}' for review. Supported agents: pi, amp, claude, codex, copilot, devin, droid, kimi, opencode, kilo, hermes, qodercli, qwen, cursor, mastracode, agy, grok\n", r_str);
                return Ok(());
            }
        }
        config::save_config(&cfg)?;
        println!("\n  ✔ Herdr Interactive Diff configuration updated:");
        println!("    • Primary AI (opens on start):  {}", cfg.primary_agent.display_name());
        println!("    • Review AI (multi-lens review): {}", cfg.review_agent.display_name());
        println!("    Saved to {}\n", config::get_config_path().display());
        println!("  Run 'herdr-interactive-diff' or use Herdr plugin to start.\n");
        return Ok(());
    }

    // First-time setup wizard if no configuration exists yet
    let app_config = if !config::config_exists() {
        config::prompt_first_time_setup()?
    } else {
        config::load_config()
    };

    // Expande til (~) se presente no caminho do diretório, ou usa o diretório de trabalho atual
    let target_dir = target_dir
        .map(|dir| {
            if let Some(stripped) = dir.strip_prefix("~/") {
                if let Ok(home) = std::env::var("HOME") {
                    format!("{}/{}", home, stripped)
                } else {
                    dir
                }
            } else {
                dir
            }
        })
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
        });

    // 1. Extração do Diff
    let diff = if is_demo {
        GitDiff::demo()
    } else {
        match GitDiff::from_local_repo(target_dir.as_deref()) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Warning: Failed to load git repository ({})", e);
                GitDiff::default()
            }
        }
    };

    let mut app = App::new_with_config(diff, target_dir.clone(), app_config);
    let mut list_state = ListState::default();
    list_state.select(Some(app.code_cursor_idx));

    // Canal assíncrono para background workers
    let (tx, mut rx) = mpsc::unbounded_channel::<AsyncAction>();

    // 2. Disparo da Classificação da IA para enriquecer o Git Diff (Tab 2)
    if is_demo {
        let demo_resp = ClassificationResponse::demo();
        app.set_classifications(demo_resp.classifications);
    } else {
        app.is_classifying = true;
        let tx_classify = tx.clone();
        let raw_diff = app.diff.raw.clone();
        tokio::spawn(async move {
            let res = classify_diff_with_antigravity(&raw_diff).await;
            let _ = tx_classify.send(AsyncAction::ClassificationReady(
                res.map_err(|e| e.to_string()),
            ));
        });

        // Background Watcher: Atualização contínua do Git Diff caso o código seja modificado
        let tx_diff = tx.clone();
        let watch_dir = target_dir.clone();
        let initial_raw = app.diff.raw.clone();
        tokio::spawn(async move {
            let mut last_raw = initial_raw;
            let mut interval = tokio::time::interval(Duration::from_millis(1500));
            interval.tick().await;

            loop {
                interval.tick().await;
                let dir_copy = watch_dir.clone();
                let diff_res = tokio::task::spawn_blocking(move || {
                    GitDiff::from_local_repo(dir_copy.as_deref())
                })
                .await;

                if let Ok(Ok(new_diff)) = diff_res {
                    if new_diff.raw != last_raw {
                        last_raw = new_diff.raw.clone();
                        let _ = tx_diff.send(AsyncAction::DiffUpdated(new_diff));
                    }
                }
            }
        });
    }

    // 3. Inicialização do Terminal Ratatui
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let enhanced_keyboard = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced_keyboard {
        let _ = execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 4. Loop Principal de Eventos e Renderização
    let mut last_sync_tick = std::time::Instant::now();
    let mut last_theme_mtime = None;
    while !app.should_quit {
        list_state.select(Some(app.code_cursor_idx));

        // Hot-reload do tema do Herdr em tempo real caso ~/.config/herdr/config.toml seja modificado
        if let Some(new_palette) = theme::check_theme_modified(&mut last_theme_mtime) {
            app.palette = new_palette;
        }

        // Renderiza
        terminal.draw(|f| {
            ui::render_ui(f, &mut app, &mut list_state);
        })?;
        app.code_scroll_offset = list_state.offset();

        // Sincroniza estado do Herdr, comentários do Claude e artefatos periodicamente
        if last_sync_tick.elapsed() >= Duration::from_millis(150) {
            app.update_herdr_state();
            app.sync_claude_review_to_diff();
            app.check_artifacts_update();
            last_sync_tick = std::time::Instant::now();
        }

        // Processa mensagens assíncronas do canal
        while let Ok(action) = rx.try_recv() {
            match action {
                AsyncAction::ClassificationReady(result) => {
                    match result {
                        Ok(response) => {
                            app.set_classifications(response.classifications);
                        }
                        Err(err) => {
                            app.is_classifying = false;
                            app.status_message = format!("AI classification failed: {}", err);
                        }
                    }
                }
                AsyncAction::DiffUpdated(new_diff) => {
                    app.update_diff(new_diff);
                }
            }
        }

        // Captura de eventos do usuário com polling não-bloqueante
        if event::poll(Duration::from_millis(30))? {
            let ev = event::read()?;

            // Mouse support: click to switch tabs/focus and scroll to browse history
            if let Event::Mouse(mouse) = ev {
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        if app.show_help {
                            app.scroll_help(-3);
                            continue;
                        }
                        match app.active_tab {
                            ActiveTab::Agents => {
                                let term_width = terminal.size().map(|s| s.width).unwrap_or(120);
                                if !app.show_review_agent || mouse.column < term_width / 2 {
                                    if let Some(session) = &app.agy_session {
                                        let offset = session.scroll_up(3);
                                        app.status_message = format!("Primary AI: scrollback +{} lines", offset);
                                    }
                                } else if let Some(session) = &app.claude_session {
                                    let offset = session.scroll_up(3);
                                    app.status_message = format!("Review AI: scrollback +{} lines", offset);
                                }
                            }
                            ActiveTab::GitDiff => {
                                let term_size = terminal.size().unwrap_or_default();
                                let term_rect = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
                                let is_over_tooltip = if app.show_tooltip {
                                    if let Some(t_rect) = ui::calculate_tooltip_rect(&app, term_rect) {
                                        mouse.column >= t_rect.x
                                            && mouse.column < t_rect.right()
                                            && mouse.row >= t_rect.y
                                            && mouse.row < t_rect.bottom()
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                if is_over_tooltip {
                                    app.scroll_tooltip(-3);
                                } else if app.show_diff_tree && mouse.column < 34 {
                                    app.selected_file_idx = app.selected_file_idx.saturating_sub(1);
                                    app.code_cursor_idx = 0;
                                    app.code_scroll_offset = 0;
                                    app.show_tooltip = false;
                                    app.tooltip_scroll_offset = 0;
                                    app.rebuild_code_lines();
                                } else {
                                    app.code_cursor_idx = app.code_cursor_idx.saturating_sub(3);
                                }
                            }
                            ActiveTab::Artifacts => {
                                if app.show_artifact_tree && mouse.column < 36 {
                                    app.select_prev_artifact();
                                } else {
                                    app.scroll_artifact(-3);
                                }
                            }
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if app.show_help {
                            app.scroll_help(3);
                            continue;
                        }
                        match app.active_tab {
                            ActiveTab::Agents => {
                                let term_width = terminal.size().map(|s| s.width).unwrap_or(120);
                                if !app.show_review_agent || mouse.column < term_width / 2 {
                                    if let Some(session) = &app.agy_session {
                                        let offset = session.scroll_down(3);
                                        app.status_message = format!("Primary AI: scrollback +{} lines", offset);
                                    }
                                } else if let Some(session) = &app.claude_session {
                                    let offset = session.scroll_down(3);
                                    app.status_message = format!("Review AI: scrollback +{} lines", offset);
                                }
                            }
                            ActiveTab::GitDiff => {
                                let term_size = terminal.size().unwrap_or_default();
                                let term_rect = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
                                let is_over_tooltip = if app.show_tooltip {
                                    if let Some(t_rect) = ui::calculate_tooltip_rect(&app, term_rect) {
                                        mouse.column >= t_rect.x
                                            && mouse.column < t_rect.right()
                                            && mouse.row >= t_rect.y
                                            && mouse.row < t_rect.bottom()
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                if is_over_tooltip {
                                    app.scroll_tooltip(3);
                                } else if app.show_diff_tree && mouse.column < 34 {
                                    if app.selected_file_idx + 1 < app.diff.files.len() {
                                        app.selected_file_idx += 1;
                                        app.code_cursor_idx = 0;
                                        app.code_scroll_offset = 0;
                                        app.show_tooltip = false;
                                        app.tooltip_scroll_offset = 0;
                                        app.rebuild_code_lines();
                                    }
                                } else if app.code_cursor_idx + 3 < app.code_lines.len() {
                                    app.code_cursor_idx += 3;
                                }
                            }
                            ActiveTab::Artifacts => {
                                if app.show_artifact_tree && mouse.column < 36 {
                                    app.select_next_artifact();
                                } else {
                                    app.scroll_artifact(3);
                                }
                            }
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if app.show_help {
                            app.show_help = false;
                            continue;
                        }

                        // 1. Click on Top Bar (first 3 rows): switch tabs with mouse
                        if mouse.row <= 2 {
                            if mouse.column <= 16 {
                                app.active_tab = ActiveTab::Agents;
                                app.sync_herdr_agent_state("idle");
                                app.status_message = "Switched to [1] Agents.".to_string();
                            } else if mouse.column <= 34 {
                                app.active_tab = ActiveTab::GitDiff;
                                app.sync_claude_review_to_diff();
                                app.refresh_diff();
                            } else if mouse.column <= 52 && app.has_artifacts() {
                                app.refresh_artifacts();
                                if app.has_artifacts() {
                                    app.active_tab = ActiveTab::Artifacts;
                                }
                            }
                            continue;
                        }

                        // 2. Click in Main Content
                        match app.active_tab {
                            ActiveTab::Agents => {
                                if app.show_review_agent {
                                    let term_width = terminal.size().map(|s| s.width).unwrap_or(120);
                                    if mouse.column < term_width / 2 {
                                        app.agent_focus = AgentFocus::Antigravity;
                                    } else {
                                        app.agent_focus = AgentFocus::Claude;
                                    }
                                }
                            }
                            ActiveTab::GitDiff => {
                                app.mouse_drag_start = Some((mouse.column, mouse.row));
                                app.mouse_drag_end = None;
                                if app.show_diff_tree && mouse.column < 34 {
                                    app.diff_pane_focus = DiffPaneFocus::FileList;
                                    if mouse.row >= 4 {
                                        let clicked_file = (mouse.row - 4) as usize;
                                        if clicked_file < app.diff.files.len() {
                                            app.selected_file_idx = clicked_file;
                                            app.code_cursor_idx = 0;
                                            app.code_scroll_offset = 0;
                                            app.show_tooltip = false;
                                            app.rebuild_code_lines();
                                        }
                                    }
                                } else {
                                    app.diff_pane_focus = DiffPaneFocus::CodeView;
                                    if mouse.row >= 4 {
                                        let rel_row = (mouse.row - 4) as usize;
                                        let clicked_idx = app.code_scroll_offset + rel_row;

                                        let term_size = terminal.size().unwrap_or_default();
                                        let term_rect = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
                                        let clicked_inside_tooltip = if app.show_tooltip {
                                            if let Some(t_rect) = ui::calculate_tooltip_rect(&app, term_rect) {
                                                mouse.column >= t_rect.x
                                                    && mouse.column < t_rect.right()
                                                    && mouse.row >= t_rect.y
                                                    && mouse.row < t_rect.bottom()
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        };

                                        if clicked_inside_tooltip {
                                            app.show_tooltip = false;
                                            app.tooltip_scroll_offset = 0;
                                        } else if clicked_idx < app.code_lines.len() {
                                            let prev_idx = app.code_cursor_idx;
                                            let was_open = app.show_tooltip;
                                            app.code_cursor_idx = clicked_idx;

                                            if app.get_active_tooltip().is_some() {
                                                app.show_tooltip = !(was_open && prev_idx == clicked_idx);
                                                app.tooltip_scroll_offset = 0;
                                            } else {
                                                app.show_tooltip = false;
                                                app.tooltip_scroll_offset = 0;
                                            }
                                        }
                                    }
                                }
                            }
                            ActiveTab::Artifacts => {
                                app.mouse_drag_start = Some((mouse.column, mouse.row));
                                app.mouse_drag_end = None;
                                if app.show_artifact_tree && mouse.column < 36 {
                                    app.artifact_pane_focus = ArtifactPaneFocus::FileList;
                                    if mouse.row >= 4 {
                                        let clicked = (mouse.row - 4) as usize;
                                        if clicked < app.artifacts.len() {
                                            app.selected_artifact_idx = clicked;
                                            app.artifact_scroll_offset = 0;
                                            app.artifact_cursor_idx = 0;
                                        }
                                    }
                                } else {
                                    app.artifact_pane_focus = ArtifactPaneFocus::DocumentView;
                                }
                            }
                        }
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        if app.active_tab != ActiveTab::Agents {
                            app.mouse_drag_end = Some((mouse.column, mouse.row));
                        }
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let (Some(start), Some(end)) = (app.mouse_drag_start, app.mouse_drag_end) {
                            let row_diff = (start.1 as i32 - end.1 as i32).abs();
                            let col_diff = (start.0 as i32 - end.0 as i32).abs();
                            if row_diff > 0 || col_diff > 4 {
                                match app.active_tab {
                                    ActiveTab::Agents => {} // No drag-copy in agent terminals
                                    ActiveTab::GitDiff => {
                                        let min_row = start.1.min(end.1);
                                        let max_row = start.1.max(end.1);
                                        if min_row >= 3 {
                                            let rel_start = (min_row.saturating_sub(3) as usize) + app.code_scroll_offset;
                                            let rel_end = (max_row.saturating_sub(3) as usize) + app.code_scroll_offset;
                                            let mut selected = Vec::new();
                                            for i in rel_start..=rel_end {
                                                if i < app.code_lines.len() {
                                                    match &app.code_lines[i] {
                                                        CodeLineDisplay::DiffLine { content, .. } => selected.push(content.clone()),
                                                        CodeLineDisplay::FullFileLine { content, .. } => selected.push(content.clone()),
                                                        CodeLineDisplay::HunkHeader { header, .. } => selected.push(header.clone()),
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            if !selected.is_empty() {
                                                let text = selected.join("\n");
                                                let count = selected.len();
                                                let _ = crate::clipboard::copy_to_clipboard(&text);
                                                app.status_message = format!("Copied {} line(s) to system clipboard (Cmd+C/pbcopy).", count);
                                            }
                                        }
                                    }
                                    ActiveTab::Artifacts => {
                                        let min_row = start.1.min(end.1);
                                        let max_row = start.1.max(end.1);
                                        if min_row >= 3 && !app.artifacts.is_empty() && app.selected_artifact_idx < app.artifacts.len() {
                                            let art = &app.artifacts[app.selected_artifact_idx];
                                            let rel_start = (min_row.saturating_sub(3) as usize) + app.artifact_scroll_offset;
                                            let rel_end = (max_row.saturating_sub(3) as usize) + app.artifact_scroll_offset;
                                            let mut selected = Vec::new();
                                            for i in rel_start..=rel_end {
                                                if i < art.rendered_lines.len() {
                                                    let line_text: String = art.rendered_lines[i].spans.iter().map(|s| s.content.as_ref()).collect();
                                                    selected.push(line_text);
                                                }
                                            }
                                            if !selected.is_empty() {
                                                let text = selected.join("\n");
                                                let count = selected.len();
                                                let _ = crate::clipboard::copy_to_clipboard(&text);
                                                app.status_message = format!("Copied {} artifact line(s) to system clipboard (Cmd+C/pbcopy).", count);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        app.mouse_drag_start = None;
                        app.mouse_drag_end = None;
                    }
                    _ => {}
                }
            } else if let Event::Key(key) = ev {
                if key.kind == KeyEventKind::Press {
                    // Identifica se a tecla Control foi pressionada
                    let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                    // If question input overlay for AI about diff is active, capture input
                    if app.is_asking_question {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_asking_question = false;
                                app.question_input.clear();
                                app.status_message = "Question canceled.".to_string();
                            }
                            KeyCode::Enter => {
                                app.submit_question_to_agent();
                            }
                            KeyCode::Backspace => {
                                app.question_input.pop();
                            }
                            KeyCode::Char(c) if !is_ctrl => {
                                app.question_input.push(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Help modal (?) takes priority when open
                    if app.show_help {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter | KeyCode::F(12) => {
                                app.show_help = false;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.scroll_help(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.scroll_help(-1);
                            }
                            KeyCode::PageDown => {
                                app.scroll_help(10);
                            }
                            KeyCode::PageUp => {
                                app.scroll_help(-10);
                            }
                            _ => {
                                if is_ctrl
                                    && (key.code == KeyCode::Char('h')
                                        || key.code == KeyCode::Char('H')
                                        || key.code == KeyCode::Char('/'))
                                {
                                    app.show_help = false;
                                }
                            }
                        }
                        continue;
                    }

                    // Global shortcut to Quit: Ctrl+Q
                    if is_ctrl && (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q')) {
                        app.should_quit = true;
                        continue;
                    }

                    // Global shortcut for Help: Ctrl+H, Ctrl+/, or F12
                    if (is_ctrl
                        && (key.code == KeyCode::Char('h')
                            || key.code == KeyCode::Char('H')
                            || key.code == KeyCode::Char('/')
                            || key.code == KeyCode::Char('?')))
                        || key.code == KeyCode::F(12)
                    {
                        app.show_help = true;
                        app.help_scroll_offset = 0;
                        continue;
                    }

                    // Global shortcut: Ctrl+R launches or toggles Review AI split
                    if is_ctrl && (key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R')) {
                        app.active_tab = ActiveTab::Agents;
                        app.toggle_review_agent();
                        continue;
                    }

                    // Global shortcut: Ctrl+A opens AI Picker Modal
                    if is_ctrl && (key.code == KeyCode::Char('a') || key.code == KeyCode::Char('A')) {
                        app.open_agent_picker();
                        continue;
                    }

                    // Global shortcut to Cycle Tabs: Ctrl+T
                    if is_ctrl && (key.code == KeyCode::Char('t') || key.code == KeyCode::Char('T')) {
                        app.active_tab = match app.active_tab {
                            ActiveTab::Agents => {
                                app.sync_claude_review_to_diff();
                                app.refresh_diff();
                                ActiveTab::GitDiff
                            }
                            ActiveTab::GitDiff => {
                                app.refresh_artifacts();
                                if app.has_artifacts() {
                                    ActiveTab::Artifacts
                                } else {
                                    ActiveTab::Agents
                                }
                            }
                            ActiveTab::Artifacts => ActiveTab::Agents,
                        };
                        continue;
                    }

                    // Global Tab Shortcuts: F1 / Ctrl+1 (Agents), F2 / Ctrl+2 (Git Diff), F3 / Ctrl+3 (Artifacts)
                    if key.code == KeyCode::F(1) || (is_ctrl && key.code == KeyCode::Char('1')) {
                        app.active_tab = ActiveTab::Agents;
                        continue;
                    }
                    if key.code == KeyCode::F(2) || (is_ctrl && key.code == KeyCode::Char('2')) {
                        app.sync_claude_review_to_diff();
                        app.refresh_diff();
                        app.active_tab = ActiveTab::GitDiff;
                        continue;
                    }
                    if key.code == KeyCode::F(3) || (is_ctrl && key.code == KeyCode::Char('3')) {
                        app.refresh_artifacts();
                        if app.has_artifacts() {
                            app.active_tab = ActiveTab::Artifacts;
                        } else {
                            app.status_message = "No artifacts available for current Antigravity session.".to_string();
                        }
                        continue;
                    }

                    // AI Picker Modal input handling
                    if app.show_agent_picker {
                        match key.code {
                            KeyCode::Esc => app.close_agent_picker(),
                            KeyCode::Enter => app.confirm_agent_picker(),
                            KeyCode::Tab | KeyCode::Left | KeyCode::Right => app.toggle_agent_picker_target(),
                            KeyCode::Char('j') | KeyCode::Down => app.agent_picker_next(),
                            KeyCode::Char('k') | KeyCode::Up => app.agent_picker_prev(),
                            _ => {}
                        }
                        continue;
                    }

                    match app.active_tab {

                        // TAB 1: AGENTS (Primary AI + optional Review AI split)
                        ActiveTab::Agents => {
                            if app.is_dual_agent_active()
                                && is_ctrl
                                && (key.code == KeyCode::Char('o') || key.code == KeyCode::Char('O')
                                    || key.code == KeyCode::Char('w') || key.code == KeyCode::Char('W'))
                            {
                                app.toggle_agent_focus();
                                continue;
                            }
                            let bytes = key_event_to_bytes(&key);
                            if !bytes.is_empty() {
                                app.send_key_to_agent(&bytes);
                            }
                        }

                        // TAB 2: GIT DIFF (File list + Diff/FullFile viewer + Floating tooltip)
                        ActiveTab::GitDiff => {
                            if is_ctrl && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')) {
                                if let Some(line) = app.code_lines.get(app.code_cursor_idx) {
                                    let text = match line {
                                        CodeLineDisplay::DiffLine { content, .. } => content.clone(),
                                        CodeLineDisplay::FullFileLine { content, .. } => content.clone(),
                                        CodeLineDisplay::HunkHeader { header, .. } => header.clone(),
                                        _ => String::new(),
                                    };
                                    if !text.is_empty() {
                                        let _ = crate::clipboard::copy_to_clipboard(&text);
                                        app.status_message = "Copied line to system clipboard (Cmd+C/pbcopy).".to_string();
                                    }
                                }
                                continue;
                            }
                            if is_ctrl && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
                                app.trigger_antigravity_validation();
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') => {
                                    app.should_quit = true;
                                }
                                // Key '?' opens bottom prompt to ask AI about selected code
                                KeyCode::Char('?') => {
                                    app.is_asking_question = true;
                                    app.question_input.clear();
                                    app.status_message = "Type your question about the selected code and press [Enter] to ask AI.".to_string();
                                }
                                KeyCode::Char('1') => {
                                    app.active_tab = ActiveTab::Agents;
                                }
                                KeyCode::Char('2') => {
                                    app.sync_claude_review_to_diff();
                                    app.refresh_diff();
                                    app.active_tab = ActiveTab::GitDiff;
                                }
                                KeyCode::Char('3') => {
                                    app.refresh_artifacts();
                                    if app.has_artifacts() {
                                        app.active_tab = ActiveTab::Artifacts;
                                    } else {
                                        app.status_message = "No artifacts available for current session.".to_string();
                                    }
                                }
                                // Key 'r', 'R' or F5: Refresh diff
                                KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::F(5) if !is_ctrl => {
                                    app.refresh_diff();
                                }
                                // Key 'f': Toggle between Diff view and Full File view
                                KeyCode::Char('f') | KeyCode::Char('F') => {
                                    app.toggle_code_view_mode();
                                }
                                // Key 'e' or 'E': Toggle file tree drawer
                                KeyCode::Char('e') | KeyCode::Char('E') => {
                                    app.toggle_diff_tree();
                                }
                                _ => {
                                    match app.diff_pane_focus {
                                        // FILE LIST FOCUS
                                        DiffPaneFocus::FileList => match key.code {
                                            // Enter or 'l' / Right Arrow enters code view
                                            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                                                app.diff_pane_focus = DiffPaneFocus::CodeView;
                                                app.code_cursor_idx = 0;
                                                app.code_scroll_offset = 0;
                                                app.status_message = "Navigating code. [Esc] to return to file list.".to_string();
                                            }
                                            KeyCode::Char('j') | KeyCode::Down => {
                                                if app.selected_file_idx + 1 < app.diff.files.len() {
                                                    app.selected_file_idx += 1;
                                                    app.code_cursor_idx = 0;
                                                    app.code_scroll_offset = 0;
                                                    app.rebuild_code_lines();
                                                }
                                            }
                                            KeyCode::Char('k') | KeyCode::Up => {
                                                app.selected_file_idx = app.selected_file_idx.saturating_sub(1);
                                                app.code_cursor_idx = 0;
                                                app.code_scroll_offset = 0;
                                                app.rebuild_code_lines();
                                            }
                                            _ => {}
                                        },

                                        // CODE VIEW FOCUS
                                        DiffPaneFocus::CodeView => match key.code {
                                            // Esc or 'h' / Left Arrow returns to file list
                                            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                                                if app.show_tooltip {
                                                    app.show_tooltip = false;
                                                    app.tooltip_scroll_offset = 0;
                                                } else if app.show_diff_tree {
                                                    app.diff_pane_focus = DiffPaneFocus::FileList;
                                                    app.status_message = "File list focused. [Enter] to inspect code.".to_string();
                                                }
                                            }
                                            // Enter or Space toggles floating review tooltip
                                            KeyCode::Enter | KeyCode::Char(' ') => {
                                                app.toggle_tooltip();
                                            }
                                            KeyCode::Char('j') | KeyCode::Down => {
                                                if app.show_tooltip {
                                                    app.scroll_tooltip(1);
                                                } else if app.code_cursor_idx + 1 < app.code_lines.len() {
                                                    app.code_cursor_idx += 1;
                                                }
                                            }
                                            KeyCode::Char('k') | KeyCode::Up => {
                                                if app.show_tooltip {
                                                    app.scroll_tooltip(-1);
                                                } else {
                                                    app.code_cursor_idx = app.code_cursor_idx.saturating_sub(1);
                                                }
                                            }
                                            KeyCode::PageDown => {
                                                if app.show_tooltip {
                                                    app.scroll_tooltip(5);
                                                } else {
                                                    app.move_cursor_page(10);
                                                }
                                            }
                                            KeyCode::PageUp => {
                                                if app.show_tooltip {
                                                    app.scroll_tooltip(-5);
                                                } else {
                                                    app.move_cursor_page(-10);
                                                }
                                            }
                                            _ => {}
                                        },
                                    }
                                }
                            }
                        }

                        // TAB 3: ARTIFACTS (Workspace & AI Markdown Artifacts Viewer)
                        ActiveTab::Artifacts => {
                            if is_ctrl && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')) {
                                if !app.artifacts.is_empty() && app.selected_artifact_idx < app.artifacts.len() {
                                    let art = &app.artifacts[app.selected_artifact_idx];
                                    let _ = crate::clipboard::copy_to_clipboard(&art.raw_content);
                                    app.status_message = format!("Copied '{}' to system clipboard (Cmd+C/pbcopy).", art.file_name);
                                }
                                continue;
                            }
                            if is_ctrl && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
                                app.trigger_antigravity_validation();
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') => {
                                    app.should_quit = true;
                                }
                                KeyCode::Char('1') => {
                                    app.active_tab = ActiveTab::Agents;
                                }
                                KeyCode::Char('2') => {
                                    app.sync_claude_review_to_diff();
                                    app.refresh_diff();
                                    app.active_tab = ActiveTab::GitDiff;
                                }
                                KeyCode::Char('3') => {
                                    app.refresh_artifacts();
                                    if app.has_artifacts() {
                                        app.active_tab = ActiveTab::Artifacts;
                                    }
                                }
                                KeyCode::Char('y') if !is_ctrl => {
                                    if !app.artifacts.is_empty() && app.selected_artifact_idx < app.artifacts.len() {
                                        let art = &app.artifacts[app.selected_artifact_idx];
                                        let _ = crate::clipboard::copy_to_clipboard(&art.raw_content);
                                        app.status_message = format!("Yanked '{}' to system clipboard (Cmd+C/pbcopy).", art.file_name);
                                    }
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::F(5) if !is_ctrl => {
                                    app.refresh_artifacts();
                                }
                                // Key 'e' or 'E': Toggle artifact tree drawer
                                KeyCode::Char('e') | KeyCode::Char('E') => {
                                    app.toggle_artifact_tree();
                                }
                                _ => match app.artifact_pane_focus {
                                    ArtifactPaneFocus::FileList => match key.code {
                                        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                                            app.artifact_pane_focus = ArtifactPaneFocus::DocumentView;
                                            app.status_message = "Viewing document. [Esc] to return to list.".to_string();
                                        }
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            app.select_next_artifact();
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => {
                                            app.select_prev_artifact();
                                        }
                                        _ => {}
                                    },
                                    ArtifactPaneFocus::DocumentView => match key.code {
                                        KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                                            if app.show_artifact_tree {
                                                app.artifact_pane_focus = ArtifactPaneFocus::FileList;
                                                app.status_message = "Artifact list focused. [Enter] to view document.".to_string();
                                            }
                                        }
                                        KeyCode::Char('j') | KeyCode::Down => {
                                            app.scroll_artifact(1);
                                        }
                                        KeyCode::Char('k') | KeyCode::Up => {
                                            app.scroll_artifact(-1);
                                        }
                                        KeyCode::PageDown => {
                                            app.scroll_artifact(15);
                                        }
                                        KeyCode::PageUp => {
                                            app.scroll_artifact(-15);
                                        }
                                        _ => {}
                                    },
                                },
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. Restore terminal on exit
    app.cleanup();
    if enhanced_keyboard {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}

/// Converte eventos de teclado do Crossterm para sequência de bytes VT100/ANSI para o PTY
#[allow(dead_code)]
fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let code = (c as u8).to_ascii_lowercase();
                if code.is_ascii_lowercase() {
                    vec![code - b'a' + 1]
                } else {
                    vec![code]
                }
            } else if c == '\n' {
                vec![b'\n']
            } else if c == '\r' {
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    || key.modifiers.contains(KeyModifiers::ALT)
                {
                    vec![b'\n']
                } else {
                    vec![b'\r']
                }
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                vec![b'\n']
            } else {
                vec![b'\r']
            }
        }
        KeyCode::Backspace => vec![127], // ASCII DEL para backspace
        KeyCode::BackTab => vec![27, b'[', b'Z'], // ANSI Shift+Tab (CSI Z)
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                vec![27, b'[', b'Z'] // ANSI Shift+Tab (CSI Z)
            } else {
                vec![b'\t']
            }
        }
        KeyCode::Esc => vec![27],
        KeyCode::Up => vec![27, b'[', b'A'],
        KeyCode::Down => vec![27, b'[', b'B'],
        KeyCode::Right => vec![27, b'[', b'C'],
        KeyCode::Left => vec![27, b'[', b'D'],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_key_event_to_bytes_regular_enter_submits() {
        let key = make_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(&key), vec![b'\r']);
    }

    #[test]
    fn test_key_event_to_bytes_shift_enter_inserts_newline() {
        let key = make_key(KeyCode::Enter, KeyModifiers::SHIFT);
        assert_eq!(key_event_to_bytes(&key), vec![b'\n']);
    }

    #[test]
    fn test_key_event_to_bytes_alt_enter_inserts_newline() {
        let key = make_key(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(key_event_to_bytes(&key), vec![b'\n']);
    }

    #[test]
    fn test_key_event_to_bytes_shift_char_cr_inserts_newline() {
        let key = make_key(KeyCode::Char('\r'), KeyModifiers::SHIFT);
        assert_eq!(key_event_to_bytes(&key), vec![b'\n']);
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_j_inserts_newline() {
        let key = make_key(KeyCode::Char('j'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(&key), vec![10]); // ASCII 10 is '\n'
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_c_sends_interrupt() {
        let key = make_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(&key), vec![3]); // ETX (Ctrl+C)
    }

    #[test]
    fn test_key_event_to_bytes_shift_tab_sends_csi_z() {
        let key1 = make_key(KeyCode::BackTab, KeyModifiers::NONE);
        let key2 = make_key(KeyCode::Tab, KeyModifiers::SHIFT);
        let expected = vec![27, b'[', b'Z'];
        assert_eq!(key_event_to_bytes(&key1), expected);
        assert_eq!(key_event_to_bytes(&key2), expected);
    }
}

