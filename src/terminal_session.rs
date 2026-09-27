#![allow(dead_code)]

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Sessão de terminal interativo alimentada por PTY e renderizada com vt100 / tui-term
pub struct TerminalSession {
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    pub pid: Option<u32>,
    pub is_alive: Arc<AtomicBool>,
    pub title: String,
    pub rows: u16,
    pub cols: u16,
}

impl TerminalSession {
    /// Inicia um novo processo interativo dentro de um pseudo-terminal (PTY), com suporte a diretório de trabalho (CWD)
    pub fn spawn(
        cmd_name: &str,
        args: &[&str],
        title: &str,
        rows: u16,
        cols: u16,
        cwd: Option<&str>,
    ) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Falha ao abrir PTY")?;

        let resolved_cmd = resolve_binary_path(cmd_name);
        let mut cmd = CommandBuilder::new(&resolved_cmd);
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        for arg in args {
            cmd.arg(arg);
        }

        // Tenta spawnar o comando especificado
        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("Falha ao spawnar '{}' ({}) no PTY", cmd_name, resolved_cmd))?;
        let pid = child.process_id();
        drop(pair.slave); // Explicitly close parent's copy of slave so EOF can be detected when child exits

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("Falha ao clonar reader do PTY")?;

        let writer = pair
            .master
            .take_writer()
            .context("Falha ao obter writer do PTY")?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10000)));
        let parser_clone = Arc::clone(&parser);

        let is_alive = Arc::new(AtomicBool::new(true));
        let is_alive_clone = Arc::clone(&is_alive);

        // Thread em background lendo do PTY e enviando para o parser VT100
        std::thread::Builder::new()
            .name(format!("pty-reader-{}", title))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut p) = parser_clone.lock() {
                        p.process(&buf[..n]);
                    }
                }
                is_alive_clone.store(false, Ordering::SeqCst);
            })
            .context("Falha ao iniciar thread leitora do PTY")?;

        Ok(Self {
            parser,
            writer: Arc::new(Mutex::new(writer)),
            master: Arc::new(Mutex::new(pair.master)),
            child: Arc::new(Mutex::new(child)),
            pid,
            is_alive,
            title: title.to_string(),
            rows,
            cols,
        })
    }

    /// Verifica se o processo dentro do PTY continua vivo
    pub fn is_alive(&self) -> bool {
        if !self.is_alive.load(Ordering::SeqCst) {
            return false;
        }
        if let Ok(mut c) = self.child.lock() {
            if let Ok(Some(_status)) = c.try_wait() {
                self.is_alive.store(false, Ordering::SeqCst);
                return false;
            }
        }
        true
    }

    /// Retorna o Process ID (PID) do processo no PTY, se disponível
    pub fn process_id(&self) -> Option<u32> {
        self.pid
    }

    /// Desloca o histórico do terminal para cima (vendo mensagens anteriores)
    pub fn scroll_up(&self, lines: usize) -> usize {
        if let Ok(mut p) = self.parser.lock() {
            let current = p.screen().scrollback();
            p.screen_mut().set_scrollback(current + lines);
            p.screen().scrollback()
        } else {
            0
        }
    }

    /// Desloca o histórico do terminal para baixo (em direção às mensagens recentes)
    pub fn scroll_down(&self, lines: usize) -> usize {
        if let Ok(mut p) = self.parser.lock() {
            let current = p.screen().scrollback();
            let new_offset = current.saturating_sub(lines);
            p.screen_mut().set_scrollback(new_offset);
            p.screen().scrollback()
        } else {
            0
        }
    }

    /// Retorna à base do terminal em tempo real
    pub fn scroll_to_bottom(&self) {
        if let Ok(mut p) = self.parser.lock() {
            p.screen_mut().set_scrollback(0);
        }
    }

    /// Retorna o offset atual de rolagem no histórico
    pub fn scroll_offset(&self) -> usize {
        if let Ok(p) = self.parser.lock() {
            p.screen().scrollback()
        } else {
            0
        }
    }

    /// Envia bytes brutos (ex: teclas digitadas pelo usuário) para o PTY
    pub fn send_bytes(&self, bytes: &[u8]) {
        self.scroll_to_bottom();
        if !self.is_alive() {
            return;
        }
        if let Ok(mut w) = self.writer.lock() {
            if w.write_all(bytes).is_err() || w.flush().is_err() {
                self.is_alive.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Envia um comando/texto com quebra de linha para o PTY
    pub fn send_command(&self, cmd: &str) {
        self.scroll_to_bottom();
        let text = format!("{}\n", cmd);
        self.send_bytes(text.as_bytes());
    }

    /// Envia texto como colagem atômica instantânea usando Bracketed Paste Mode (\x1b[200~ ... \x1b[201~\r)
    pub fn paste_command(&self, text: &str) {
        self.scroll_to_bottom();
        if !self.is_alive() {
            return;
        }
        let safe = text.replace("\x1b[201~", "");
        let payload = format!("\x1b[200~{}\x1b[201~\r", safe);
        self.send_bytes(payload.as_bytes());
    }

    /// Envia texto como colagem atômica usando Bracketed Paste Mode (\x1b[200~ ... \x1b[201~) sem quebra de linha forçada
    pub fn paste_text(&self, text: &str) {
        self.scroll_to_bottom();
        if !self.is_alive() {
            return;
        }
        let safe = text.replace("\x1b[201~", "");
        let payload = format!("\x1b[200~{}\x1b[201~", safe);
        self.send_bytes(payload.as_bytes());
    }

    /// Redimensiona o PTY quando a janela ou split do Ratatui mudar
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == 0 || cols == 0 {
            return;
        }
        if self.rows != rows || self.cols != cols {
            self.rows = rows;
            self.cols = cols;
            if let Ok(master) = self.master.lock() {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            if let Ok(mut p) = self.parser.lock() {
                p.screen_mut().set_size(rows, cols);
            }
        }
    }

    /// Captura o texto visível da tela do PTY
    pub fn read_screen_text(&self) -> String {
        if let Ok(p) = self.parser.lock() {
            let screen = p.screen();
            let mut lines = Vec::new();
            for line in screen.rows(0, self.cols) {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    lines.push(trimmed.to_string());
                }
            }
            lines.join("\n")
        } else {
            String::new()
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.is_alive.store(false, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

/// Localiza o binário no PATH ou nos caminhos comuns de CLI locais (~/.local/bin)
fn resolve_binary_path(name: &str) -> String {
    if let Ok(home) = std::env::var("HOME") {
        let local_path = format!("{}/.local/bin/{}", home, name);
        if std::path::Path::new(&local_path).exists() {
            return local_path;
        }
    }

    // Se tiver barra no nome, já é caminho absoluto
    if name.contains('/') && std::path::Path::new(name).exists() {
        return name.to_string();
    }

    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_echo_pty() {
        let session = TerminalSession::spawn("sh", &["-c", "echo weavers_pty_test"], "EchoTest", 24, 80, None);
        assert!(session.is_ok(), "Deve spawnar processo no PTY com sucesso");

        let session = session.unwrap();
        let mut found = false;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if session.read_screen_text().contains("weavers_pty_test") {
                found = true;
                break;
            }
        }
        assert!(found, "Output do PTY deve conter o texto executado");
    }

    #[test]
    fn test_terminal_scrollback() {
        let session = TerminalSession::spawn("echo", &["test"], "ScrollTest", 10, 40, None).unwrap();
        if let Ok(mut p) = session.parser.lock() {
            for i in 0..50 {
                p.process(format!("Linha de teste {}\r\n", i).as_bytes());
            }
        }

        assert_eq!(session.scroll_offset(), 0);
        let scrolled = session.scroll_up(15);
        assert_eq!(scrolled, 15);
        assert_eq!(session.scroll_offset(), 15);

        let scrolled = session.scroll_down(5);
        assert_eq!(scrolled, 10);
        assert_eq!(session.scroll_offset(), 10);

        session.scroll_to_bottom();
        assert_eq!(session.scroll_offset(), 0);
    }
}
