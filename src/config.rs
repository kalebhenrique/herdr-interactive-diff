use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Supported AI agents in Herdr and herdr-interactive-diff (all 17 native Herdr agents)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Agy,
    Claude,
    Pi,
    Amp,
    Codex,
    Copilot,
    Devin,
    Droid,
    Kimi,
    OpenCode,
    Kilo,
    Hermes,
    QoderCli,
    Qwen,
    Cursor,
    MastraCode,
    Grok,
}

#[allow(dead_code)]
pub const ALL_AGENTS: &[AgentKind] = &[
    AgentKind::Agy,
    AgentKind::Claude,
    AgentKind::Pi,
    AgentKind::Amp,
    AgentKind::Codex,
    AgentKind::Copilot,
    AgentKind::Devin,
    AgentKind::Droid,
    AgentKind::Kimi,
    AgentKind::OpenCode,
    AgentKind::Kilo,
    AgentKind::Hermes,
    AgentKind::QoderCli,
    AgentKind::Qwen,
    AgentKind::Cursor,
    AgentKind::MastraCode,
    AgentKind::Grok,
];

impl AgentKind {
    /// Parses string argument into AgentKind
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "agy" | "antigravity" | "antigravity-cli" => Some(AgentKind::Agy),
            "claude" | "claude-code" | "claudecode" => Some(AgentKind::Claude),
            "pi" => Some(AgentKind::Pi),
            "amp" | "amp-local" => Some(AgentKind::Amp),
            "codex" => Some(AgentKind::Codex),
            "copilot" | "github-copilot" | "ghcs" => Some(AgentKind::Copilot),
            "devin" | "devin-cli" => Some(AgentKind::Devin),
            "droid" => Some(AgentKind::Droid),
            "kimi" | "kimi-code" => Some(AgentKind::Kimi),
            "opencode" | "open-code" => Some(AgentKind::OpenCode),
            "kilo" | "kilo-code" => Some(AgentKind::Kilo),
            "hermes" | "hermes-agent" => Some(AgentKind::Hermes),
            "qodercli" | "qoder" | "qodercn" => Some(AgentKind::QoderCli),
            "qwen" | "qwen-code" => Some(AgentKind::Qwen),
            "cursor" | "cursor-agent" => Some(AgentKind::Cursor),
            "mastracode" | "mastra" => Some(AgentKind::MastraCode),
            "grok" | "grok-build" => Some(AgentKind::Grok),
            _ => None,
        }
    }

    /// Short CLI identifier
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::Agy => "agy",
            AgentKind::Claude => "claude",
            AgentKind::Pi => "pi",
            AgentKind::Amp => "amp",
            AgentKind::Codex => "codex",
            AgentKind::Copilot => "copilot",
            AgentKind::Devin => "devin",
            AgentKind::Droid => "droid",
            AgentKind::Kimi => "kimi",
            AgentKind::OpenCode => "opencode",
            AgentKind::Kilo => "kilo",
            AgentKind::Hermes => "hermes",
            AgentKind::QoderCli => "qodercli",
            AgentKind::Qwen => "qwen",
            AgentKind::Cursor => "cursor",
            AgentKind::MastraCode => "mastracode",
            AgentKind::Grok => "grok",
        }
    }

    /// Binary command line executable name to spawn
    #[allow(dead_code)]
    pub fn command_bin(&self) -> &'static str {
        match self {
            AgentKind::Agy => "agy",
            AgentKind::Claude => "claude",
            AgentKind::Pi => "pi",
            AgentKind::Amp => "amp",
            AgentKind::Codex => "codex",
            AgentKind::Copilot => "copilot",
            AgentKind::Devin => "devin",
            AgentKind::Droid => "droid",
            AgentKind::Kimi => "kimi",
            AgentKind::OpenCode => "opencode",
            AgentKind::Kilo => "kilo",
            AgentKind::Hermes => "hermes",
            AgentKind::QoderCli => "qodercli",
            AgentKind::Qwen => "qwen",
            AgentKind::Cursor => "cursor",
            AgentKind::MastraCode => "mastracode",
            AgentKind::Grok => "grok",
        }
    }

    /// Official Herdr integration authority identifier
    #[allow(dead_code)]
    pub fn herdr_source(&self) -> &'static str {
        match self {
            AgentKind::Agy => "herdr:antigravity_cli",
            AgentKind::Claude => "herdr:claude",
            AgentKind::Pi => "herdr:pi",
            AgentKind::Amp => "herdr:amp",
            AgentKind::Codex => "herdr:codex",
            AgentKind::Copilot => "herdr:copilot",
            AgentKind::Devin => "herdr:devin",
            AgentKind::Droid => "herdr:droid",
            AgentKind::Kimi => "herdr:kimi",
            AgentKind::OpenCode => "herdr:opencode",
            AgentKind::Kilo => "herdr:kilo",
            AgentKind::Hermes => "herdr:hermes",
            AgentKind::QoderCli => "herdr:qodercli",
            AgentKind::Qwen => "herdr:qwen",
            AgentKind::Cursor => "herdr:cursor",
            AgentKind::MastraCode => "herdr:mastracode",
            AgentKind::Grok => "herdr:grok",
        }
    }

    /// User-friendly display title
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentKind::Agy => "Antigravity CLI (agy)",
            AgentKind::Claude => "Claude Code (claude)",
            AgentKind::Pi => "Pi Agent (pi)",
            AgentKind::Amp => "Amp Code (amp)",
            AgentKind::Codex => "Codex CLI (codex)",
            AgentKind::Copilot => "GitHub Copilot (copilot)",
            AgentKind::Devin => "Devin CLI (devin)",
            AgentKind::Droid => "Droid Agent (droid)",
            AgentKind::Kimi => "Kimi Code (kimi)",
            AgentKind::OpenCode => "OpenCode (opencode)",
            AgentKind::Kilo => "Kilo Code (kilo)",
            AgentKind::Hermes => "Hermes Agent (hermes)",
            AgentKind::QoderCli => "Qoder CLI (qodercli)",
            AgentKind::Qwen => "Qwen Code (qwen)",
            AgentKind::Cursor => "Cursor Agent (cursor)",
            AgentKind::MastraCode => "MastraCode (mastracode)",
            AgentKind::Grok => "Grok Build (grok)",
        }
    }
}

