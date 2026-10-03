use std::fs;
use std::path::{Path, PathBuf};
use anyhow::Result;

pub const ZSH_COMPLETION: &str = r#"#compdef herdr-interactive-diff weavers

_herdr_interactive_diff() {
    local curcontext="$curcontext" state line
    typeset -A opt_args

    _arguments -C \
        '(-s --start)'{-s,--start}'[Set primary agent to open on start]:agent:(agy claude)' \
        '(-r --review)'{-r,--review}'[Set review agent for multi-lens review]:agent:(agy claude)' \
        '(-c --config)'{-c,--config}'[Display current AI agent configuration]' \
        '--setup[Rerun interactive first-time AI setup wizard]' \
        '--demo[Start in demo mode with mock data]' \
        '(-p --path)'{-p,--path}'[Specify working repository directory]:directory:_files -/' \
        '(-h --help)'{-h,--help}'[Display this help menu]' \
        '--completions[Generate shell completion script (zsh, bash, fish)]:shell:(zsh bash fish)' \
        '--install-completions[Install shell completions for zsh, bash, fish]' \
        '*:directory:_files -/' && return 0
}

_herdr_interactive_diff "$@"
"#;

pub const BASH_COMPLETION: &str = r#"_herdr_diff_completions() {
    local cur prev words cword
    if declare -f _init_completion >/dev/null 2>&1; then
        _init_completion || return
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
        prev="${COMP_WORDS[COMP_CWORD-1]}"
    fi

    case "$prev" in
        -s|--start|-r|--review)
            COMPREPLY=( $(compgen -W "agy claude" -- "$cur") )
            return 0
            ;;
        -p|--path)
            COMPREPLY=( $(compgen -d -- "$cur") )
            return 0
            ;;
        --completions)
            COMPREPLY=( $(compgen -W "zsh bash fish" -- "$cur") )
            return 0
            ;;
    esac

    if [[ "$cur" == -* ]]; then
        COMPREPLY=( $(compgen -W "-s --start -r --review -c --config --setup --demo -p --path -h --help --install-completions --completions" -- "$cur") )
    else
        COMPREPLY=( $(compgen -d -- "$cur") )
    fi
}
complete -F _herdr_diff_completions herdr-interactive-diff weavers
"#;

pub const FISH_COMPLETION: &str = r#"complete -c herdr-interactive-diff -s s -l start -x -a "agy claude" -d "Set primary agent to open on start"
complete -c herdr-interactive-diff -s r -l review -x -a "agy claude" -d "Set review agent for multi-lens review"
complete -c herdr-interactive-diff -s c -l config -d "Display current AI agent configuration"
complete -c herdr-interactive-diff -l setup -d "Rerun interactive first-time AI setup wizard"
complete -c herdr-interactive-diff -l demo -d "Start in demo mode with mock data"
complete -c herdr-interactive-diff -s p -l path -r -a "(__fish_complete_directories)" -d "Specify working repository directory"
complete -c herdr-interactive-diff -s h -l help -d "Display this help menu"
complete -c herdr-interactive-diff -l completions -x -a "zsh bash fish" -d "Generate completion script"
complete -c herdr-interactive-diff -l install-completions -d "Install shell completions"
complete -c weavers -w herdr-interactive-diff
"#;

/// Automatically installs zsh completions if a standard homebrew or system path is writable
pub fn try_auto_install_zsh() {
    let site_functions = PathBuf::from("/opt/homebrew/share/zsh/site-functions");
    if site_functions.is_dir() {
        let target = site_functions.join("_herdr-interactive-diff");
        if !target.exists() {
            let _ = fs::write(&target, ZSH_COMPLETION);
            let _ = fs::write(site_functions.join("_weavers"), ZSH_COMPLETION);
            clean_zcompdump();
        }
    }
}

