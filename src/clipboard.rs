use std::io::Write;
use std::process::{Command, Stdio};

/// Copies text to the system clipboard using native platform tools with OSC 52 fallback.
pub fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        if copy_with("pbcopy", &[], text).is_ok() {
            return Ok(());
        }
    }

    #[cfg(target_os = "windows")]
    {
        if copy_with("clip.exe", &[], text).is_ok() {
            return Ok(());
        }
    }

    #[cfg(target_os = "linux")]
    {
        if copy_with("wl-copy", &[], text).is_ok() {
            return Ok(());
        }
        if copy_with("xclip", &["-selection", "clipboard"], text).is_ok() {
            return Ok(());
        }
        if copy_with("xsel", &["--clipboard", "--input"], text).is_ok() {
            return Ok(());
        }
    }

    // OSC 52 fallback for terminal emulators that support it
    copy_osc52(text);
    Ok(())
}

fn copy_with(cmd: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{} exited with {}", cmd, status),
        ))
    }
}

fn copy_osc52(text: &str) {
    use std::io::stdout;
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut b64 = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        b64.push(ALPHABET[(b0 >> 2) as usize] as char);
        b64.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            b64.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            b64.push('=');
        }
        if chunk.len() > 2 {
            b64.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            b64.push('=');
        }
    }
    let seq = format!("\x1b]52;c;{}\x07", b64);
    let mut out = stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osc52_does_not_panic() {
        copy_osc52("test copy");
    }
}
