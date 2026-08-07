use anyhow::{anyhow, bail, Context, Result};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::provider::Provider;

pub const BINARY: &str = "claude-code-proxy";

/// A claude-code-proxy we started and are responsible for killing.
///
/// One per agpx invocation rather than a shared daemon: CCP_CODEX_EFFORT is
/// read from the server process env and overrides per-request effort, so a
/// shared server could not honour --effort per session.
pub struct Proxy {
    child: Child,
    pub port: u16,
}

impl Proxy {
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        // SIGTERM first so ccp can flush its logs, then reap.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Path where the install script places the pinned ccp binary.
fn managed_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from_iter([
        &home,
        std::ffi::OsStr::new(".local/share/agpx/bin"),
    ]))
}

fn managed_binary() -> Option<std::path::PathBuf> {
    managed_dir().map(|d| d.join(BINARY))
}

pub fn find_binary() -> Option<std::path::PathBuf> {
    // Always prefer the private copy the install script put in place.
    if let Some(p) = managed_binary() {
        if p.is_file() {
            return Some(p);
        }
    }
    which(BINARY)
}

pub fn which(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
}

fn install_hint() -> String {
    format!(
        "{BINARY} not found.\n\n\
         Re-run the agpx installer to pull in the managed copy:\n  \
         curl -fsSL https://raw.githubusercontent.com/ftorrresd/agpx/main/scripts/install.sh | sh"
    )
}

pub fn require_binary() -> Result<std::path::PathBuf> {
    find_binary().ok_or_else(|| anyhow!(install_hint()))
}

/// Ask the OS for a free port, then immediately release it.
///
/// There is a small race between releasing and ccp binding. Retrying the whole
/// spawn is cheaper than holding the socket and handing ccp an inherited fd.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("could not reserve a local port")?;
    Ok(listener.local_addr()?.port())
}

/// Start a private ccp and wait until it accepts connections.
pub fn spawn(provider: Provider, effort: Option<&str>, verbose: bool) -> Result<Proxy> {
    let binary = require_binary()?;
    let port = free_port()?;

    let mut cmd = Command::new(&binary);
    cmd.arg("serve")
        .arg("--port")
        .arg(port.to_string())
        // The monitor TUI would fight Claude Code for the terminal.
        .arg("--no-monitor")
        .env("CCP_BIND_ADDRESS", "127.0.0.1")
        .stdin(Stdio::null());

    if verbose {
        cmd.env("CCP_LOG_VERBOSE", "1")
            .env("CCP_LOG_STDERR", "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    }

    if let Some(alias) = provider.alias_provider() {
        cmd.env("CCP_ALIAS_PROVIDER", alias);
    }
    if let Some(effort) = effort {
        // Server-side and absolute: this overrides whatever effort the
        // request carries, which is exactly why the server is per-session.
        cmd.env("CCP_CODEX_EFFORT", effort);
    }

    // Put ccp in its own process group so Ctrl+C in Claude Code does not also
    // interrupt the proxy out from under it. We kill it explicitly on Drop.
    unsafe {
        cmd.pre_exec(|| {
            libc::setpgid(0, 0);
            Ok(())
        });
    }

    let child = cmd
        .spawn()
        .with_context(|| format!("failed to start {}", binary.display()))?;
    let mut proxy = Proxy { child, port };

    wait_until_ready(&mut proxy, Duration::from_secs(15))?;
    Ok(proxy)
}

fn wait_until_ready(proxy: &mut Proxy, timeout: Duration) -> Result<()> {
    let addr: SocketAddr = ([127, 0, 0, 1], proxy.port).into();
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        // If ccp died (bad auth, port taken), surface its message rather than
        // spinning until the timeout.
        if let Ok(Some(status)) = proxy.child.try_wait() {
            let mut detail = String::new();
            if let Some(mut err) = proxy.child.stderr.take() {
                use std::io::Read;
                let _ = err.read_to_string(&mut detail);
            }
            let detail = detail.trim();
            bail!(
                "{BINARY} exited before it was ready ({status}){}{}",
                if detail.is_empty() { "" } else { ":\n" },
                detail
            );
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    bail!(
        "{BINARY} did not start listening on 127.0.0.1:{} within {}s",
        proxy.port,
        timeout.as_secs()
    )
}

/// Hand a subcommand straight to ccp (used by `agpx login` and `agpx models`).
pub fn delegate(args: &[&str]) -> Result<i32> {
    let binary = require_binary()?;
    let status = Command::new(&binary)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    Ok(status.code().unwrap_or(1))
}
