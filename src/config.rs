use std::fs;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiffPlacement {
    #[default]
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

#[deprecated(note = "Use PluginConfig instead")]
#[allow(dead_code)]
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
    if let Ok(config_dir) = std::env::var("HERDR_PLUGIN_CONFIG_DIR") {
        return PathBuf::from(config_dir).join("config.json");
    }
    if let Ok(home) = std::env::var("HOME") {
        // 1. Herdr standard plugin config directory (~/.config/herdr/plugins/config/herdr-interactive-diff/config.json)
        let herdr_plugin_dir = PathBuf::from(&home)
            .join(".config/herdr/plugins/config/herdr-interactive-diff");
        let herdr_plugin_cfg = herdr_plugin_dir.join("config.json");
        if herdr_plugin_cfg.exists() {
            return herdr_plugin_cfg;
        }

        // 2. Standard user config (~/.config/herdr-interactive-diff/config.json)
        let standard_cfg = PathBuf::from(&home).join(".config/herdr-interactive-diff/config.json");
        if standard_cfg.exists() {
            return standard_cfg;
        }

        // 3. If Herdr plugin directory exists, prefer saving there
        if herdr_plugin_dir.exists() {
            return herdr_plugin_cfg;
        }

        // 4. Default for standalone or new installations: ~/.config/herdr-interactive-diff/config.json
        return standard_cfg;
    }
    // Deprecated fallback environment variable
    if let Ok(custom) = std::env::var("WEAVERS_CONFIG_PATH") {
        return PathBuf::from(custom);
    }
    PathBuf::from(".herdr-interactive-diff/config.json")
}


/// Loads configuration from disk, returning default if not found or invalid
pub fn load_config() -> PluginConfig {
    let path = get_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<PluginConfig>(&content) {
            return cfg;
        }
    }
    // Fallback: check mirror paths or legacy weavers location for seamless migration
    if let Ok(home) = std::env::var("HOME") {
        let mirrors = [
            PathBuf::from(&home).join(".config/herdr/plugins/config/herdr-interactive-diff/config.json"),
            PathBuf::from(&home).join(".config/herdr-interactive-diff/config.json"),
            PathBuf::from(&home).join(".config/weavers/config.json"), // legacy migration fallback only
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
    println!("  • Diff placement:       {}", cfg.placement.display_name());
    println!("  • Preferred Primary AI: {}", cfg.primary_agent.display_name());
    println!("  • Preferred Review AI:  {}", cfg.review_agent.display_name());
    if let Some(ref pid) = cfg.primary_pane_id {
        println!("  • Bound Primary Pane:   {}", pid);
    }
    if let Some(ref pid) = cfg.review_pane_id {
        println!("  • Bound Review Pane:    {}", pid);
    }
    if let Some(t) = &cfg.custom_theme {
        println!("  • Theme override:       {}", t);
    }
    println!("\n  Note: AI agents are automatically discovered from active Herdr panes.");
    println!("  Press 'a' inside the interactive diff to assign or switch agent panes.");
    println!("\n  Use 'herdr-interactive-diff --placement <split|tab>' to set placement\n");
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