/// Explicitly installs completions into the best available shell directories
pub fn install_completions() -> Result<()> {
    println!("\n  󰚩 HERDR INTERACTIVE DIFF • Shell Completion Installer\n");
    let mut installed_any = false;

    // 1. Zsh
    let zsh_candidates = [
        PathBuf::from("/opt/homebrew/share/zsh/site-functions"),
        PathBuf::from("/usr/local/share/zsh/site-functions"),
        dirs_home().map(|h| h.join(".zfunc")).unwrap_or_default(),
    ];

    let mut zsh_installed_path: Option<PathBuf> = None;
    for dir in &zsh_candidates {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if !dir.exists() {
            let _ = fs::create_dir_all(dir);
        }
        if is_dir_writable(dir) {
            let target = dir.join("_herdr-interactive-diff");
            if fs::write(&target, ZSH_COMPLETION).is_ok() {
                let _ = fs::write(dir.join("_weavers"), ZSH_COMPLETION);
                zsh_installed_path = Some(target);
                installed_any = true;
                break;
            }
        }
    }

    if let Some(path) = zsh_installed_path {
        println!("  ✔ Zsh completion installed to: {}", path.display());
        clean_zcompdump();
        if path.starts_with(dirs_home().unwrap_or_default().join(".zfunc")) {
            println!("    Note: Ensure `fpath=(~/.zfunc $fpath)` is in your ~/.zshrc");
        }
    } else {
        println!("  ⚠ Could not find a writable zsh site-functions directory.");
        println!("    Run `herdr-interactive-diff --completions zsh > ~/.zfunc/_herdr-interactive-diff` manually.");
    }

    // 2. Fish
    if let Some(home) = dirs_home() {
        let fish_dir = home.join(".config/fish/completions");
        if fish_dir.is_dir() || home.join(".config/fish").is_dir() {
            let _ = fs::create_dir_all(&fish_dir);
            let target = fish_dir.join("herdr-interactive-diff.fish");
            if fs::write(&target, FISH_COMPLETION).is_ok() {
                let _ = fs::write(fish_dir.join("weavers.fish"), FISH_COMPLETION);
                println!("  ✔ Fish completion installed to: {}", target.display());
                installed_any = true;
            }
        }
    }

    // 3. Bash
    let bash_candidates = [
        PathBuf::from("/opt/homebrew/etc/bash_completion.d"),
        PathBuf::from("/usr/local/etc/bash_completion.d"),
        dirs_home().map(|h| h.join(".local/share/bash-completion/completions")).unwrap_or_default(),
    ];
    for dir in &bash_candidates {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if dir.is_dir() && is_dir_writable(dir) {
            let target = dir.join("herdr-interactive-diff");
            if fs::write(&target, BASH_COMPLETION).is_ok() {
                let _ = fs::write(dir.join("weavers"), BASH_COMPLETION);
                println!("  ✔ Bash completion installed to: {}", target.display());
                installed_any = true;
                break;
            }
        }
    }

    if installed_any {
        println!("\n  🎉 Shell autocomplete successfully configured!");
        println!("  To activate immediately in your current terminal:");
        println!("      autoload -Uz compinit && compinit");
        println!("  Or simply open a new terminal window.\n");
    } else {
        println!("\n  Run `herdr-interactive-diff --completions <shell>` to view the script manually.\n");
    }

    Ok(())
}

fn is_dir_writable(dir: &Path) -> bool {
    let test_file = dir.join(".herdr_diff_write_test");
    if fs::write(&test_file, b"test").is_ok() {
        let _ = fs::remove_file(&test_file);
        true
    } else {
        false
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn clean_zcompdump() {
    if let Some(home) = dirs_home() {
        if let Ok(entries) = fs::read_dir(&home) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(".zcompdump") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_scripts_contain_agents_and_flags() {
        assert!(ZSH_COMPLETION.contains("-s,--start"));
        assert!(ZSH_COMPLETION.contains("-r,--review"));
        assert!(ZSH_COMPLETION.contains("-c,--config"));
        assert!(BASH_COMPLETION.contains("-s|--start"));
        assert!(BASH_COMPLETION.contains("-r|--review"));
        assert!(FISH_COMPLETION.contains("-s s -l start"));
        assert!(FISH_COMPLETION.contains("-s r -l review"));
    }
}
