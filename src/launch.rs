use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::creds;
use crate::provider::{Mode, Provider};
use crate::proxy;

pub struct Options {
    pub provider: Provider,
    pub model: Option<String>,
    pub small_model: Option<String>,
    pub effort: Option<String>,
    pub verbose: bool,
    /// Everything after `--`, forwarded to claude untouched.
    pub claude_args: Vec<OsString>,
}

/// Claude Code sizes its auto-compaction against a 200k Anthropic window. Codex
/// models are larger, so without this it compacts far earlier than it needs to.
const CODEX_COMPACT_WINDOW: &str = "272000";

pub fn run(opts: Options) -> Result<i32> {
    let provider = opts.provider;

    if opts.effort.is_some() && !provider.supports_effort() {
        bail!(
            "--effort is not supported for {}; only openai (Codex) exposes a reasoning-effort knob",
            provider.name()
        );
    }

    let model = opts.model.clone().or_else(|| {
        provider
            .default_model()
            .map(|m| m.to_string())
    });

    if provider.requires_model() && model.is_none() {
        bail!(
            "{p} has no generic model alias, so --model is required.\n\
             Run `agpx models` to see the catalog.",
            p = provider.name()
        );
    }

    let claude = proxy::which("claude").context(
        "claude not found on PATH. Install Claude Code first: https://claude.com/claude-code",
    )?;

    let mut cmd = Command::new(&claude);
    cmd.args(&opts.claude_args);

    // An inherited key would take precedence over our proxy token and silently
    // send traffic to Anthropic instead of the chosen provider.
    cmd.env_remove("ANTHROPIC_API_KEY");

    // Held so the proxy outlives the env setup and dies when we return.
    let _proxy_guard;

    match provider.mode() {
        Mode::Native => {
            // Nothing to intercept: let Claude Code use its own auth.
            if model.is_some() || opts.small_model.is_some() {
                apply_models(&mut cmd, &model, &opts.small_model);
            }
        }
        Mode::Direct => {
            let base = provider
                .direct_base_url()
                .expect("Direct providers define a base URL");
            let key = creds::api_key(provider)?;
            cmd.env("ANTHROPIC_BASE_URL", base);
            cmd.env("ANTHROPIC_AUTH_TOKEN", key);
            apply_models(&mut cmd, &model, &opts.small_model);
            apply_common(&mut cmd);
        }
        Mode::Proxied => {
            let p = proxy::spawn(provider, opts.effort.as_deref(), opts.verbose)?;
            cmd.env("ANTHROPIC_BASE_URL", p.base_url());
            // ccp does not authenticate local clients; it holds the real
            // provider credentials itself and binds to loopback only.
            cmd.env("ANTHROPIC_AUTH_TOKEN", "unused");
            apply_models(&mut cmd, &model, &opts.small_model);
            apply_common(&mut cmd);

            if provider == Provider::Openai && std::env::var_os("CLAUDE_CODE_AUTO_COMPACT_WINDOW").is_none() {
                cmd.env("CLAUDE_CODE_AUTO_COMPACT_WINDOW", CODEX_COMPACT_WINDOW);
            }

            if opts.verbose {
                eprintln!(
                    "agpx: {} via {} on {}",
                    provider.name(),
                    proxy::BINARY,
                    p.base_url()
                );
            }
            _proxy_guard = p;
        }
    }

    if provider.mode() == Mode::Native {
        // No proxy to tear down, so hand the terminal over entirely.
        let err = cmd.exec();
        return Err(err).context("failed to exec claude");
    }

    let status = cmd.status().context("failed to run claude")?;
    Ok(status.code().unwrap_or(1))
}

fn apply_models(cmd: &mut Command, model: &Option<String>, small: &Option<String>) {
    if let Some(model) = model {
        cmd.env("ANTHROPIC_MODEL", model);
    }
    if let Some(small) = small {
        // Claude Code renamed this; set both so we work across 2.x versions.
        cmd.env("ANTHROPIC_SMALL_FAST_MODEL", small);
        cmd.env("ANTHROPIC_DEFAULT_HAIKU_MODEL", small);
    }
}

fn apply_common(cmd: &mut Command) {
    // Keep Claude Code from calling Anthropic endpoints the provider cannot serve.
    cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    // Non-streaming fallback is not implemented by the translating providers.
    cmd.env("CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK", "1");
}
