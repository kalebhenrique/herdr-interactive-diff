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
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
        MouseButton, MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use tokio::sync::mpsc;

use crate::ai_engine::ClassificationResponse;
use crate::app::{ActiveTab, App, ArtifactPaneFocus, CodeLineDisplay, DiffPaneFocus};
use crate::git::GitDiff;
use crate::herdr::HerdrClient;

/// Asynchronous messages sent from background workers to UI
pub enum AsyncAction {
    #[allow(dead_code)]
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
    let mut do_open = false;
    let mut do_open_left = false;
    let mut do_open_tab = false;
    let mut set_placement: Option<String> = None;
    let mut do_focus_primary = false;
    let mut do_focus_review = false;
    let mut do_trigger_review = false;
    let mut do_trigger_validation = false;

    let mut iter = args.into_iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--open" => do_open = true,
            "--open-left" | "--open-split" => do_open_left = true,
            "--open-tab" => do_open_tab = true,
            "--focus-primary" => do_focus_primary = true,
            "--focus-review" => do_focus_review = true,
            "--trigger-review" => do_trigger_review = true,
            "--trigger-validation" => do_trigger_validation = true,
            "--placement" => {
                if let Some(val) = iter.next() {
                    set_placement = Some(val);
                } else {
                    eprintln!("\n  Usage: herdr-interactive-diff --placement <split|tab>\n");
                    return Ok(());
                }
            }
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
                println!("      --open                      Open interactive diff (uses configured placement: split or tab)");
                println!("      --open-left, --open-split   Open interactive diff in vertical split on the left in Herdr");
                println!("      --open-tab                  Open interactive diff in a new tab in Herdr");
                println!("      --placement <split|tab>     Configure default placement for <prefix>+f (split or tab)");
                println!("      --focus-primary             Focus Primary AI pane in Herdr");
                println!("      --focus-review              Focus Review AI pane in Herdr");
                println!("      --trigger-review            Submit 5-lens code review to Review AI");
                println!("      --trigger-validation        Submit anti-overengineering validation to Primary AI");
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

    if do_open {
        let cfg = config::load_config();
        match herdr::open_diff_with_placement(None, cfg.placement) {
            Ok(_) => {
                match cfg.placement {
                    config::DiffPlacement::Split => println!("✔ Opened herdr-interactive-diff on the left split in Herdr."),
                    config::DiffPlacement::Tab => println!("✔ Opened herdr-interactive-diff in a new tab in Herdr."),
                }
            }
            Err(e) => {
                eprintln!("✖ Error opening interactive diff: {}", e);
            }
        }
        return Ok(());
    }

    if do_open_left {
        match herdr::open_split_left(None) {
            Ok(_) => {
                println!("✔ Opened herdr-interactive-diff on the left split in Herdr.");
            }
            Err(e) => {
                eprintln!("✖ Error opening left split: {}", e);
            }
        }
        return Ok(());
    }

    if do_open_tab {
        match herdr::open_tab(None) {
            Ok(_) => {
                println!("✔ Opened herdr-interactive-diff in a new tab in Herdr.");
            }
            Err(e) => {
                eprintln!("✖ Error opening tab: {}", e);
            }
        }
        return Ok(());
    }

    fn resolve_target_pane(
        client: &HerdrClient,
        configured_pane_id: Option<&str>,
        agent_name: &str,
        target_dir: Option<&str>,
        fallback_index: usize,
    ) -> String {
        let agents = client.list_agents();
        let focused_ws = client.get_focused_workspace_id();

        // 1. If configured_pane_id is set and exists in Herdr:
        if let Some(pane_id) = configured_pane_id {
            let clean = pane_id.trim();
            if !clean.is_empty() {
                if let Some(ag) = agents.iter().find(|a| a.pane_id == clean) {
                    if focused_ws.is_none() || ag.workspace_id.as_deref() == focused_ws.as_deref() {
                        return clean.to_string();
                    }
                }
            }
        }

        // 2. Match agent of requested kind in the CURRENTLY FOCUSED WORKSPACE
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = agents.iter().find(|a| {
                a.agent.to_lowercase() == agent_name.to_lowercase()
                    && a.workspace_id.as_deref() == Some(ws_id.as_str())
            }) {
                return ag.pane_id.clone();
            }
        }