impl std::fmt::Display for AgentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Placement mode when opening Interactive Diff in Herdr
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffPlacement {
    Split,
    Tab,
}

impl DiffPlacement {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "split" | "split-left" | "split_left" | "vertical" | "left" | "1" => Some(DiffPlacement::Split),
            "tab" | "new-tab" | "new_tab" | "newtab" | "2" => Some(DiffPlacement::Tab),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DiffPlacement::Split => "split",
            DiffPlacement::Tab => "tab",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            DiffPlacement::Split => "Vertical Split (Left)",
            DiffPlacement::Tab => "New Tab",
        }
    }
}

impl Default for DiffPlacement {
    fn default() -> Self {
        DiffPlacement::Split
    }
}

impl std::fmt::Display for DiffPlacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Persistent configuration for Herdr Interactive Diff AI agents
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConfig {
    pub primary_agent: AgentKind,
    pub review_agent: AgentKind,
    #[serde(default)]
    pub primary_pane_id: Option<String>,
    #[serde(default)]
    pub review_pane_id: Option<String>,
    #[serde(default)]
    pub custom_theme: Option<String>,
    #[serde(default)]
    pub placement: DiffPlacement,
}

pub type WeaversConfig = PluginConfig;

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            primary_agent: AgentKind::Agy,
            review_agent: AgentKind::Claude,
            primary_pane_id: None,
            review_pane_id: None,
            custom_theme: None,
            placement: DiffPlacement::Split,
        }
    }
}

