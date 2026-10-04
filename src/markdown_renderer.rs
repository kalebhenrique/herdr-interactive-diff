use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// Item de artefato descoberto no Antigravity CLI / Gemini
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ArtifactItem {
    pub file_name: String,
    pub path: PathBuf,
    pub conversation_id: Option<String>,
    pub size_bytes: u64,
    pub modified_str: String,
    pub raw_content: String,
    pub rendered_lines: Vec<Line<'static>>,
    pub last_rendered_width: usize,
}

pub fn current_timestamp_str() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

/// Extrai um UUID canônico (8-4-4-4-12 hex) a partir de um texto bruto
fn extract_uuid_from_text(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '-');
        if clean.len() == 36 {
            let parts: Vec<&str> = clean.split('-').collect();
            if parts.len() == 5
                && parts[0].len() == 8
                && parts[1].len() == 4
                && parts[2].len() == 4
                && parts[3].len() == 4
                && parts[4].len() == 12
                && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
            {
                return Some(clean.to_lowercase());
            }
        }
    }
    None
}

/// Auxiliar que inspeciona descritores de arquivo de um processo específico via lsof ou /proc
fn detect_presence_lock_single_pid(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/sbin/lsof")
            .args(["-n", "-P", "-p", &pid.to_string()])
            .output()
            .or_else(|_| {
                std::process::Command::new("lsof")
                    .args(["-n", "-P", "-p", &pid.to_string()])
                    .output()
            })
            .ok()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Checa lock de presença em ~/.gemini/antigravity-cli/presence/<UUID>.lock
                if line.contains("/.gemini/antigravity-cli/presence/") && line.contains(".lock") {
                    if let Some(pos) = line.rfind('/') {
                        let filename = &line[pos + 1..];
                        if let Some(conv_id) = filename.split('.').next() {
                            if conv_id.len() == 36 && conv_id.contains('-') {
                                return Some(conv_id.to_string());
                            }
                        }
                    }
                }
                // Checa diretório brain ativo em ~/.gemini/antigravity-cli/brain/<UUID>/
                if line.contains("/.gemini/antigravity-cli/brain/") {
                    if let Some(pos) = line.find("/.gemini/antigravity-cli/brain/") {
                        let tail = &line[pos + "/.gemini/antigravity-cli/brain/".len()..];
                        let candidate = tail.split('/').next().unwrap_or("");
                        if candidate.len() == 36 && candidate.contains('-') {
                            return Some(candidate.to_string());
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let fd_dir = format!("/proc/{}/fd", pid);
        if let Ok(entries) = fs::read_dir(fd_dir) {
            for entry in entries.flatten() {
                if let Ok(target) = fs::read_link(entry.path()) {
                    let target_str = target.to_string_lossy();
                    if target_str.contains("/.gemini/antigravity-cli/presence/") && target_str.contains(".lock") {
                        if let Some(pos) = target_str.rfind('/') {
                            let filename = &target_str[pos + 1..];
                            if let Some(conv_id) = filename.split('.').next() {
                                if conv_id.len() == 36 && conv_id.contains('-') {
                                    return Some(conv_id.to_string());
                                }
                            }
                        }
                    }
                    if target_str.contains("/.gemini/antigravity-cli/brain/") {
                        if let Some(pos) = target_str.find("/.gemini/antigravity-cli/brain/") {
                            let tail = &target_str[pos + "/.gemini/antigravity-cli/brain/".len()..];
                            let candidate = tail.split('/').next().unwrap_or("");
                            if candidate.len() == 36 && candidate.contains('-') {
                                return Some(candidate.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detecta o UUID da conversa ativa através do lock em ~/.gemini/antigravity-cli/presence/<UUID>.lock
/// ou diretório de brain mantido pelo processo filho (child_pid) ou seus subprocessos.
/// Isso detecta tanto inicializações normais quanto conversas resumidas com /resume!
pub fn detect_presence_lock_for_pid(pid: u32) -> Option<String> {
    if let Some(id) = detect_presence_lock_single_pid(pid) {
        return Some(id);
    }

    // Fallback: se o PID for um wrapper ou processo pai, inspeciona filhos via pgrep
    #[cfg(unix)]
    {
        if let Ok(output) = std::process::Command::new("pgrep").args(["-P", &pid.to_string()]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(cpid) = line.trim().parse::<u32>() {
                        if let Some(id) = detect_presence_lock_single_pid(cpid) {
                            return Some(id);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detecta o ID da conversa ativa do Antigravity CLI para o repositório atual
pub fn detect_active_antigravity_conversation_id(
    repo_path: Option<&str>,
    screen_text: Option<&str>,
    child_pid: Option<u32>,
) -> Option<String> {
    let home = std::env::var("HOME").ok()?;

    // 1. Prioridade máxima: Identificação em tempo real do lock/brain mantido pelo processo agy
    // Quando o usuário roda /resume ou inicia um chat, este lock é atualizado imediatamente pelo agy!
    if let Some(pid) = child_pid {
        if let Some(conv_id) = detect_presence_lock_for_pid(pid) {
            return Some(conv_id);
        }
    }

    // 2. Extração explícita de UUID do terminal (ex: caso usuário tenha colado `/resume <UUID>`)
    if let Some(screen) = screen_text {
        if screen.contains("/resume") || screen.contains("Conversation ID:") || screen.contains("conversation:") {
            if let Some(uuid) = extract_uuid_from_text(screen) {
                return Some(uuid);
            }
        }
    }

    // 3. Fallback inicial pelo arquivo crash_<PID>_<CONVERSATION_ID>.log
    if let Some(pid) = child_pid {
        let crashes_dir = PathBuf::from(&home).join(".gemini/antigravity-cli/crashes");
        if crashes_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&crashes_dir) {
                let prefix = format!("crash_{}_", pid);
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with(&prefix) && fname.ends_with(".log") {
                        if let Some(conv_id) = fname.strip_prefix(&prefix).and_then(|s| s.strip_suffix(".log")) {
                            if !conv_id.is_empty() {
                                return Some(conv_id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Verificação das conversas mais recentes em ~/.gemini/antigravity-cli/brain/ vinculadas ao repo_path
    let target_dir = match repo_path {
        Some(p) => std::fs::canonicalize(p)
            .map(|pb| pb.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.to_string()),
        None => String::new(),
    };

    if !target_dir.is_empty() {
        let brain_dir = PathBuf::from(&home).join(".gemini/antigravity-cli/brain");
        if brain_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&brain_dir) {
                let mut candidates: Vec<(String, SystemTime)> = entries
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.len() == 36 && name.contains('-') && e.path().is_dir() {
                            let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
                            Some((name, mtime))
                        } else {
                            None
                        }
                    })
                    .collect();

                candidates.sort_by_key(|b| std::cmp::Reverse(b.1));
                candidates.truncate(20);

                for (conv_id, _) in &candidates {
                    let transcript_path = brain_dir
                        .join(conv_id)
                        .join(".system_generated/logs/transcript.jsonl");
                    if transcript_path.is_file() {
                        if let Ok(file) = fs::File::open(&transcript_path) {
                            use std::io::{BufRead, BufReader};
                            let reader = BufReader::new(file);
                            for line in reader.lines().take(40).flatten() {
                                if line.contains(&target_dir) {
                                    // The most recent conversation in target_dir is the current active session
                                    return Some(conv_id.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detecta a sessão ativa do Claude Code para o repositório atual em ~/.claude/projects/
pub fn detect_active_claude_session_id(repo_path: Option<&str>) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path_str = match repo_path {
        Some(p) => std::fs::canonicalize(p)
            .map(|pb| pb.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.to_string()),
        None => std::env::current_dir()
            .map(|pb| pb.to_string_lossy().to_string())
            .unwrap_or_default(),
    };
    if path_str.is_empty() {
        return None;
    }

    let slug = path_str.replace('/', "-");
    let project_dir = PathBuf::from(&home).join(".claude/projects").join(&slug);
    if !project_dir.is_dir() {
        return None;
    }

    let entries = fs::read_dir(&project_dir).ok()?;
    let mut jsonl_files: Vec<(String, SystemTime)> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .filter_map(|e| {
            let stem = e.path().file_stem()?.to_str()?.to_string();
            let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            Some((stem, mtime))
        })
        .collect();

    jsonl_files.sort_by_key(|b| std::cmp::Reverse(b.1));
    jsonl_files.first().map(|(id, _)| id.clone())
}

/// Obtém o caminho do arquivo .jsonl da sessão ativa do Claude Code para o repositório atual
pub fn get_active_claude_session_path(repo_path: Option<&str>) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let path_str = match repo_path {
        Some(p) => std::fs::canonicalize(p)
            .map(|pb| pb.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.to_string()),
        None => std::env::current_dir()
            .map(|pb| pb.to_string_lossy().to_string())
            .unwrap_or_default(),
    };
    if path_str.is_empty() {
        return None;
    }

    let slug = path_str.replace('/', "-");
    let project_dir = PathBuf::from(&home).join(".claude/projects").join(&slug);
    if !project_dir.is_dir() {
        return None;
    }

    let session_id = detect_active_claude_session_id(repo_path)?;
    let jsonl_path = project_dir.join(format!("{}.jsonl", session_id));
    if jsonl_path.is_file() {
        Some(jsonl_path)
    } else {
        None
    }
}

/// Lê a última mensagem de texto do assistente (Claude Code) a partir de um arquivo .jsonl
pub fn read_claude_transcript_file(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    use std::io::{BufRead, BufReader};
    let reader = BufReader::new(file);
    let mut last_assistant_text = None;

    for line in reader.lines().map_while(Result::ok) {
        if line.contains("\"role\":\"assistant\"") || line.contains("\"type\":\"assistant\"") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let text_parts: Vec<String> = if let Some(content) = v.pointer("/message/content").and_then(|c| c.as_array()) {
                    content
                        .iter()
                        .filter_map(|item| {
                            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect()
                } else if let Some(text) = v.pointer("/content").and_then(|c| c.as_str()) {
                    vec![text.to_string()]
                } else {
                    Vec::new()
                };

                if !text_parts.is_empty() {
                    last_assistant_text = Some(text_parts.join("\n"));
                }
            }
        }
    }

    last_assistant_text
}

/// Lê a última mensagem de texto do assistente (Claude Code) a partir do arquivo .jsonl da sessão ativa
pub fn read_latest_claude_session_text(repo_path: Option<&str>) -> Option<String> {
    let jsonl_path = get_active_claude_session_path(repo_path)?;
    read_claude_transcript_file(&jsonl_path)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClaudeTranscriptResult {
    /// File exists and was modified since last check (or first read)
    Modified(Option<String>),
    /// File exists and has NOT changed on disk
    Unchanged,
    /// No Claude session jsonl file found on disk
    NoSessionFile,
}

/// Lê a última mensagem do Claude Code apenas se o arquivo tiver sido modificado no disco desde a última checagem.
/// Evita I/O e parsing redundantes quando a sessão estiver ociosa.
pub fn read_latest_claude_session_text_if_modified(
    repo_path: Option<&str>,
    cached_mtime: &mut Option<SystemTime>,
    cached_len: &mut u64,
) -> ClaudeTranscriptResult {
    let path = match get_active_claude_session_path(repo_path) {
        Some(p) => p,
        None => return ClaudeTranscriptResult::NoSessionFile,
    };
    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return ClaudeTranscriptResult::NoSessionFile,
    };
    let mtime = meta.modified().ok();
    let len = meta.len();

    if *cached_mtime == mtime && *cached_len == len {
        return ClaudeTranscriptResult::Unchanged;
    }

    *cached_mtime = mtime;
    *cached_len = len;

    ClaudeTranscriptResult::Modified(read_claude_transcript_file(&path))
}

/// Descobre artefatos (.md) de uma conversa específica do Antigravity CLI
pub fn discover_artifacts_for_session(conv_id: &str) -> Vec<ArtifactItem> {
    let mut items = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        let active_brain_dir = PathBuf::from(home)
            .join(".gemini/antigravity-cli/brain")
            .join(conv_id);

        if active_brain_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&active_brain_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md") {
                        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        if !name.starts_with('.') && !name.ends_with(".metadata.json") {
                            if let Some(item) = load_artifact_file(&p, Some(conv_id.to_string())) {
                                items.push(item);
                            }
                        }
                    }
                }
            }
        }
    }

    // Ordena os artefatos de forma intuitiva: task.md -> implementation_plan.md -> walkthrough.md -> outros
    items.sort_by(|a, b| {
        let order = |name: &str| match name {
            "task.md" => 0,
            "implementation_plan.md" => 1,
            "walkthrough.md" => 2,
            _ => 3,
        };
        order(&a.file_name)
            .cmp(&order(&b.file_name))
            .then_with(|| a.file_name.cmp(&b.file_name))
    });

    items
}

/// Descobre artefatos (.md) estritamente vinculados ao chat ativo do Antigravity CLI.
/// Retorna vazio caso não haja chat com artefatos aberto.
pub fn discover_artifacts(
    repo_path: Option<&str>,
    screen_text: Option<&str>,
    child_pid: Option<u32>,
) -> Vec<ArtifactItem> {
    if let Some(conv_id) = detect_active_antigravity_conversation_id(repo_path, screen_text, child_pid) {
        discover_artifacts_for_session(&conv_id)
    } else {
        Vec::new()
    }
}

/// Carrega um arquivo de artefato individual do disco e renderiza seu markdown
pub fn load_artifact_file(path: &Path, conversation_id: Option<String>) -> Option<ArtifactItem> {
    let content = fs::read_to_string(path).ok()?;
    let meta = fs::metadata(path).ok()?;
    let size_bytes = meta.len();
    let _mod_time = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let modified_str = current_timestamp_str();

    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("artifact.md").to_string();
    let rendered_lines = parse_markdown_with_width(&content, 80);

    Some(ArtifactItem {
        file_name,
        path: path.to_path_buf(),
        conversation_id,
        size_bytes,
        modified_str,
        raw_content: content,
        rendered_lines,
        last_rendered_width: 80,
    })
}

/// Helper to wrap long lines preserving words and respecting max_width
pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for paragraph in text.lines() {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in paragraph.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > max_width {
                if !current_line.is_empty() {
                    result.push(current_line);
                }
                let mut rem = word;
                while rem.chars().count() > max_width {
                    let split_byte_idx = rem
                        .char_indices()
                        .nth(max_width)
                        .map(|(idx, _)| idx)
                        .unwrap_or(rem.len());
                    let (head, tail) = rem.split_at(split_byte_idx);
                    result.push(head.to_string());
                    rem = tail;
                }
                current_line = rem.to_string();
            } else if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.chars().count() + 1 + word_len <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                result.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            result.push(current_line);
        }
    }
    result
}

fn lang_icon(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "rust" | "rs" => " rust",
        "ruby" | "rb" => " ruby",
        "python" | "py" => " python",
        "javascript" | "js" => " javascript",
        "typescript" | "ts" => " typescript",
        "go" => " go",
        "c" | "cpp" => " cpp",
        "json" => " json",
        "yaml" | "yml" => " yaml",
        "toml" => " toml",
        "bash" | "sh" | "zsh" => " bash",
        "markdown" | "md" => " markdown",
        "html" => " html",
        "css" => " css",
        "sql" => "󰆼 sql",
        "mermaid" => "󰈙 mermaid",
        _ => " code",
    }
}

#[derive(Debug, Clone)]
struct SequenceParticipant {
    id: String,
    label: String,
}

#[derive(Debug, Clone)]
enum SequenceItem {
    Message {
        from: String,
        to: String,
        is_dashed: bool,
        is_async: bool,
        label: String,
        number: Option<usize>,
    },
    Note {
        target: String,
        text: String,
    },
    Divider(String),
}

/// Renderiza diagramas de sequência mermaid no terminal em estilo ASCII/Unicode box-drawing
pub fn render_sequence_diagram(diagram_lines: &[&str], width: usize) -> Vec<Line<'static>> {
    let mut participants: Vec<SequenceParticipant> = Vec::new();
    let mut items: Vec<SequenceItem> = Vec::new();
    let mut autonumber = false;
    let mut msg_counter = 1;

    for line in diagram_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") || trimmed.eq_ignore_ascii_case("sequenceDiagram") {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("autonumber") {
            autonumber = true;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("participant ").or_else(|| trimmed.strip_prefix("actor ")) {
            let rest = rest.trim();
            let (id, label) = if let Some((id_part, label_part)) = rest.split_once(" as ") {
                (id_part.trim().to_string(), label_part.trim().to_string())
            } else {
                (rest.to_string(), rest.to_string())
            };
            if !participants.iter().any(|p| p.id == id) {
                participants.push(SequenceParticipant { id, label });
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("Note ") {
            if let Some((target_part, note_text)) = rest.split_once(':') {
                let target = target_part
                    .strip_prefix("over ")
                    .or_else(|| target_part.strip_prefix("right of "))
                    .or_else(|| target_part.strip_prefix("left of "))
                    .unwrap_or(target_part)
                    .trim();
                let first_target = target.split(',').next().unwrap_or(target).trim().to_string();
                items.push(SequenceItem::Note {
                    target: first_target,
                    text: note_text.trim().to_string(),
                });
                continue;
            }
        }

        if trimmed.starts_with("loop ") || trimmed.starts_with("alt ") || trimmed.starts_with("opt ") || trimmed.starts_with("rect ") {
            items.push(SequenceItem::Divider(trimmed.to_string()));
            continue;
        }
        if trimmed == "end" || trimmed.starts_with("else") {
            items.push(SequenceItem::Divider(trimmed.to_string()));
            continue;
        }

        let arrows = [
            ("-->>", true, false),
            ("->>", false, false),
            ("-->", true, false),
            ("->", false, false),
            ("--)", true, true),
            ("-)", false, true),
        ];

        let mut matched = false;
        for &(arrow, is_dashed, is_async) in &arrows {
            if let Some(pos) = trimmed.find(arrow) {
                let from = trimmed[..pos].trim().to_string();
                let after_arrow = &trimmed[pos + arrow.len()..];
                let (to, label) = if let Some((to_part, label_part)) = after_arrow.split_once(':') {
                    (to_part.trim().to_string(), label_part.trim().to_string())
                } else {
                    (after_arrow.trim().to_string(), String::new())
                };

                if !participants.iter().any(|p| p.id == from) {
                    participants.push(SequenceParticipant { id: from.clone(), label: from.clone() });
                }
                if !participants.iter().any(|p| p.id == to) {
                    participants.push(SequenceParticipant { id: to.clone(), label: to.clone() });
                }

                let num = if autonumber {
                    let n = msg_counter;
                    msg_counter += 1;
                    Some(n)
                } else {
                    None
                };

                items.push(SequenceItem::Message {
                    from,
                    to,
                    is_dashed,
                    is_async,
                    label,
                    number: num,
                });
                matched = true;
                break;
            }
        }

        if !matched && (trimmed.starts_with("activate ") || trimmed.starts_with("deactivate ")) {
            continue;
        }
    }

    if participants.is_empty() {
        let mut lines = Vec::new();
        for l in diagram_lines {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(l.to_string(), Style::default().fg(Color::Rgb(205, 214, 244))),
            ]));
        }
        return lines;
    }

    let num_p = participants.len();
    let col_w = (width.saturating_sub(6) / num_p).max(14);
    let total_diagram_w = col_w * num_p;

    let center_cols: Vec<usize> = (0..num_p)
        .map(|idx| idx * col_w + col_w / 2)
        .collect();

    let mut lines = Vec::new();

    let dashes_right = total_diagram_w.saturating_sub(22).max(2);
    lines.push(Line::from(vec![
        Span::styled(
            format!("  ╭── 󰈙 sequenceDiagram {}╮", "─".repeat(dashes_right)),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]));

    let mut top_box_line = String::from("  ");
    for _ in participants.iter() {
        let box_len = col_w.saturating_sub(2).max(6);
        let pad = col_w.saturating_sub(box_len);
        let left_pad = pad / 2;
        let right_pad = pad - left_pad;
        top_box_line.push_str(&" ".repeat(left_pad));
        top_box_line.push_str(&format!("╭{}╮", "─".repeat(box_len.saturating_sub(2))));
        top_box_line.push_str(&" ".repeat(right_pad));
    }
    lines.push(Line::from(vec![Span::styled(top_box_line, Style::default().fg(Color::Cyan))]));

    let mut mid_box_spans = vec![Span::raw("  ")];
    for p in &participants {
        let box_len = col_w.saturating_sub(2).max(6);
        let pad = col_w.saturating_sub(box_len);
        let left_pad = pad / 2;
        let right_pad = pad - left_pad;
        mid_box_spans.push(Span::raw(" ".repeat(left_pad)));
        mid_box_spans.push(Span::styled("│", Style::default().fg(Color::Cyan)));

        let max_label_len = box_len.saturating_sub(2);
        let truncated = if p.label.chars().count() > max_label_len {
            let mut s: String = p.label.chars().take(max_label_len.saturating_sub(1)).collect();
            s.push('…');
            s
        } else {
            p.label.clone()
        };
        let label_pad = max_label_len.saturating_sub(truncated.chars().count());
        let l_pad = label_pad / 2;
        let r_pad = label_pad - l_pad;
        mid_box_spans.push(Span::raw(" ".repeat(l_pad)));
        mid_box_spans.push(Span::styled(truncated, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
        mid_box_spans.push(Span::raw(" ".repeat(r_pad)));
        mid_box_spans.push(Span::styled("│", Style::default().fg(Color::Cyan)));
        mid_box_spans.push(Span::raw(" ".repeat(right_pad)));
    }
    lines.push(Line::from(mid_box_spans));

    let mut bot_box_line = String::from("  ");
    for _ in participants.iter() {
        let box_len = col_w.saturating_sub(2).max(6);
        let pad = col_w.saturating_sub(box_len);
        let left_pad = pad / 2;
        let right_pad = pad - left_pad;
        let inner_len = box_len.saturating_sub(2);
        let mid = inner_len / 2;
        let l_dashes = "─".repeat(mid);
        let r_dashes = "─".repeat(inner_len.saturating_sub(mid + 1));
        bot_box_line.push_str(&" ".repeat(left_pad));
        bot_box_line.push_str(&format!("╰{}┬{}╯", l_dashes, r_dashes));
        bot_box_line.push_str(&" ".repeat(right_pad));
    }
    lines.push(Line::from(vec![Span::styled(bot_box_line, Style::default().fg(Color::Cyan))]));

    let make_lifeline_row = || -> String {
        let mut row = vec![' '; total_diagram_w + 4];
        row[0] = ' ';
        row[1] = ' ';
        for &col in &center_cols {
            if col + 2 < row.len() {
                row[col + 2] = '│';
            }
        }
        row.into_iter().collect()
    };

    lines.push(Line::from(vec![Span::styled(make_lifeline_row(), Style::default().fg(Color::DarkGray))]));

    for item in items {
        match item {
            SequenceItem::Message { from, to, is_dashed, is_async, label, number } => {
                let from_idx = participants.iter().position(|p| p.id == from).unwrap_or(0);
                let to_idx = participants.iter().position(|p| p.id == to).unwrap_or(0);

                let label_text = if let Some(num) = number {
                    format!("[{}] {}", num, label)
                } else {
                    label.clone()
                };

                if from_idx == to_idx {
                    let c = center_cols[from_idx];
                    let mut row1 = make_lifeline_row().chars().collect::<Vec<char>>();
                    let mut row2 = make_lifeline_row().chars().collect::<Vec<char>>();
                    let loop_width = 18.min(col_w);

                    let l_str = format!("╭── {} ──╮", label_text);
                    let l_chars: Vec<char> = l_str.chars().collect();
                    for (offset, ch) in l_chars.iter().enumerate() {
                        if c + 2 + offset < row1.len() {
                            row1[c + 2 + offset] = *ch;
                        }
                    }

                    let arrow_char = if is_async { '>' } else { '►' };
                    let ret_str = format!("╰{}─{}", "─".repeat(loop_width.saturating_sub(4)), arrow_char);
                    let ret_chars: Vec<char> = ret_str.chars().collect();
                    for (offset, ch) in ret_chars.iter().enumerate() {
                        if c + 2 + offset < row2.len() {
                            row2[c + 2 + offset] = *ch;
                        }
                    }

                    lines.push(Line::from(vec![Span::styled(row1.into_iter().collect::<String>(), Style::default().fg(Color::Yellow))]));
                    lines.push(Line::from(vec![Span::styled(row2.into_iter().collect::<String>(), Style::default().fg(Color::Yellow))]));
                    lines.push(Line::from(vec![Span::styled(make_lifeline_row(), Style::default().fg(Color::DarkGray))]));
                } else {
                    let (start_idx, end_idx, is_l_to_r) = if from_idx < to_idx {
                        (from_idx, to_idx, true)
                    } else {
                        (to_idx, from_idx, false)
                    };

                    let c_start = center_cols[start_idx] + 2;
                    let c_end = center_cols[end_idx] + 2;
                    let span_len = c_end.saturating_sub(c_start);

                    let arrow_char = if is_async { '>' } else { '►' };
                    let fill_char = if is_dashed { '┄' } else { '─' };

                    let mut row = make_lifeline_row().chars().collect::<Vec<char>>();

                    let max_lbl = span_len.saturating_sub(6);
                    let display_lbl = if label_text.chars().count() > max_lbl && max_lbl > 3 {
                        let mut s: String = label_text.chars().take(max_lbl - 1).collect();
                        s.push('…');
                        s
                    } else {
                        label_text
                    };

                    let lbl_len = display_lbl.chars().count();
                    let rem_dashes = span_len.saturating_sub(lbl_len + 4);
                    let l_dashes = rem_dashes / 2;
                    let r_dashes = rem_dashes - l_dashes;

                    let msg_segment = if is_l_to_r {
                        format!(
                            "├{} {} {}{}",
                            fill_char.to_string().repeat(l_dashes),
                            display_lbl,
                            fill_char.to_string().repeat(r_dashes),
                            arrow_char
                        )
                    } else {
                        let left_arrow = if is_async { '<' } else { '◄' };
                        format!(
                            "{}{} {} {}┤",
                            left_arrow,
                            fill_char.to_string().repeat(l_dashes),
                            display_lbl,
                            fill_char.to_string().repeat(r_dashes)
                        )
                    };

                    let seg_chars: Vec<char> = msg_segment.chars().collect();
                    for (offset, ch) in seg_chars.iter().enumerate() {
                        if c_start + offset < row.len() && (is_l_to_r || c_start + offset <= c_end) {
                            row[c_start + offset] = *ch;
                        }
                    }

                    let row_str: String = row.into_iter().collect();
                    lines.push(Line::from(vec![Span::styled(row_str, Style::default().fg(Color::Yellow))]));
                    lines.push(Line::from(vec![Span::styled(make_lifeline_row(), Style::default().fg(Color::DarkGray))]));
                }
            }
            SequenceItem::Note { target, text } => {
                let p_idx = participants.iter().position(|p| p.id == target).unwrap_or(0);
                let c = center_cols[p_idx];
                let mut row = make_lifeline_row().chars().collect::<Vec<char>>();
                let note_str = format!("┌─ 󰋖 Note: {} ─┐", text);
                for (offset, ch) in note_str.chars().enumerate() {
                    if c + 2 + offset < row.len() {
                        row[c + 2 + offset] = ch;
                    }
                }
                lines.push(Line::from(vec![Span::styled(row.into_iter().collect::<String>(), Style::default().fg(Color::LightMagenta))]));
                lines.push(Line::from(vec![Span::styled(make_lifeline_row(), Style::default().fg(Color::DarkGray))]));
            }
            SequenceItem::Divider(title) => {
                let div_str = format!("  ├── [{}] {}", title, "─".repeat(total_diagram_w.saturating_sub(title.len() + 8).max(4)));
                lines.push(Line::from(vec![Span::styled(div_str, Style::default().fg(Color::LightBlue))]));
                lines.push(Line::from(vec![Span::styled(make_lifeline_row(), Style::default().fg(Color::DarkGray))]));
            }
        }
    }

    let mut bot_line = vec![' '; total_diagram_w + 4];
    bot_line[0] = ' ';
    bot_line[1] = ' ';
    for &col in &center_cols {
        if col + 2 < bot_line.len() {
            bot_line[col + 2] = '┴';
        }
    }
    lines.push(Line::from(vec![Span::styled(bot_line.into_iter().collect::<String>(), Style::default().fg(Color::Cyan))]));

    lines.push(Line::from(vec![
        Span::styled(
            format!("  ╰{}╯", "─".repeat(total_diagram_w.max(20))),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    lines
}

/// Renderizador de Markdown de alta fidelidade inspirado em render-markdown.nvim para Ratatui
#[allow(dead_code)]
pub fn parse_markdown_to_lines(text: &str) -> Vec<Line<'static>> {
    parse_markdown_with_width(text, 80)
}

/// Renderizador de Markdown com largura responsiva adaptável a qualquer dimensão de janela
pub fn parse_markdown_with_width(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut active_callout_color: Option<Color> = None;

    let box_width = width.saturating_sub(4).max(20);

    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let raw_line = raw_lines[i];
        let trimmed = raw_line.trim_end();
        let ltrim = trimmed.trim_start();

        // 1. Fechamento de Bloco de Código se já aberto
        if in_code_block {
            if ltrim.starts_with("```") {
                in_code_block = false;
                let dashes = box_width.saturating_sub(2).max(2);
                let bottom_str = format!("  ╰{}╯", "─".repeat(dashes));
                lines.push(Line::from(vec![
                    Span::styled(bottom_str, Style::default().fg(Color::DarkGray)),
                ]));
                code_lang.clear();
                i += 1;
                continue;
            }

            // Linha interna do bloco de código
            let code_content = raw_line.strip_prefix("  ").unwrap_or(trimmed);
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(code_content.to_string(), Style::default().fg(Color::Rgb(205, 214, 244))),
            ]));
            i += 1;
            continue;
        }

        // 2. Abertura de Blocos de Código Fenced (suporta indentado como sob listas e inline ```bash cmd```)
        let bullet_stripped = ltrim
            .strip_prefix("* ")
            .or_else(|| ltrim.strip_prefix("- "))
            .unwrap_or(ltrim);

        if bullet_stripped.starts_with("```") {
            active_callout_color = None;
            let after_open = bullet_stripped.strip_prefix("```").unwrap_or("");

            // Caso A: Bloco em uma linha única (ex: ```bash yarn lint```)
            if let Some(close_pos) = after_open.find("```") {
                let inside = after_open[..close_pos].trim();
                let (lang, content) = if let Some(space_idx) = inside.find(|c: char| c.is_whitespace()) {
                    let possible_lang = &inside[..space_idx];
                    let rest = inside[space_idx..].trim();
                    match possible_lang.to_lowercase().as_str() {
                        "bash" | "sh" | "zsh" | "rust" | "rs" | "json" | "yaml" | "yml" | "toml" | "js" | "ts" | "py" | "diff" => {
                            (possible_lang, rest)
                        }
                        _ => ("bash", inside)
                    }
                } else if inside.eq_ignore_ascii_case("bash") || inside.eq_ignore_ascii_case("sh") {
                    ("bash", "")
                } else {
                    ("bash", inside)
                };

                let icon = lang_icon(lang);
                let dashes_right = box_width.saturating_sub(icon.width() + 6).max(2);
                let header_str = format!("  ╭── {} {}╮", icon, "─".repeat(dashes_right));
                lines.push(Line::from(vec![
                    Span::styled(header_str, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                ]));
                if !content.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(content.to_string(), Style::default().fg(Color::Rgb(205, 214, 244))),
                    ]));
                }
                let dashes = box_width.saturating_sub(2).max(2);
                lines.push(Line::from(vec![
                    Span::styled(format!("  ╰{}╯", "─".repeat(dashes)), Style::default().fg(Color::DarkGray)),
                ]));
                i += 1;
                continue;
            }

            // Caso B: Diagrama de Sequência Mermaid
            let lang = after_open.trim();
            if lang.eq_ignore_ascii_case("mermaid") || lang.eq_ignore_ascii_case("sequenceDiagram") {
                let mut block_lines = Vec::new();
                let mut j = i + 1;
                while j < raw_lines.len() && !raw_lines[j].trim_start().starts_with("```") {
                    block_lines.push(raw_lines[j]);
                    j += 1;
                }
                let is_sequence = lang.eq_ignore_ascii_case("sequenceDiagram")
                    || block_lines.iter().any(|l| l.trim().starts_with("sequenceDiagram"));
                if is_sequence {
                    let seq_rendered = render_sequence_diagram(&block_lines, box_width);
                    lines.extend(seq_rendered);
                    i = if j < raw_lines.len() { j + 1 } else { j };
                    continue;
                }
            }

            // Caso C: Bloco multilinhas normal
            in_code_block = true;
            code_lang = if lang.is_empty() { "code".to_string() } else { lang.to_string() };
            let icon = lang_icon(&code_lang);
            let dashes_right = box_width.saturating_sub(icon.width() + 6).max(2);
            let header_str = format!("  ╭── {} {}╮", icon, "─".repeat(dashes_right));
            lines.push(Line::from(vec![
                Span::styled(header_str, Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
            ]));
            i += 1;
            continue;
        }

        // Linha em branco
        if trimmed.trim().is_empty() {
            active_callout_color = None;
            lines.push(Line::from(""));
            i += 1;
            continue;
        }

        let ltrim = trimmed.trim_start();
        let leading_spaces = trimmed.len() - ltrim.len();

        // 2. Headings (# H1 .. ###### H6) com glifos idênticos ao render-markdown.nvim:
        // icons = { '󰲡 ', '󰲣 ', '󰲥 ', '󰲧 ', '󰲩 ', '󰲫 ' }
        if let Some(title) = ltrim.strip_prefix("# ") {
            active_callout_color = None;
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲡 ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            i += 1;
            continue;
        } else if let Some(title) = ltrim.strip_prefix("## ") {
            active_callout_color = None;
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲣 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            i += 1;
            continue;
        } else if let Some(title) = ltrim.strip_prefix("### ") {
            active_callout_color = None;
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲥 ", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
            ]));
            i += 1;
            continue;
        } else if let Some(title) = ltrim.strip_prefix("#### ") {
            active_callout_color = None;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲧 ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
            ]));
            i += 1;
            continue;
        } else if let Some(title) = ltrim.strip_prefix("##### ") {
            active_callout_color = None;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲩 ", Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)),
            ]));
            i += 1;
            continue;
        } else if let Some(title) = ltrim.strip_prefix("###### ") {
            active_callout_color = None;
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("󰲫 ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
                Span::styled(title.to_string(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
            ]));
            i += 1;
            continue;
        }

        // 3. Divisores Horizontais (---, ***, ___)
        if ltrim == "---" || ltrim == "***" || ltrim == "___" {
            active_callout_color = None;
            let dashes = box_width.max(10);
            let rule_str = format!("  {}", "─".repeat(dashes));
            lines.push(Line::from(vec![
                Span::styled(rule_str, Style::default().fg(Color::DarkGray)),
            ]));
            i += 1;
            continue;
        }

        // 4. Detecção de Tabelas Markdown (| col 1 | col 2 |) com bordas unicode box-drawing
        if ltrim.starts_with('|') {
            active_callout_color = None;
            let mut table_block = Vec::new();
            while i < raw_lines.len() && raw_lines[i].trim().starts_with('|') {
                table_block.push(raw_lines[i]);
                i += 1;
            }
            let rendered_table = render_table(&table_block, width);
            lines.extend(rendered_table);
            continue;
        }

        // 5. Callouts e Admonitions do GitHub / Obsidian (> [!NOTE], > [!TIP], etc.)
        // No render-markdown.nvim: quote.icon = '▋'
        if ltrim.starts_with('>') {
            let quote_body = ltrim.strip_prefix('>').unwrap_or("").trim_start();

            if quote_body.starts_with("[!") {
                let end_bracket = quote_body.find(']');
                if let Some(bracket_idx) = end_bracket {
                    let callout_type = &quote_body[2..bracket_idx];
                    let extra_title = quote_body[bracket_idx + 1..].trim();

                    let (rendered_icon, color) = match callout_type.to_uppercase().as_str() {
                        "NOTE" | "INFO" => ("󰋽 Note", Color::Rgb(137, 180, 250)),
                        "TIP" | "HINT" => ("󰌶 Tip", Color::Rgb(166, 227, 161)),
                        "IMPORTANT" => ("󰅾 Important", Color::Rgb(203, 166, 247)),
                        "WARNING" | "ATTENTION" => ("󰀪 Warning", Color::Rgb(249, 226, 175)),
                        "CAUTION" | "DANGER" | "ERROR" | "FAIL" | "FAILURE" => ("󰳦 Caution", Color::Rgb(243, 139, 168)),
                        "TODO" => ("󰗡 Todo", Color::Cyan),
                        "SUCCESS" | "DONE" | "CHECK" => ("󰄬 Success", Color::LightGreen),
                        "QUESTION" | "HELP" | "FAQ" => ("󰘥 Question", Color::LightYellow),
                        "BUG" => ("󰨰 Bug", Color::LightRed),
                        _ => ("󰋽 Note", Color::Cyan),
                    };

                    active_callout_color = Some(color);

                    let mut spans = vec![
                        Span::styled("  ▋ ", Style::default().fg(color)),
                        Span::styled(format!("{} ", rendered_icon), Style::default().fg(color).add_modifier(Modifier::BOLD)),
                    ];
                    if !extra_title.is_empty() {
                        spans.push(Span::styled(extra_title.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
                    }
                    lines.push(Line::from(spans));
                    i += 1;
                    continue;
                }
            }

            // Linha continuação de callout ou blockquote normal
            let bar_color = active_callout_color.unwrap_or(Color::DarkGray);
            let avail_w = box_width.saturating_sub(6).max(10);
            let chunks = wrap_text(quote_body, avail_w);
            if chunks.is_empty() {
                let spans = vec![
                    Span::styled("  ▋ ", Style::default().fg(bar_color)),
                ];
                lines.push(Line::from(spans));
            } else {
                for chunk in chunks {
                    let mut spans = vec![
                        Span::styled("  ▋ ", Style::default().fg(bar_color)),
                    ];
                    spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::Rgb(205, 214, 244))));
                    lines.push(Line::from(spans));
                }
            }
            i += 1;
            continue;
        }

        // Se saiu do bloco de citação, reseta o callout ativo
        active_callout_color = None;

        // 6. Checkboxes / Task lists (- [ ], - [x], - [-])
        // No render-markdown.nvim: unchecked = '󰄱 ', checked = '󰱒 ', todo = '󰥔 '
        if let Some(task_text) = ltrim.strip_prefix("- [ ] ").or_else(|| ltrim.strip_prefix("* [ ] ")) {
            let indent = " ".repeat(leading_spaces + 2);
            let sub_indent = " ".repeat(leading_spaces + 6);
            let avail_w = box_width.saturating_sub(leading_spaces + 6).max(10);
            let chunks = wrap_text(task_text, avail_w);
            if chunks.is_empty() {
                let spans = vec![
                    Span::raw(indent),
                    Span::styled("󰄱 ", Style::default().fg(Color::DarkGray)),
                ];
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled("󰄱 ", Style::default().fg(Color::DarkGray)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(sub_indent.clone())];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        } else if let Some(task_text) = ltrim.strip_prefix("- [x] ")
            .or_else(|| ltrim.strip_prefix("- [X] "))
            .or_else(|| ltrim.strip_prefix("* [x] "))
            .or_else(|| ltrim.strip_prefix("* [X] "))
        {
            let indent = " ".repeat(leading_spaces + 2);
            let sub_indent = " ".repeat(leading_spaces + 6);
            let avail_w = box_width.saturating_sub(leading_spaces + 6).max(10);
            let chunks = wrap_text(task_text, avail_w);
            if chunks.is_empty() {
                let spans = vec![
                    Span::raw(indent),
                    Span::styled("󰱒 ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                ];
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled("󰱒 ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::Rgb(166, 227, 161))));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(sub_indent.clone())];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::Rgb(166, 227, 161))));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        } else if let Some(task_text) = ltrim.strip_prefix("- [-] ").or_else(|| ltrim.strip_prefix("* [-] ")) {
            let indent = " ".repeat(leading_spaces + 2);
            let sub_indent = " ".repeat(leading_spaces + 6);
            let avail_w = box_width.saturating_sub(leading_spaces + 6).max(10);
            let chunks = wrap_text(task_text, avail_w);
            if chunks.is_empty() {
                let spans = vec![
                    Span::raw(indent),
                    Span::styled("󰥔 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ];
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled("󰥔 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::Rgb(249, 226, 175))));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(sub_indent.clone())];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::Rgb(249, 226, 175))));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        }

        // 7. Bullets por nível de indentação conforme render-markdown.nvim:
        // icons = { '●', '○', '◆', '◇' }
        if let Some(item_text) = ltrim.strip_prefix("- ").or_else(|| ltrim.strip_prefix("* ")) {
            let (icon, color) = match leading_spaces {
                0..=1 => ("● ", Color::Cyan),
                2..=3 => ("○ ", Color::Yellow),
                4..=5 => ("◆ ", Color::LightMagenta),
                _ => ("◇ ", Color::LightGreen),
            };
            let indent = " ".repeat(leading_spaces + 2);
            let sub_indent = " ".repeat(leading_spaces + 4);
            let avail_w = box_width.saturating_sub(leading_spaces + 4).max(10);
            let chunks = wrap_text(item_text, avail_w);
            if chunks.is_empty() {
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(icon, Style::default().fg(color)),
                ];
                spans.extend(parse_inline_formatting(item_text, Style::default().fg(Color::White)));
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled(icon, Style::default().fg(color)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(sub_indent.clone())];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        }

        // 8. Listas numeradas (1. , 2. )
        if let Some(dot_idx) = ltrim.find(". ") {
            let prefix = &ltrim[..dot_idx];
            if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
                let item_text = &ltrim[dot_idx + 2..];
                let num_prefix = format!("{}. ", prefix);
                let indent = " ".repeat(leading_spaces + 2);
                let sub_indent = " ".repeat(leading_spaces + 2 + num_prefix.len());
                let avail_w = box_width.saturating_sub(sub_indent.len()).max(10);
                let chunks = wrap_text(item_text, avail_w);
                if chunks.is_empty() {
                    let mut spans = vec![
                        Span::raw(indent),
                        Span::styled(num_prefix, Style::default().fg(Color::LightCyan)),
                    ];
                    spans.extend(parse_inline_formatting(item_text, Style::default().fg(Color::White)));
                    lines.push(Line::from(spans));
                } else {
                    for (idx, chunk) in chunks.into_iter().enumerate() {
                        if idx == 0 {
                            let mut spans = vec![
                                Span::raw(indent.clone()),
                                Span::styled(num_prefix.clone(), Style::default().fg(Color::LightCyan)),
                            ];
                            spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                            lines.push(Line::from(spans));
                        } else {
                            let mut spans = vec![Span::raw(sub_indent.clone())];
                            spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                            lines.push(Line::from(spans));
                        }
                    }
                }
                i += 1;
                continue;
            }
        }

        // 9. Parágrafo comum com formatação inline e suporte a Why: / Fix:
        let indent = " ".repeat(leading_spaces + 2);
        let avail_w = box_width.saturating_sub(indent.len()).max(10);

        if let Some(rest) = ltrim.strip_prefix("Why: ") {
            let chunks = wrap_text(rest, avail_w.saturating_sub(5).max(10));
            if chunks.is_empty() {
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled("Why: ", Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                ];
                spans.extend(parse_inline_formatting(rest, Style::default().fg(Color::White)));
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled("Why: ", Style::default().fg(Color::LightCyan).add_modifier(Modifier::BOLD)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(format!("{}     ", indent))];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        } else if let Some(rest) = ltrim.strip_prefix("Fix: ") {
            let chunks = wrap_text(rest, avail_w.saturating_sub(5).max(10));
            if chunks.is_empty() {
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled("Fix: ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                ];
                spans.extend(parse_inline_formatting(rest, Style::default().fg(Color::White)));
                lines.push(Line::from(spans));
            } else {
                for (idx, chunk) in chunks.into_iter().enumerate() {
                    if idx == 0 {
                        let mut spans = vec![
                            Span::raw(indent.clone()),
                            Span::styled("Fix: ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                        ];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    } else {
                        let mut spans = vec![Span::raw(format!("{}     ", indent))];
                        spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                        lines.push(Line::from(spans));
                    }
                }
            }
            i += 1;
            continue;
        }

        let chunks = wrap_text(ltrim, avail_w);
        if chunks.is_empty() {
            let mut spans = vec![Span::raw(indent)];
            spans.extend(parse_inline_formatting(ltrim, Style::default().fg(Color::White)));
            lines.push(Line::from(spans));
        } else {
            for chunk in chunks {
                let mut spans = vec![Span::raw(indent.clone())];
                spans.extend(parse_inline_formatting(&chunk, Style::default().fg(Color::White)));
                lines.push(Line::from(spans));
            }
        }
        i += 1;
    }

    lines
}

/// Renderizador de Tabelas Markdown no estilo render-markdown.nvim:
/// Utiliza caracteres de desenho de caixa Unicode ('┌', '┬', '┐', '├', '┼', '┤', '└', '┴', '┘', '│', '─')
fn render_table(table_lines: &[&str], width: usize) -> Vec<Line<'static>> {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for line in table_lines {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<String> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();

        let is_sep = cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' '));
        if !is_sep {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return Vec::new();
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return Vec::new();
    }

    // Normaliza número de colunas por linha
    for row in &mut rows {
        while row.len() < col_count {
            row.push(String::new());
        }
    }

    let box_width = width.saturating_sub(4).max(20);

    // Calcula largura visual necessária para cada coluna com padding
    let mut col_widths = vec![4usize; col_count];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            col_widths[i] = col_widths[i].max(cell.width() + 2); // +2 para 1 espaço de margem em cada lado
        }
    }

    // Escala colunas proporcionalmente se exceder a largura disponível
    let total_needed: usize = col_widths.iter().sum::<usize>() + col_count + 1;
    if total_needed > box_width && total_needed > 0 {
        let avail = box_width.saturating_sub(col_count + 1).max(col_count * 3);
        let sum_cols: usize = col_widths.iter().sum();
        for w in &mut col_widths {
            if let Some(res) = (*w * avail).checked_div(sum_cols) {
                *w = res.max(3);
            }
        }
    }

    let mut result = Vec::new();
    let border_style = Style::default().fg(Color::DarkGray);

    // Borda Superior: ┌─────────────┬─────────────┐
    let mut top_line = String::from("  ┌");
    for (i, w) in col_widths.iter().enumerate() {
        top_line.push_str(&"─".repeat(*w));
        if i + 1 < col_count {
            top_line.push('┬');
        } else {
            top_line.push('┐');
        }
    }
    result.push(Line::from(Span::styled(top_line, border_style)));

    for (row_idx, row) in rows.iter().enumerate() {
        let is_header = row_idx == 0;
        let mut spans = vec![Span::styled("  │", border_style)];

        for (col_idx, cell) in row.iter().enumerate() {
            let col_w = col_widths[col_idx];
            let cell_width = cell.width();
            let display_cell = if cell_width > col_w.saturating_sub(2) {
                let mut truncated = String::new();
                let max_len = col_w.saturating_sub(3);
                for c in cell.chars() {
                    if truncated.width() + c.len_utf8() <= max_len {
                        truncated.push(c);
                    } else {
                        break;
                    }
                }
                format!("{}…", truncated)
            } else {
                cell.clone()
            };

            let pad_left = " ";
            let pad_right_len = col_w.saturating_sub(display_cell.width() + 1);
            let pad_right = " ".repeat(pad_right_len);

            spans.push(Span::raw(pad_left));
            if is_header {
                spans.push(Span::styled(
                    display_cell,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.extend(parse_inline_formatting(&display_cell, Style::default().fg(Color::White)));
            }
            spans.push(Span::raw(pad_right));
            spans.push(Span::styled("│", border_style));
        }

        result.push(Line::from(spans));

        // Linha divisória após cabeçalho: ├─────────────┼─────────────┤
        if is_header && rows.len() > 1 {
            let mut mid_line = String::from("  ├");
            for (i, w) in col_widths.iter().enumerate() {
                mid_line.push_str(&"─".repeat(*w));
                if i + 1 < col_count {
                    mid_line.push('┼');
                } else {
                    mid_line.push('┤');
                }
            }
            result.push(Line::from(Span::styled(mid_line, border_style)));
        }
    }

    // Borda Inferior: └─────────────┴─────────────┘
    let mut bottom_line = String::from("  └");
    for (i, w) in col_widths.iter().enumerate() {
        bottom_line.push_str(&"─".repeat(*w));
        if i + 1 < col_count {
            bottom_line.push('┴');
        } else {
            bottom_line.push('┘');
        }
    }
    result.push(Line::from(Span::styled(bottom_line, border_style)));

    result
}

/// Parse de formatação inline: **bold**, `code`, *italic*, ~~strikethrough~~
fn parse_inline_formatting(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();

    while let Some(ch) = chars.next() {
        if ch == '`' {
            if !current.is_empty() {
                spans.push(Span::styled(current.clone(), base_style));
                current.clear();
            }
            let mut code = String::new();
            for c in chars.by_ref() {
                if c == '`' {
                    break;
                }
                code.push(c);
            }
            // Realce idêntico ao render-markdown.nvim: fundo sutil Rgb(40, 42, 54) e texto amarelo suave
            spans.push(Span::styled(
                format!(" {} ", code),
                Style::default()
                    .fg(Color::Rgb(249, 226, 175))
                    .bg(Color::Rgb(40, 42, 54))
                    .add_modifier(Modifier::BOLD),
            ));
        } else if ch == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consome segundo '*'
            if !current.is_empty() {
                spans.push(Span::styled(current.clone(), base_style));
                current.clear();
            }
            let mut bold_text = String::new();
            #[allow(clippy::while_let_on_iterator)]
            while let Some(c) = chars.next() {
                if c == '*' && chars.peek() == Some(&'*') {
                    chars.next();
                    break;
                }
                bold_text.push(c);
            }
            spans.push(Span::styled(
                bold_text,
                base_style.add_modifier(Modifier::BOLD),
            ));
        } else if ch == '~' && chars.peek() == Some(&'~') {
            chars.next(); // consome segundo '~'
            if !current.is_empty() {
                spans.push(Span::styled(current.clone(), base_style));
                current.clear();
            }
            let mut strike_text = String::new();
            #[allow(clippy::while_let_on_iterator)]
            while let Some(c) = chars.next() {
                if c == '~' && chars.peek() == Some(&'~') {
                    chars.next();
                    break;
                }
                strike_text.push(c);
            }
            spans.push(Span::styled(
                strike_text,
                base_style.add_modifier(Modifier::CROSSED_OUT),
            ));
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, base_style));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_headings_and_alerts() {
        let md = r#"# Titulo H1
## Subtitulo H2
### Secao H3
> [!NOTE]
> Isto é uma nota importante.
- [ ] Item pendente
- [x] Item concluído
- [-] Item em progresso
- Item com bullet nível 0
  - Subitem nível 1
| Recurso | Status |
| --- | --- |
| Artifacts | Concluído |
```rust
fn main() {}
```
"#;
        let lines = parse_markdown_to_lines(md);
        assert!(!lines.is_empty());

        let line_strings: Vec<String> = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect();
        let joined = line_strings.join("\n");

        // Ícones de cabeçalho do render-markdown.nvim
        assert!(joined.contains("󰲡"));
        assert!(joined.contains("󰲣"));
        assert!(joined.contains("󰲥"));

        // Callout e bloco
        assert!(joined.contains("󰋽 Note"));
        assert!(joined.contains("▋"));

        // Checkboxes do render-markdown.nvim
        assert!(joined.contains("󰄱"));
        assert!(joined.contains("󰱒"));
        assert!(joined.contains("󰥔"));

        // Bullets
        assert!(joined.contains("●"));
        assert!(joined.contains("○"));

        // Tabela com box drawing
        assert!(joined.contains('┌'));
        assert!(joined.contains('│'));
        assert!(joined.contains('└'));

        // Bloco de código
        assert!(joined.contains("rust"));
    }

    #[test]
    fn test_parse_markdown_with_narrow_width() {
        let md = r#"
---
```rust
println!("hello world");
```
| A | B |
|---|---|
| 1 | 2 |
"#;
        let narrow_lines = parse_markdown_with_width(md, 40);
        assert!(!narrow_lines.is_empty());
        for line in &narrow_lines {
            let line_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            assert!(line_len <= 45, "Line '{}' exceeded narrow width: len {}", line, line_len);
        }
    }

    #[test]
    fn test_detect_active_claude_session_id_with_nonexistent_dir() {
        let res = detect_active_claude_session_id(Some("/nonexistent/path/that/does/not/exist"));
        assert_eq!(res, None);
    }

    #[test]
    fn test_read_claude_transcript_file_parses_assistant_text() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join(format!("test_claude_transcript_{}.jsonl", std::process::id()));
        let jsonl_content = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"hello"}]}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Here is my review."}]}}
"#;
        std::fs::write(&test_file, jsonl_content).unwrap();
        let parsed = read_claude_transcript_file(&test_file);
        let _ = std::fs::remove_file(&test_file);
        assert_eq!(parsed, Some("Here is my review.".to_string()));
    }

    #[test]
    fn test_read_latest_claude_session_text_if_modified_nonexistent() {
        let mut mtime = None;
        let mut len = 0;
        let res = read_latest_claude_session_text_if_modified(Some("/nonexistent/repo"), &mut mtime, &mut len);
        assert_eq!(res, ClaudeTranscriptResult::NoSessionFile);
    }

    #[test]
    fn test_sequence_diagram_rendering() {
        let md = r#"
```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant H as Herdr
    participant A as Agent
    U->>H: Open Diff
    H->>A: Send Prompt
    A-->>H: Response
    H-->>U: Render UI
    Note over H: Sync State
```
"#;
        let lines = parse_markdown_with_width(md, 80);
        let joined: String = lines.iter().flat_map(|l| l.spans.iter().map(|s| s.content.as_ref())).collect();
        assert!(joined.contains("sequenceDiagram"));
        assert!(joined.contains("User"));
        assert!(joined.contains("Herdr"));
        assert!(joined.contains("Agent"));
        assert!(joined.contains("Open Diff"));
        assert!(joined.contains("Send Prompt"));
        assert!(joined.contains("Response"));
        assert!(joined.contains("Render UI"));
        assert!(joined.contains("Note: Sync State"));
        assert!(joined.contains('┴'));
    }

    #[test]
    fn test_indented_and_single_line_bash_code_blocks() {
        // 1. Single-line code fence
        let md_single = "```bash yarn lint```";
        let lines_single = parse_markdown_with_width(md_single, 80);
        let joined_single: String = lines_single.iter().flat_map(|l| l.spans.iter().map(|s| s.content.as_ref())).collect();
        assert!(joined_single.contains("yarn lint"));
        assert!(!joined_single.contains("```bash"));

        // 2. Indented block under bullet point
        let md_indented = r#"
* Executar o linter no repositório inteiro:
  ```bash
  cargo test
  ```
"#;
        let lines_indented = parse_markdown_with_width(md_indented, 80);
        let joined_indented: String = lines_indented.iter().flat_map(|l| l.spans.iter().map(|s| s.content.as_ref())).collect();
        assert!(joined_indented.contains("cargo test"));
        assert!(!joined_indented.contains("```bash"));
    }
}
