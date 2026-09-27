use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOrigin {
    Context,
    Addition,
    Deletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
}

impl FileStatus {
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Added => "[A]",
            Self::Modified => "[M]",
            Self::Deleted => "[D]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub origin: LineOrigin,
    pub content: String,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub id: String, // ex: "src/auth/session.rs#0"
    pub file_path: String,
    pub header: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

impl DiffHunk {
    /// Retorna o conteúdo textual completo do hunk para passar como contexto à IA
    #[allow(dead_code)]
    pub fn to_diff_text(&self) -> String {
        let mut text = format!("{}\n", self.header);
        for line in &self.lines {
            let prefix = match line.origin {
                LineOrigin::Context => " ",
                LineOrigin::Addition => "+",
                LineOrigin::Deletion => "-",
            };
            text.push_str(&format!("{}{}\n", prefix, line.content));
        }
        text
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DiffFile {
    pub old_path: Option<String>,
    pub new_path: String,
    pub status: FileStatus,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
    pub full_content: Option<String>,
}

impl DiffFile {
    /// Nome curto do arquivo (basename)
    pub fn file_name(&self) -> &str {
        self.new_path.split('/').next_back().unwrap_or(&self.new_path)
    }

    /// Carrega o arquivo completo do disco caso disponível
    pub fn load_full_content(&mut self, repo_path: Option<&str>) {
        if self.full_content.is_some() {
            return;
        }

        let full_path = if let Some(base) = repo_path {
            format!("{}/{}", base, self.new_path)
        } else {
            self.new_path.clone()
        };

        if let Ok(content) = std::fs::read_to_string(&full_path) {
            self.full_content = Some(content);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct GitDiff {
    pub raw: String,
    pub files: Vec<DiffFile>,
}

impl GitDiff {
    /// Extrai o git diff do repositório local.
    /// Tenta em ordem: HEAD, working tree, staged (--cached), arquivos não-rastreados (untracked), branch diff e commit anterior.
    pub fn from_local_repo(repo_path: Option<&str>) -> Result<Self> {
        let mut diff_text = String::new();

        // 1. Tenta git diff HEAD (staged + unstaged de arquivos rastreados)
        let mut cmd = Command::new("git");
        if let Some(path) = repo_path {
            cmd.current_dir(path);
        }
        if let Ok(output) = cmd.args(["diff", "HEAD"]).output() {
            let out_str = String::from_utf8_lossy(&output.stdout).to_string();
            if !out_str.trim().is_empty() {
                diff_text = out_str;
            }
        }

        // 2. Se HEAD falhar ou estiver vazio, tenta git diff (working tree)
        if diff_text.trim().is_empty() {
            let mut fallback_cmd = Command::new("git");
            if let Some(path) = repo_path {
                fallback_cmd.current_dir(path);
            }
            if let Ok(fallback_out) = fallback_cmd.args(["diff"]).output() {
                let fb_text = String::from_utf8_lossy(&fallback_out.stdout).to_string();
                if !fb_text.trim().is_empty() {
                    diff_text = fb_text;
                }
            }
        }

        // 3. Verifica se há alterações em staged (--cached)
        if diff_text.trim().is_empty() {
            let mut staged_cmd = Command::new("git");
            if let Some(path) = repo_path {
                staged_cmd.current_dir(path);
            }
            if let Ok(staged_out) = staged_cmd.args(["diff", "--cached"]).output() {
                let st_text = String::from_utf8_lossy(&staged_out.stdout).to_string();
                if !st_text.trim().is_empty() {
                    diff_text = st_text;
                }
            }
        }

        // 4. Captura arquivos novos / não rastreados (untracked files)
        let mut untracked_cmd = Command::new("git");
        if let Some(path) = repo_path {
            untracked_cmd.current_dir(path);
        }
        if let Ok(out) = untracked_cmd.args(["ls-files", "--others", "--exclude-standard"]).output() {
            let list = String::from_utf8_lossy(&out.stdout);
            for line in list.lines() {
                let file = line.trim();
                if file.is_empty() {
                    continue;
                }
                let mut diff_no_index = Command::new("git");
                if let Some(path) = repo_path {
                    diff_no_index.current_dir(path);
                }
                if let Ok(d_out) = diff_no_index.args(["diff", "--no-index", "/dev/null", file]).output() {
                    let d_str = String::from_utf8_lossy(&d_out.stdout);
                    if !d_str.trim().is_empty() {
                        if !diff_text.is_empty() && !diff_text.ends_with('\n') {
                            diff_text.push('\n');
                        }
                        diff_text.push_str(&d_str);
                    }
                }
            }
        }

        let mut diff = Self::parse(&diff_text)?;
        for file in &mut diff.files {
            file.load_full_content(repo_path);
        }

        Ok(diff)
    }

    /// Faz o parse de unified diff em estruturas de dados ricas
    pub fn parse(raw_diff: &str) -> Result<Self> {
        let mut files = Vec::new();
        let mut current_file: Option<DiffFile> = None;
        let mut current_hunk: Option<DiffHunk> = None;

        let mut old_lineno = 0usize;
        let mut new_lineno = 0usize;
        let mut hunk_counter = 0usize;

        for line in raw_diff.lines() {
            if line.starts_with("diff --git ") {
                // Salva hunk anterior e arquivo anterior
                if let Some(hunk) = current_hunk.take() {
                    if let Some(file) = current_file.as_mut() {
                        file.hunks.push(hunk);
                    }
                }
                if let Some(file) = current_file.take() {
                    files.push(file);
                }

                // Extrai nome do arquivo: "diff --git a/caminho b/caminho"
                let parts: Vec<&str> = line.split_whitespace().collect();
                let new_path = if parts.len() >= 4 {
                    parts[3].strip_prefix("b/").unwrap_or(parts[3]).to_string()
                } else {
                    "unknown_file".to_string()
                };
                let old_path = if parts.len() >= 3 {
                    Some(parts[2].strip_prefix("a/").unwrap_or(parts[2]).to_string())
                } else {
                    None
                };

                let status = if old_path.as_deref() == Some("/dev/null") {
                    FileStatus::Added
                } else if new_path == "/dev/null" {
                    FileStatus::Deleted
                } else {
                    FileStatus::Modified
                };

                current_file = Some(DiffFile {
                    old_path,
                    new_path,
                    status,
                    additions: 0,
                    deletions: 0,
                    hunks: Vec::new(),
                    full_content: None,
                });
                hunk_counter = 0;
            } else if line.starts_with("new file mode") {
                if let Some(file) = current_file.as_mut() {
                    file.status = FileStatus::Added;
                }
            } else if line.starts_with("deleted file mode") {
                if let Some(file) = current_file.as_mut() {
                    file.status = FileStatus::Deleted;
                }
            } else if line.starts_with("@@ ") {
                // Novo Hunk
                if let Some(hunk) = current_hunk.take() {
                    if let Some(file) = current_file.as_mut() {
                        file.hunks.push(hunk);
                    }
                }

                let file_path = current_file
                    .as_ref()
                    .map(|f| f.new_path.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
                old_lineno = old_start;
                new_lineno = new_start;

                let hunk_id = format!("{}#{}", file_path, hunk_counter);
                hunk_counter += 1;

                current_hunk = Some(DiffHunk {
                    id: hunk_id,
                    file_path,
                    header: line.to_string(),
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    lines: Vec::new(),
                });
            } else if let Some(hunk) = current_hunk.as_mut() {
                if let Some(content) = line.strip_prefix('+') {
                    hunk.lines.push(DiffLine {
                        origin: LineOrigin::Addition,
                        content: content.to_string(),
                        old_lineno: None,
                        new_lineno: Some(new_lineno),
                    });
                    new_lineno += 1;
                    if let Some(file) = current_file.as_mut() {
                        file.additions += 1;
                    }
                } else if let Some(content) = line.strip_prefix('-') {
                    hunk.lines.push(DiffLine {
                        origin: LineOrigin::Deletion,
                        content: content.to_string(),
                        old_lineno: Some(old_lineno),
                        new_lineno: None,
                    });
                    old_lineno += 1;
                    if let Some(file) = current_file.as_mut() {
                        file.deletions += 1;
                    }
                } else if let Some(content) = line.strip_prefix(' ') {
                    hunk.lines.push(DiffLine {
                        origin: LineOrigin::Context,
                        content: content.to_string(),
                        old_lineno: Some(old_lineno),
                        new_lineno: Some(new_lineno),
                    });
                    old_lineno += 1;
                    new_lineno += 1;
                } else if line.is_empty() {
                    // Linha vazia de contexto
                    hunk.lines.push(DiffLine {
                        origin: LineOrigin::Context,
                        content: String::new(),
                        old_lineno: Some(old_lineno),
                        new_lineno: Some(new_lineno),
                    });
                    old_lineno += 1;
                    new_lineno += 1;
                }
            }
        }

        // Finaliza o último hunk e arquivo
        if let Some(hunk) = current_hunk.take() {
            if let Some(file) = current_file.as_mut() {
                file.hunks.push(hunk);
            }
        }
        if let Some(file) = current_file.take() {
            files.push(file);
        }

        Ok(GitDiff {
            raw: raw_diff.to_string(),
            files,
        })
    }

    /// Cria um diff didático de demonstração contendo os 4 níveis de complexidade e arquivos completos
    pub fn demo() -> Self {
        let demo_raw = r#"diff --git a/src/auth/session.rs b/src/auth/session.rs
new file mode 100644
index e69de29..b4f2c11 100644
--- /dev/null
+++ b/src/auth/session.rs
@@ -14,6 +14,8 @@ pub fn validate_user_session(ptr: *const SessionData) -> bool {
     if ptr.is_null() {
         return false;
     }
+    // PONT NO CHECK! DEREF RAW POINTER DANGEROUS!
+    let raw_session = unsafe { &*ptr };
+    raw_session.is_active && !raw_session.is_expired()
 }
diff --git a/src/storage/cache.rs b/src/storage/cache.rs
index 31d2e14..89c5b2a 100644
--- a/src/storage/cache.rs
+++ b/src/storage/cache.rs
@@ -42,7 +42,7 @@ impl CacheManager {
     pub fn get_cached_item(&self, key: &str) -> Option<Item> {
-        let entry = self.cache.lock().unwrap();
+        let entry = self.cache.lock().expect("mutex poisoned");
-        entry.get(key).cloned()
+        entry.get(key).map(|item| item.clone())
     }
 }
diff --git a/src/compute/pipeline.rs b/src/compute/pipeline.rs
index 92a11b0..c3182ef 100644
--- a/src/compute/pipeline.rs
+++ b/src/compute/pipeline.rs
@@ -75,6 +75,9 @@ pub fn process_events(events: Vec<Event>) -> Vec<ProcessedEvent> {
     let mut results = Vec::new();
+    // Sabia que iter().filter_map() evita alocar um vetor intermediário aqui?
+    let filtered: Vec<Event> = events.into_iter().filter(|e| e.is_valid()).collect();
+    for ev in filtered {
+        results.push(ev.process());
     }
     results
 }
diff --git a/src/version.rs b/src/version.rs
index 1111111..2222222 100644
--- a/src/version.rs
+++ b/src/version.rs
@@ -1,4 +1,4 @@
-pub const APP_VERSION: &str = "0.1.0";
+pub const APP_VERSION: &str = "0.2.0";
 pub const BUILD_DATE: &str = "2026-09-22";
"#;

        let mut diff = Self::parse(demo_raw).unwrap_or_default();

        // Conteúdos completos dos arquivos para teste de "Ver Arquivo Todo"
        if let Some(f0) = diff.files.get_mut(0) {
            f0.full_content = Some(r#"// src/auth/session.rs
use crate::models::SessionData;

pub struct SessionManager {
    timeout_secs: u64,
}

impl SessionManager {
    pub fn new(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }
}

pub fn validate_user_session(ptr: *const SessionData) -> bool {
    if ptr.is_null() {
        return false;
    }
    // PONT NO CHECK! DEREF RAW POINTER DANGEROUS!
    let raw_session = unsafe { &*ptr };
    raw_session.is_active && !raw_session.is_expired()
}
"#.to_string());
        }

        if let Some(f1) = diff.files.get_mut(1) {
            f1.full_content = Some(r#"// src/storage/cache.rs
use std::sync::Mutex;
use std::collections::HashMap;

pub struct Item {
    pub id: String,
    pub payload: Vec<u8>,
}

impl Clone for Item {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            payload: self.payload.clone(),
        }
    }
}

pub struct CacheManager {
    cache: Mutex<HashMap<String, Item>>,
}

impl CacheManager {
    pub fn get_cached_item(&self, key: &str) -> Option<Item> {
        let entry = self.cache.lock().expect("mutex poisoned");
        entry.get(key).map(|item| item.clone())
    }
}
"#.to_string());
        }

        if let Some(f2) = diff.files.get_mut(2) {
            f2.full_content = Some(r#"// src/compute/pipeline.rs
pub struct Event {
    pub id: u64,
    pub active: bool,
}

impl Event {
    pub fn is_valid(&self) -> bool {
        self.active
    }
    pub fn process(self) -> ProcessedEvent {
        ProcessedEvent { id: self.id }
    }
}

pub struct ProcessedEvent {
    pub id: u64,
}

pub fn process_events(events: Vec<Event>) -> Vec<ProcessedEvent> {
    let mut results = Vec::new();
    // Sabia que iter().filter_map() evita alocar um vetor intermediário aqui?
    let filtered: Vec<Event> = events.into_iter().filter(|e| e.is_valid()).collect();
    for ev in filtered {
        results.push(ev.process());
    }
    results
}
"#.to_string());
        }

        if let Some(f3) = diff.files.get_mut(3) {
            f3.full_content = Some(r#"// src/version.rs
pub const APP_VERSION: &str = "0.2.0";
pub const BUILD_DATE: &str = "2026-09-22";
pub const SYSTEM_TAG: &str = "weavers-augmented";
"#.to_string());
        }

        diff
    }
}

/// Helper para fazer parse de @@ -old_start,old_lines +new_start,new_lines @@
fn parse_hunk_header(header: &str) -> (usize, usize, usize, usize) {
    let mut old_start = 1;
    let mut old_lines = 1;
    let mut new_start = 1;
    let mut new_lines = 1;

    let parts: Vec<&str> = header.split("@@").collect();
    if parts.len() >= 2 {
        let coords_str = parts[1].trim();
        for token in coords_str.split_whitespace() {
            if let Some(stripped) = token.strip_prefix('-') {
                let range: Vec<&str> = stripped.split(',').collect();
                if let Ok(val) = range[0].parse() {
                    old_start = val;
                }
                if range.len() > 1 {
                    if let Ok(val) = range[1].parse() {
                        old_lines = val;
                    }
                }
            } else if let Some(stripped) = token.strip_prefix('+') {
                let range: Vec<&str> = stripped.split(',').collect();
                if let Ok(val) = range[0].parse() {
                    new_start = val;
                }
                if range.len() > 1 {
                    if let Ok(val) = range[1].parse() {
                        new_lines = val;
                    }
                }
            }
        }
    }

    (old_start, old_lines, new_start, new_lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_demo_diff() {
        let diff = GitDiff::demo();
        assert_eq!(diff.files.len(), 4);
        assert_eq!(diff.files[0].new_path, "src/auth/session.rs");
        assert_eq!(diff.files[0].status, FileStatus::Added);
        assert_eq!(diff.files[0].hunks.len(), 1);
        assert_eq!(diff.files[0].hunks[0].id, "src/auth/session.rs#0");

        let hunk = &diff.files[0].hunks[0];
        assert_eq!(hunk.old_start, 14);
        assert_eq!(hunk.new_start, 14);
        assert!(hunk.lines.iter().any(|l| l.origin == LineOrigin::Addition));
        assert!(diff.files[0].full_content.is_some());
    }

    #[test]
    fn test_parse_hunk_header() {
        let (old_s, old_l, new_s, new_l) = parse_hunk_header("@@ -14,6 +14,8 @@ fn test()");
        assert_eq!(old_s, 14);
        assert_eq!(old_l, 6);
        assert_eq!(new_s, 14);
        assert_eq!(new_l, 8);
    }

    #[test]
    fn test_from_local_repo_with_untracked() {
        struct CleanupGuard<'a>(&'a std::path::Path);
        impl<'a> Drop for CleanupGuard<'a> {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(self.0);
            }
        }

        let dummy = std::path::Path::new(".dummy_untracked_test_file.tmp");
        let _ = std::fs::write(dummy, "test untracked file content");
        let _guard = CleanupGuard(dummy);
        let diff = GitDiff::from_local_repo(None).unwrap();
        assert!(!diff.files.is_empty());
        assert!(diff.files.iter().any(|f| f.status == FileStatus::Added));
    }
}