/// Returns the configuration file path
pub fn get_config_path() -> PathBuf {
    if let Ok(custom) = std::env::var("HERDR_INTERACTIVE_DIFF_CONFIG_PATH") {
        return PathBuf::from(custom);
    }
    if let Ok(custom) = std::env::var("WEAVERS_CONFIG_PATH") {
        return PathBuf::from(custom);
    }
    if let Ok(config_dir) = std::env::var("HERDR_PLUGIN_CONFIG_DIR") {
        return PathBuf::from(config_dir).join("config.json");
    }
    if let Ok(home) = std::env::var("HOME") {
        // Herdr standard plugin config directory
        let herdr_plugin_cfg = PathBuf::from(&home)
            .join(".config/herdr/plugins/config/herdr-interactive-diff/config.json");
        if herdr_plugin_cfg.exists() {
            return herdr_plugin_cfg;
        }

        let p = PathBuf::from(&home).join(".config/herdr-interactive-diff/config.json");
        if p.exists() {
            return p;
        }
        let legacy = PathBuf::from(&home).join(".config/weavers/config.json");
        if legacy.exists() {
            return legacy;
        }

        let herdr_plugin_dir = PathBuf::from(&home)
            .join(".config/herdr/plugins/config/herdr-interactive-diff");
        if herdr_plugin_dir.exists() {
            return herdr_plugin_cfg;
        }
        return legacy;
    }
    PathBuf::from(".herdr-interactive-diff/config.json")
}

/// Checks if configuration file exists on disk
pub fn config_exists() -> bool {
    get_config_path().exists()
}

/// Loads configuration from disk, returning default if not found or invalid
pub fn load_config() -> PluginConfig {
    let path = get_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<PluginConfig>(&content) {
            return cfg;
        }
    }
    // Fallback: check mirror paths if primary had invalid or missing content
    if let Ok(home) = std::env::var("HOME") {
        let mirrors = [
            PathBuf::from(&home).join(".config/herdr/plugins/config/herdr-interactive-diff/config.json"),
            PathBuf::from(&home).join(".config/weavers/config.json"),
            PathBuf::from(&home).join(".config/herdr-interactive-diff/config.json"),
        ];
        for mirror in mirrors {
            if mirror != path {
                if let Ok(content) = fs::read_to_string(&mirror) {
                    if let Ok(cfg) = serde_json::from_str::<PluginConfig>(&content) {
                        return cfg;
                    }
                }
            }
        }
    }
    PluginConfig::default()
}

