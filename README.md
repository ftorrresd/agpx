# agpx

Launch Claude Code against any provider in one command.

```
agpx --openai --model gpt-5.6-sol --effort xhigh
agpx --deepseek --model deepseek-chat
agpx --anthropic
```

A thin Rust launcher that wraps [claude-code-proxy](https://github.com/raine/claude-code-proxy) for subscription providers (Codex, Kimi, Grok, Cursor) and points Claude Code directly at Anthropic-compatible endpoints for everything else.

## How it works

agpx starts a private `claude-code-proxy serve` on an ephemeral port for subscription-backed providers, then spawns Claude Code pointed at it. When Claude Code exits, the proxy is killed. No daemon, no long-lived process, no config drift between sessions.

| `agpx --anthropic` | No proxy. Claude Code uses its own auth. |
| `agpx --deepseek` | Points `ANTHROPIC_BASE_URL` at DeepSeek's Anthropic-compatible endpoint. |
| `agpx --openai` | Spawns a per-session `claude-code-proxy` translating to the Codex subscription backend. |
| `agpx --kimi` etc. | Same proxy model for Kimi, Grok, and Cursor subscriptions. |

## Install

```sh
cargo install --path .
```

Requires:
- **Rust** 1.94+
- **[claude-code-proxy](https://github.com/raine/claude-code-proxy)** for subscription providers:
  ```sh
  brew install raine/claude-code-proxy/claude-code-proxy
  ```
- **[Claude Code](https://claude.com/claude-code)** on PATH

## Quick start

### Subscription providers (OpenAI Codex, Kimi, Grok, Cursor)

```sh
# Log in once (delegates to claude-code-proxy for OAuth)
agpx login openai

# Launch
agpx --openai --model gpt-5.6-sol --effort xhigh
```

### API-key providers (DeepSeek)

```sh
# Store your key
agpx login deepseek
# or: export DEEPSEEK_API_KEY=sk-...

# Launch
agpx --deepseek --model deepseek-chat
```

### Anthropic

```sh
agpx --anthropic
# same as running `claude` directly
```

## Usage

```
agpx [--provider <NAME>] [--model <MODEL>] [--small-model <MODEL>]
     [--effort <EFFORT>] [--verbose] [--] [<claude-flags>...]

agpx login <PROVIDER> [KEY]
agpx logout <PROVIDER>
agpx models [PROVIDER]
```

| Flag | |
|---|---|
| `--provider` | anthropic, deepseek, openai, kimi, grok, cursor (default: anthropic) |
| `--model` | Model to request. Omit for provider defaults or alias resolution. |
| `--small-model` | Cheap model for background tasks (chat titles, etc.) |
| `--effort` | Reasoning effort: `none` `low` `medium` `high` `xhigh` `max` (Codex only) |
| `--verbose` | Show what agpx is doing underneath |

Everything after `--` is forwarded to Claude Code:

```sh
agpx --openai --model gpt-5.6-sol -- --dangerously-skip-permissions
```

## Credential storage

- **Subscription providers**: OAuth tokens live in claude-code-proxy's own store (`~/.config/claude-code-proxy/`). agpx delegates `login`/`logout` to it.
- **API-key providers**: Keys are stored at `~/.config/agpx/credentials.json` (0600). Or export the env var: `DEEPSEEK_API_KEY`.
- **Anthropic**: Nothing stored. Claude Code handles its own auth.

## Caveats

- **`--effort` is per-session global.** ccp reads effort from the server env, so `--effort` applies to every request in that session.
- **WebSearch/WebFetch break** on non-Anthropic providers. Claude Code's server-side tools don't translate.
- **Ctrl+C may leak a ccp process.** The proxy is in a separate process group so it survives Claude Code's exit; agpx kills it on clean shutdown. SIGKILL to agpx will orphan it.
- **Unofficial client.** Subscription providers may enforce terms against third-party clients.

## License

MIT
