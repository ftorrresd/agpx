use anyhow::{bail, Result};

/// How a provider gets Claude Code talking to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Claude Code talks to Anthropic directly. No proxy, no rewriting.
    Native,
    /// The provider serves the Anthropic Messages API itself, so we only
    /// need to repoint the base URL and supply a key.
    Direct,
    /// Needs claude-code-proxy to translate. We run a private one.
    Proxied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Deepseek,
    Openai,
    Kimi,
    Grok,
    Cursor,
}

impl Provider {
    pub fn mode(self) -> Mode {
        match self {
            Provider::Anthropic => Mode::Native,
            Provider::Deepseek => Mode::Direct,
            Provider::Openai | Provider::Kimi | Provider::Grok | Provider::Cursor => Mode::Proxied,
        }
    }

    /// The name agpx uses on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::Deepseek => "deepseek",
            Provider::Openai => "openai",
            Provider::Kimi => "kimi",
            Provider::Grok => "grok",
            Provider::Cursor => "cursor",
        }
    }

    /// The name claude-code-proxy uses. `--openai` is really Codex, since the
    /// backing account is a ChatGPT subscription rather than a platform key.
    pub fn ccp_name(self) -> Option<&'static str> {
        match self {
            Provider::Openai => Some("codex"),
            Provider::Kimi => Some("kimi"),
            Provider::Grok => Some("grok"),
            Provider::Cursor => Some("cursor"),
            _ => None,
        }
    }

    /// ccp resolves the generic `claude-*` model names Claude Code sends via
    /// CCP_ALIAS_PROVIDER, which only understands codex and kimi. For the
    /// others an explicit --model is the only way to land on a real model.
    pub fn alias_provider(self) -> Option<&'static str> {
        match self {
            Provider::Openai => Some("codex"),
            Provider::Kimi => Some("kimi"),
            _ => None,
        }
    }

    pub fn requires_model(self) -> bool {
        self.mode() == Mode::Proxied && self.alias_provider().is_none()
    }

    /// Anthropic-compatible endpoint for Direct-mode providers.
    pub fn direct_base_url(self) -> Option<&'static str> {
        match self {
            Provider::Deepseek => Some("https://api.deepseek.com/anthropic"),
            _ => None,
        }
    }

    pub fn default_model(self) -> Option<&'static str> {
        match self {
            // Deliberately not hardcoded for proxied providers: the catalog
            // moves, so we let ccp's alias resolution pick. Run `agpx models`.
            Provider::Deepseek => Some("deepseek-chat"),
            _ => None,
        }
    }

    /// Only Codex exposes a reasoning-effort knob through ccp.
    pub fn supports_effort(self) -> bool {
        matches!(self, Provider::Openai)
    }

    pub fn from_name(s: &str) -> Result<Self> {
        Ok(match s {
            "anthropic" => Provider::Anthropic,
            "deepseek" => Provider::Deepseek,
            "openai" | "codex" => Provider::Openai,
            "kimi" => Provider::Kimi,
            "grok" => Provider::Grok,
            "cursor" => Provider::Cursor,
            other => bail!(
                "unknown provider {other:?} (expected one of: \
                 anthropic, deepseek, openai, kimi, grok, cursor)"
            ),
        })
    }
}

/// Values ccp accepts for CCP_CODEX_EFFORT. Anything else makes the server
/// reject every request, so we validate before spawning rather than after.
pub const EFFORTS: &[&str] = &["none", "low", "medium", "high", "xhigh", "max"];

pub fn validate_effort(effort: &str) -> Result<()> {
    if !EFFORTS.contains(&effort) {
        bail!(
            "invalid effort {effort:?}, expected one of: {}",
            EFFORTS.join(", ")
        );
    }
    Ok(())
}