/// Saves configuration to disk
pub fn save_config(cfg: &PluginConfig) -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(cfg)
        .context("Failed to serialize configuration to JSON")?;
    fs::write(&path, &json)
        .with_context(|| format!("Failed to write configuration to {}", path.display()))?;

    // Mirror to standard locations so Herdr actions and CLI shell invocations stay 100% in sync
    if let Ok(home) = std::env::var("HOME") {
        let mirrors = [
            PathBuf::from(&home).join(".config/herdr/plugins/config/herdr-interactive-diff/config.json"),
            PathBuf::from(&home).join(".config/weavers/config.json"),
            PathBuf::from(&home).join(".config/herdr-interactive-diff/config.json"),
        ];
        for mirror in mirrors {
            if mirror != path {
                if let Some(parent) = mirror.parent() {
                    if parent.exists() {
                        let _ = fs::write(&mirror, &json);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Prints current configuration cleanly in terminal
pub fn print_config() {
    let cfg = load_config();
    let path = get_config_path();
    println!("\n  󰚩 HERDR INTERACTIVE DIFF CONFIGURATION");
    println!("  Path: {}", path.display());
    println!("  --------------------------------------------------");
    println!("  • Primary agent:  {}", cfg.primary_agent.display_name());
    println!("  • Review agent:   {}", cfg.review_agent.display_name());
    println!("  • Diff placement: {}", cfg.placement.display_name());
    if let Some(t) = &cfg.custom_theme {
        println!("  • Theme override: {}", t);
    }
    println!("\n  Supported agents: pi, amp, claude, codex, copilot, devin, droid,");
    println!("                   kimi, opencode, kilo, hermes, qodercli, qwen,");
    println!("                   cursor, mastracode, antigravity (agy), grok\n");
    println!("  Use 'herdr-interactive-diff -s <agent>' to set primary agent");
    println!("  Use 'herdr-interactive-diff -r <agent>' to set review agent");
    println!("  Use 'herdr-interactive-diff --placement <split|tab>' to set placement\n");
}

/// Prompts the user interactively in the terminal on first run
pub fn prompt_first_time_setup() -> Result<PluginConfig> {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        let default_cfg = PluginConfig::default();
        let _ = save_config(&default_cfg);
        return Ok(default_cfg);
    }

    println!("\n  󰚩 Welcome to Herdr Interactive Diff! Initial AI Setup");
    println!("  ===============================================================");
    println!("  Configure your preferred AI agents from the 17 Herdr supported agents.");
    println!("  (Change anytime with 'herdr-interactive-diff -s <agent>' or '-r <agent>')\n");

    let mut reader = stdin.lock();

    // 1. Primary Agent
    println!("  1. Choose Primary AI (opens on start):");
    println!("     [1] Antigravity CLI (agy) [default]");
    println!("     [2] Claude Code (claude)");
    println!("     [3] Grok Build (grok)");
    println!("     [4] GitHub Copilot (copilot)");
    println!("     [5] Cursor Agent (cursor)");
    println!("     [6] Devin CLI (devin)");
    println!("     [7] Other supported agent (enter name)");
    print!("  Enter choice [1-7 or name] (default 1): ");
    let _ = io::stdout().flush();

    let mut line1 = String::new();
    let _ = reader.read_line(&mut line1);
    let primary = match line1.trim() {
        "2" | "claude" => AgentKind::Claude,
        "3" | "grok" => AgentKind::Grok,
        "4" | "copilot" => AgentKind::Copilot,
        "5" | "cursor" => AgentKind::Cursor,
        "6" | "devin" => AgentKind::Devin,
        other => AgentKind::parse(other).unwrap_or(AgentKind::Agy),
    };

    println!("\n  2. Choose Review AI (runs technical review on diff):");
    match primary {
        AgentKind::Agy => {
            println!("     [1] Claude Code (claude) [default]");
            println!("     [2] Antigravity CLI (agy)");
            println!("     [3] Grok Build (grok)");
        }
        _ => {
            println!("     [1] Antigravity CLI (agy) [default]");
            println!("     [2] Claude Code (claude)");
            println!("     [3] Grok Build (grok)");
        }
    }
    print!("  Enter choice (default 1): ");
    let _ = io::stdout().flush();

    let mut line2 = String::new();
    let _ = reader.read_line(&mut line2);
    let review = match primary {
        AgentKind::Agy => match line2.trim() {
            "2" | "agy" => AgentKind::Agy,
            "3" | "grok" => AgentKind::Grok,
            other => AgentKind::parse(other).unwrap_or(AgentKind::Claude),
        },
        _ => match line2.trim() {
            "2" | "claude" => AgentKind::Claude,
            "3" | "grok" => AgentKind::Grok,
            other => AgentKind::parse(other).unwrap_or(AgentKind::Agy),
        },
    };

    println!("\n  3. Choose Diff Placement (where to open on <prefix>+f):");
    println!("     [1] Vertical Split on the left (split) [default]");
    println!("     [2] New Tab (tab)");
    print!("  Enter choice [1-2] (default 1): ");
    let _ = io::stdout().flush();

    let mut line3 = String::new();
    let _ = reader.read_line(&mut line3);
    let placement = DiffPlacement::parse(line3.trim()).unwrap_or(DiffPlacement::Split);

    let config = PluginConfig {
        primary_agent: primary,
        review_agent: review,
        primary_pane_id: None,
        review_pane_id: None,
        custom_theme: None,
        placement,
    };

    save_config(&config)?;
    println!("\n  ✔ Configuration saved to {}", get_config_path().display());
    println!("    • Primary AI: {}", config.primary_agent.display_name());
    println!("    • Review AI:  {}", config.review_agent.display_name());
    println!("    • Placement:  {}", config.placement.display_name());
    println!("  Launching...\n");
    std::thread::sleep(std::time::Duration::from_millis(400));

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_placement_serialization_and_parsing() {
        assert_eq!(DiffPlacement::parse("split"), Some(DiffPlacement::Split));
        assert_eq!(DiffPlacement::parse("split-left"), Some(DiffPlacement::Split));
        assert_eq!(DiffPlacement::parse("vertical"), Some(DiffPlacement::Split));
        assert_eq!(DiffPlacement::parse("1"), Some(DiffPlacement::Split));
        assert_eq!(DiffPlacement::parse("tab"), Some(DiffPlacement::Tab));
        assert_eq!(DiffPlacement::parse("new-tab"), Some(DiffPlacement::Tab));
        assert_eq!(DiffPlacement::parse("2"), Some(DiffPlacement::Tab));
        assert_eq!(DiffPlacement::parse("unknown"), None);

        let config = PluginConfig {
            primary_agent: AgentKind::Agy,
            review_agent: AgentKind::Claude,
            primary_pane_id: None,
            review_pane_id: None,
            custom_theme: None,
            placement: DiffPlacement::Tab,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"placement\":\"tab\""));

        let deserialized: PluginConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.placement, DiffPlacement::Tab);
    }

    #[test]
    fn test_all_17_agents_parse() {
        let expected = [
            ("pi", AgentKind::Pi),
            ("amp", AgentKind::Amp),
            ("claude", AgentKind::Claude),
            ("codex", AgentKind::Codex),
            ("copilot", AgentKind::Copilot),
            ("devin", AgentKind::Devin),
            ("droid", AgentKind::Droid),
            ("kimi", AgentKind::Kimi),
            ("opencode", AgentKind::OpenCode),
            ("kilo", AgentKind::Kilo),
            ("hermes", AgentKind::Hermes),
            ("qodercli", AgentKind::QoderCli),
            ("qwen", AgentKind::Qwen),
            ("cursor", AgentKind::Cursor),
            ("mastracode", AgentKind::MastraCode),
            ("antigravity", AgentKind::Agy),
            ("grok", AgentKind::Grok),
        ];

        for (name, expected_kind) in expected {
            assert_eq!(AgentKind::parse(name), Some(expected_kind), "Failed for {}", name);
        }
    }

    #[test]
    fn test_plugin_config_serialization_preserves_pane_ids() {
        let config = PluginConfig {
            primary_agent: AgentKind::Agy,
            review_agent: AgentKind::Claude,
            primary_pane_id: Some("w1W:pA".to_string()),
            review_pane_id: Some("w1W:pB".to_string()),
            custom_theme: Some("kanagawa".to_string()),
            placement: DiffPlacement::Split,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"primary_pane_id\":\"w1W:pA\""));
        assert!(json.contains("\"review_pane_id\":\"w1W:pB\""));

        let deserialized: PluginConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.primary_pane_id.as_deref(), Some("w1W:pA"));
        assert_eq!(deserialized.review_pane_id.as_deref(), Some("w1W:pB"));
        assert_eq!(deserialized.primary_agent, AgentKind::Agy);
        assert_eq!(deserialized.review_agent, AgentKind::Claude);
        assert_eq!(deserialized.placement, DiffPlacement::Split);
    }
}
