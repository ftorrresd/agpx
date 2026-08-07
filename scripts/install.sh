#!/bin/sh
set -eu

# ---------------------------------------------------------------------------
# agpx — one-shot installer
#
#   curl -fsSL https://raw.githubusercontent.com/ftorrresd/agpx/main/scripts/install.sh | sh
#
# Downloads the latest musl-static binary from GitHub Releases and puts it
# into /usr/local/bin (with sudo) or ~/.local/bin.
# ---------------------------------------------------------------------------

REPO="ftorrresd/agpx"
BINARY="agpx"
BIN_DIR=""
USE_SUDO=""

# --- helpers ----------------------------------------------------------------

bold()  { printf '\033[1m%s\033[0m\n' "$*"; }
red()   { printf '\033[31m%s\033[0m\n' "$*"; }
abort() { red "Error: $*" >&2; exit 1; }

# --- target selection -------------------------------------------------------

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)
    # musl binary runs anywhere on Linux
    ;;
  *)
    abort "$OS is not supported (only Linux for now)"
    ;;
esac

case "$ARCH" in
  x86_64|amd64)  : ;;
  aarch64|arm64) abort "arm64 not yet built; open an issue at https://github.com/$REPO" ;;
  *)             abort "unsupported architecture: $ARCH" ;;
esac

# --- install path -----------------------------------------------------------

if [ -w /usr/local/bin ]; then
  BIN_DIR="/usr/local/bin"
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  BIN_DIR="/usr/local/bin"
  USE_SUDO="sudo"
else
  BIN_DIR="${HOME}/.local/bin"
  mkdir -p "$BIN_DIR"
fi

# --- download ---------------------------------------------------------------

URL="https://github.com/$REPO/releases/latest/download/$BINARY"
bold "→ Downloading $BINARY from $URL"

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "/tmp/$BINARY"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$URL" -O "/tmp/$BINARY"
else
  abort "need curl or wget"
fi

chmod +x "/tmp/$BINARY"

# --- install ----------------------------------------------------------------

bold "→ Installing to $BIN_DIR"
if [ -n "$USE_SUDO" ]; then
  sudo mv "/tmp/$BINARY" "$BIN_DIR/$BINARY"
else
  mv "/tmp/$BINARY" "$BIN_DIR/$BINARY"
fi

# --- PATH check -------------------------------------------------------------

if ! command -v "$BINARY" >/dev/null 2>&1; then
  case "$SHELL" in
    */zsh)  RC="$HOME/.zshrc"  ;;
    */bash) RC="$HOME/.bashrc" ;;
    */fish) RC="$HOME/.config/fish/config.fish" ;;
    *)      RC="$HOME/.profile" ;;
  esac
  bold "Note: $BIN_DIR is not on your PATH."
  bold "      Add \`export PATH=\"$BIN_DIR:\$PATH\"\` to $RC and restart your shell."
fi

bold "✓ $BINARY installed ($BIN_DIR/$BINARY)"
"$BIN_DIR/$BINARY" --version
