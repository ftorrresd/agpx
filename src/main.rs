use std::ffi::OsString;

use anyhow::{bail, Result};
use clap::{ArgAction, Parser, Subcommand};

use agpx::creds;
use agpx::launch::{self, Options};
use agpx::provider::{validate_effort, Provider};

/// Launch Claude Code against any provider in one command.
///
/// Everything after `--` is forwarded directly to Claude Code:
///
///   agpx openai --model gpt-5.6-sol -- --dangerously-skip-permissions
#[derive(Parser)]
#[command(name = "agpx", version, about)]
struct Cli {
    /// Provider to use. One of: anthropic, deepseek, openai, kimi, grok, cursor.
    /// Defaults to anthropic when omitted.
    provider: Option<String>,

    /// Model to use. When omitted the provider resolves a sensible default or
    /// delegates to the server alias.
    #[arg(long, global = true)]
    model: Option<String>,

    /// Small/fast model for cheap background operations (chat titles, etc.).
    #[arg(long, global = true)]
    small_model: Option<String>,

    /// Reasoning effort (sets CLAUDE_CODE_EFFORT_LEVEL).
    /// Accepted: none, low, medium, high, xhigh, max.
    #[arg(long, global = true)]
    effort: Option<String>,

    /// Print what agpx is doing underneath.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Clone)]
enum Commands {
    /// Store an API key or log into a subscription provider.
    Login {
        /// Provider to log into.
        provider: String,
        /// API key for the provider (skip to use interactive/OAuth login).
        key: Option<String>,
    },
    /// Remove stored credentials for a provider.
    Logout {
        /// Provider to log out of.
        provider: String,
    },
    /// List models available for the given provider. Delegates to
    /// claude-code-proxy for subscription providers.
    Models {
        /// Provider whose catalog to show.
        provider: Option<String>,
    },
}

// ── entry ──────────────────────────────────────────────────────────────────

fn resolve_provider(cli: &Cli) -> &str {
    cli.provider.as_deref().unwrap_or("anthropic")
}

fn main() -> Result<()> {
    let (agpx_args, claude_args) = split_on_dashdash(std::env::args_os());

    let cli = Cli::parse_from(agpx_args);

    if let Some(ref cmd) = cli.command {
        return dispatch(cmd.clone(), &cli);
    }

    let provider_name = resolve_provider(&cli);
    let provider = Provider::from_name(provider_name)?;

    if let Some(ref effort) = cli.effort {
        validate_effort(effort)?;
    }

    let opts = Options {
        provider,
        model: cli.model,
        small_model: cli.small_model,
        effort: cli.effort,
        verbose: cli.verbose,
        claude_args,
    };

    let exit = launch::run(opts)?;
    std::process::exit(exit);
}

/// Returns (args‑before‑`--`, args‑after‑`--`).
///
/// Does this ourselves instead of letting clap handle it, because the tail
/// is foreign — it belongs to Claude Code and can contain flags that would
/// collide with ours.
fn split_on_dashdash(args: impl IntoIterator<Item = OsString>) -> (Vec<OsString>, Vec<OsString>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut past = false;

    // Always keep argv[0] on the left side (clap needs it).
    for (i, a) in args.into_iter().enumerate() {
        if i == 0 {
            left.push(a);
            continue;
        }
        if past {
            right.push(a);
        } else if a == "--" {
            past = true;
        } else {
            left.push(a);
        }
    }

    (left, right)
}

// ── subcommand dispatch ────────────────────────────────────────────────────

fn dispatch(cmd: Commands, cli: &Cli) -> Result<()> {
    match cmd {
        Commands::Login { provider, key } => login(provider, key),
        Commands::Logout { provider } => logout(provider),
        Commands::Models { provider: maybe_prov } => models(maybe_prov, cli),
    }
}

fn login(provider: String, key: Option<String>) -> Result<()> {
    let provider = Provider::from_name(&provider)?;
    match provider.mode() {
        agpx::provider::Mode::Native => {
            eprintln!(
                "{} does not need an API key — Claude Code handles auth directly.",
                provider.name()
            );
        }
        agpx::provider::Mode::Direct => {
            let key = key.unwrap_or_else(|| {
                eprint!("{} API key: ", provider.name());
                let mut buf = String::new();
                std::io::stdin()
                    .read_line(&mut buf)
                    .map(|_| buf.trim().to_string())
                    .unwrap_or_default()
            });
            creds::set_api_key(provider, &key)?;
            eprintln!("API key stored for {}.", provider.name());
        }
        agpx::provider::Mode::Proxied => {
            let name = provider.ccp_name().unwrap_or(provider.name());
            if let Some(key) = key {
                creds::set_api_key(provider, &key)?;
                eprintln!("API key stored for {}.", provider.name());
            } else {
                let code = agpx::proxy::delegate(&[name, "auth", "login"])?;
                if code != 0 {
                    bail!("login failed (exit code {code})");
                }
            }
        }
    }
    Ok(())
}

fn logout(provider: String) -> Result<()> {
    let provider = Provider::from_name(&provider)?;
    match provider.mode() {
        agpx::provider::Mode::Native => {
            eprintln!("{} has no stored credentials to remove.", provider.name());
        }
        agpx::provider::Mode::Direct => {
            if creds::clear_api_key(provider)? {
                eprintln!("API key removed for {}.", provider.name());
            } else {
                eprintln!("No API key was stored for {}.", provider.name());
            }
        }
        agpx::provider::Mode::Proxied => {
            let name = provider.ccp_name().unwrap_or(provider.name());
            let code = agpx::proxy::delegate(&[name, "auth", "logout"])?;
            if code != 0 {
                bail!("logout command failed (exit code {code})");
            }
        }
    }
    Ok(())
}

fn models(maybe_prov: Option<String>, cli: &Cli) -> Result<()> {
    let provider_name = maybe_prov
        .as_deref()
        .unwrap_or_else(|| resolve_provider(cli));
    let provider = Provider::from_name(provider_name)?;

    match provider.mode() {
        agpx::provider::Mode::Native => {
            println!("Anthropic models are resolved by Claude Code itself.");
            println!("Run `claude` and pick a model from the /menu.");
        }
        agpx::provider::Mode::Direct => {
            println!("DeepSeek models (Anthropic-compatible endpoint):");
            println!("  deepseek-v4-pro   — 1.6T MoE, strong reasoning");
            println!("  deepseek-v4-flash  — 284B MoE, fast & cheap");
        }
        agpx::provider::Mode::Proxied => {
            let name = provider.ccp_name().unwrap_or(provider.name());
            let output = agpx::proxy::delegate_output(&["models"])?;
            for line in output.lines() {
                // Each line is "provider: model, model, ..." — print only the
                // section matching the requested provider.
                if line.starts_with(&format!("{name}:")) || !line.contains(':') {
                    println!("{line}");
                }
            }
        }
    }
    Ok(())
}