        // 3. If target_dir is available, prefer an agent of requested kind matching current working directory / repo
        if let Some(dir) = target_dir {
            let norm_dir = std::fs::canonicalize(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| dir.to_string());
            for ag in &agents {
                if ag.agent.to_lowercase() == agent_name.to_lowercase() {
                    if let Some(ref cwd) = ag.cwd.as_ref().or(ag.foreground_cwd.as_ref()) {
                        let norm_cwd = std::fs::canonicalize(cwd)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| cwd.to_string());
                        if norm_cwd == norm_dir || norm_dir.starts_with(&norm_cwd) || norm_cwd.starts_with(&norm_dir) {
                            return ag.pane_id.clone();
                        }
                    }
                }
            }
        }

        // 4. Any agent in current focused workspace
        if let Some(ref ws_id) = focused_ws {
            if let Some(ag) = agents.iter().find(|a| a.workspace_id.as_deref() == Some(ws_id.as_str())) {
                return ag.pane_id.clone();
            }
        }

        // 5. Configured pane ID if valid
        if let Some(pane_id) = configured_pane_id {
            let clean = pane_id.trim();
            if !clean.is_empty() && agents.iter().any(|a| a.pane_id == clean) {
                return clean.to_string();
            }
        }

        // 6. Match any agent of requested kind
        agents
            .iter()
            .find(|a| a.agent.to_lowercase() == agent_name.to_lowercase())
            .map(|a| a.pane_id.clone())
            .or_else(|| agents.get(fallback_index).map(|a| a.pane_id.clone()))
            .or_else(|| agents.first().map(|a| a.pane_id.clone()))
            .unwrap_or_else(|| agent_name.to_string())
    }

    if do_focus_primary {
        let detected = herdr::detect_herdr_working_dir();
        let cwd_str = std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string());
        let target_dir_resolved = target_dir
            .as_deref()
            .or(detected.as_deref())
            .or(cwd_str.as_deref())
            .unwrap_or(".");
        let cfg = config::load_config();
        if let Some(client) = HerdrClient::try_detect() {
            let target = resolve_target_pane(
                &client,
                cfg.primary_pane_id.as_deref(),
                cfg.primary_agent.as_str(),
                Some(target_dir_resolved),
                0,
            );
            if let Err(e) = client.focus_pane(&target) {
                if !target.contains(':') {
                    let _ = client.focus_agent(&target);
                } else {
                    eprintln!("Failed to focus pane {}: {}", target, e);
                }
            }
            println!("✔ Focused Primary AI target ({})", target);
        } else {
            eprintln!("Herdr socket not found at ~/.config/herdr/herdr.sock");
        }
        return Ok(());
    }

    if do_focus_review {
        let detected = herdr::detect_herdr_working_dir();
        let cwd_str = std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string());
        let target_dir_resolved = target_dir
            .as_deref()
            .or(detected.as_deref())
            .or(cwd_str.as_deref())
            .unwrap_or(".");
        let cfg = config::load_config();
        if let Some(client) = HerdrClient::try_detect() {
            let target = resolve_target_pane(
                &client,
                cfg.review_pane_id.as_deref(),
                cfg.review_agent.as_str(),
                Some(target_dir_resolved),
                1,
            );
            if let Err(e) = client.focus_pane(&target) {
                if !target.contains(':') {
                    let _ = client.focus_agent(&target);
                } else {
                    eprintln!("Failed to focus pane {}: {}", target, e);
                }
            }
            println!("✔ Focused Review AI target ({})", target);
        } else {
            eprintln!("Herdr socket not found at ~/.config/herdr/herdr.sock");
        }
        return Ok(());
    }

    if do_trigger_review {
        let detected = herdr::detect_herdr_working_dir();
        let cwd_str = std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string());
        let target_dir_resolved = target_dir
            .as_deref()
            .or(detected.as_deref())
            .or(cwd_str.as_deref())
            .unwrap_or(".");
        let diff = GitDiff::from_local_repo(Some(target_dir_resolved)).unwrap_or_default();
        if diff.files.is_empty() {
            println!("No modified git files found to review.");
            return Ok(());
        }
        let prompt = ai_engine::build_claude_review_prompt(&diff.raw);
        let _ = clipboard::copy_to_clipboard(&prompt);

        let cfg = config::load_config();
        if let Some(client) = HerdrClient::try_detect() {
            let target = resolve_target_pane(
                &client,
                cfg.review_pane_id.as_deref(),
                cfg.review_agent.as_str(),
                Some(target_dir_resolved),
                1,
            );
            let _ = client.submit_agent_prompt(&target, &prompt);
            let _ = client.focus_pane(&target);
            let _ = client.show_notification("5-Lens Code Review submitted to Review AI.");
            println!("✔ 5-Lens Code Review submitted to Review AI ({})", target);
        } else {
            println!("✔ 5-Lens Code Review copied to clipboard (Herdr socket not connected).");
        }
        return Ok(());
    }

    if do_trigger_validation {
        let detected = herdr::detect_herdr_working_dir();
        let cwd_str = std::env::current_dir().ok().map(|p| p.to_string_lossy().to_string());
        let target_dir_resolved = target_dir
            .as_deref()
            .or(detected.as_deref())
            .or(cwd_str.as_deref())
            .unwrap_or(".");

        let cfg = config::load_config();
        let client_opt = HerdrClient::try_detect();

        let review_target = client_opt.as_ref().map(|c| {
            resolve_target_pane(
                c,
                cfg.review_pane_id.as_deref(),
                cfg.review_agent.as_str(),
                Some(target_dir_resolved),
                1,
            )
        });

        let review_text = client_opt
            .as_ref()
            .and_then(|c| review_target.as_ref().and_then(|t| c.read_pane_text(t, 200)))
            .or_else(|| markdown_renderer::read_latest_claude_session_text(Some(target_dir_resolved)))
            .unwrap_or_default();

        let prompt = ai_engine::build_antigravity_validation_prompt(&review_text);
        let _ = clipboard::copy_to_clipboard(&prompt);

        if let Some(client) = client_opt {
            let target = resolve_target_pane(
                &client,
                cfg.primary_pane_id.as_deref(),
                cfg.primary_agent.as_str(),
                Some(target_dir_resolved),
                0,
            );
            let _ = client.submit_agent_prompt(&target, &prompt);
            let _ = client.focus_pane(&target);
            let _ = client.show_notification("Anti-overengineering validation submitted to Primary AI.");
            println!("✔ Validation prompt submitted to Primary AI ({})", target);
        } else {
            println!("✔ Validation prompt copied to clipboard (Herdr socket not connected).");
        }
        return Ok(());
    }

    if show_config {
        config::print_config();
        return Ok(());
    }

    if run_setup {
        let _ = config::prompt_first_time_setup()?;
        return Ok(());
    }

    if set_primary.is_some() || set_review.is_some() || set_placement.is_some() {
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
        if let Some(pl_str) = set_placement {
            if let Some(placement) = config::DiffPlacement::parse(&pl_str) {
                cfg.placement = placement;
            } else {
                eprintln!("\n  ✖ Unknown placement '{}'. Valid options: split (vertical split left), tab (new tab)\n", pl_str);
                return Ok(());
            }
        }
        config::save_config(&cfg)?;
        println!("\n  ✔ Herdr Interactive Diff configuration updated:");
        println!("    • Primary AI (opens on start):   {}", cfg.primary_agent.display_name());
        println!("    • Review AI (multi-lens review):  {}", cfg.review_agent.display_name());
        println!("    • Diff Placement (<prefix>+f):    {}", cfg.placement.display_name());
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

    // Expande til (~) se presente no caminho do diretório, detecta contexto do Herdr ou usa o diretório atual
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
        .or_else(herdr::detect_herdr_working_dir)
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()))
        });

    // Se um diretório de trabalho foi determinado, sincroniza o processo para que
    // subprocessos (Agy, Claude), comandos git e caminhos relativos operem na pasta correta.
    if let Some(ref dir) = target_dir {
        let _ = std::env::set_current_dir(dir);
    }

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
        // Arquitetura desacoplada: sincroniza findings da IA de Review caso já existam no pane
        app.sync_review_to_diff();

        // Background Watcher: Atualização contínua do Git Diff caso o código seja modificado
        let tx_diff = tx.clone();
        let watch_dir = target_dir.clone();
        let initial_raw = app.diff.raw.clone();
        tokio::spawn(async move {
            let mut last_raw = initial_raw;
            let mut interval = tokio::time::interval(Duration::from_millis(2000));
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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
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
    let mut last_herdr_tick = std::time::Instant::now();
    let mut last_review_sync_tick = std::time::Instant::now();
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

        // Sincroniza estado do Herdr periodicamente (a cada 1s, com deduplicação de estado)
        if last_herdr_tick.elapsed() >= Duration::from_millis(1000) {
            app.update_herdr_state();
            last_herdr_tick = std::time::Instant::now();
        }

        // Sincroniza comentários de review e artefatos de forma otimizada (a cada 2s com mtime check)
        if last_review_sync_tick.elapsed() >= Duration::from_millis(2000) {
            app.sync_claude_review_to_diff();
            app.check_artifacts_update();
            last_review_sync_tick = std::time::Instant::now();
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
                            if mouse.column <= 18 {
                                app.active_tab = ActiveTab::GitDiff;
                                app.sync_review_to_diff();
                                app.refresh_diff();
                            } else if mouse.column <= 36 && app.has_artifacts() {
                                app.refresh_artifacts();
                                if app.has_artifacts() {
                                    app.active_tab = ActiveTab::Artifacts;
                                }
                            }
                            continue;
                        }

                        // 2. Click in Main Content
                        match app.active_tab {
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
                        app.mouse_drag_end = Some((mouse.column, mouse.row));
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        if let (Some(start), Some(end)) = (app.mouse_drag_start, app.mouse_drag_end) {
                            let row_diff = (start.1 as i32 - end.1 as i32).abs();
                            let col_diff = (start.0 as i32 - end.0 as i32).abs();
                            if row_diff > 0 || col_diff > 4 {
                                match app.active_tab {
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
            } else if let Event::Paste(text) = ev {
                if app.is_asking_question {
                    app.question_input.push_str(&text);
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

                    // Global shortcut: Ctrl+R triggers 5-lens code review to Review AI
                    if is_ctrl && (key.code == KeyCode::Char('r') || key.code == KeyCode::Char('R')) {
                        app.trigger_review();
                        continue;
                    }

                    // Global shortcut: Ctrl+A opens AI Picker Modal
                    if is_ctrl && (key.code == KeyCode::Char('a') || key.code == KeyCode::Char('A')) {
                        app.open_agent_picker();
                        continue;
                    }

                    // Global shortcut: Ctrl+S copies review to Primary AI for anti-overengineering validation
                    if is_ctrl && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S')) {
                        app.trigger_validation();
                        continue;
                    }

                    // Global shortcut to Cycle Tabs: Ctrl+T or Tab
                    if (is_ctrl && (key.code == KeyCode::Char('t') || key.code == KeyCode::Char('T')))
                        || (key.code == KeyCode::Tab && !app.is_asking_question && !app.show_agent_picker && !app.show_help)
                    {
                        app.active_tab = match app.active_tab {
                            ActiveTab::GitDiff => {
                                app.refresh_artifacts();
                                ActiveTab::Artifacts
                            }
                            ActiveTab::Artifacts => {
                                app.sync_review_to_diff();
                                app.refresh_diff();
                                ActiveTab::GitDiff
                            }
                        };
                        continue;
                    }

                    // Global Tab Shortcuts: F1 / Ctrl+1 (Git Diff), F2 / Ctrl+2 (Artifacts)
                    if key.code == KeyCode::F(1) || (is_ctrl && key.code == KeyCode::Char('1')) {
                        app.sync_review_to_diff();
                        app.refresh_diff();
                        app.active_tab = ActiveTab::GitDiff;
                        continue;
                    }
                    if key.code == KeyCode::F(2) || (is_ctrl && key.code == KeyCode::Char('2')) {
                        app.refresh_artifacts();
                        app.active_tab = ActiveTab::Artifacts;
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

                        // TAB 1: GIT DIFF (File list + Diff/FullFile viewer + Floating tooltip)
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
                                    app.sync_review_to_diff();
                                    app.refresh_diff();
                                    app.active_tab = ActiveTab::GitDiff;
                                }
                                KeyCode::Char('2') => {
                                    app.refresh_artifacts();
                                    app.active_tab = ActiveTab::Artifacts;
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

                        // TAB 2: ARTIFACTS (Workspace & AI Markdown Artifacts Viewer)
                        ActiveTab::Artifacts => {
                            if is_ctrl && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')) {
                                if !app.artifacts.is_empty() && app.selected_artifact_idx < app.artifacts.len() {
                                    let art = &app.artifacts[app.selected_artifact_idx];
                                    let _ = crate::clipboard::copy_to_clipboard(&art.raw_content);
                                    app.status_message = format!("Copied '{}' to system clipboard (Cmd+C/pbcopy).", art.file_name);
                                }
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') => {
                                    app.should_quit = true;
                                }
                                KeyCode::Char('1') => {
                                    app.sync_review_to_diff();
                                    app.refresh_diff();
                                    app.active_tab = ActiveTab::GitDiff;
                                }
                                KeyCode::Char('2') => {
                                    app.refresh_artifacts();
                                    app.active_tab = ActiveTab::Artifacts;
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
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)?;
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

