#!/usr/bin/env bash
# Icebox installer
# Usage: curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
set -euo pipefail

REPO="SteelCrab/icebox"
INSTALL_DIR="${ICEBOX_INSTALL_DIR:-$HOME/.local/bin}"

info()  { printf "\033[1;34m=>\033[0m %s\n" "$1"; }
error() { printf "\033[1;31merror:\033[0m %s\n" "$1" >&2; exit 1; }

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) PLATFORM="darwin" ;;
  Linux)  PLATFORM="linux" ;;
  *)      error "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64)        ARCH="x86_64" ;;
  *)             error "Unsupported architecture: $ARCH" ;;
esac

ASSET="icebox"
URL="https://github.com/$REPO/releases/latest/download/$ASSET"

info "Downloading icebox for $PLATFORM/$ARCH..."
TMPFILE="$(mktemp)"
if ! curl -fsSL "$URL" -o "$TMPFILE"; then
  rm -f "$TMPFILE"
  error "Download failed. Check https://github.com/$REPO/releases for available binaries."
fi

chmod +x "$TMPFILE"

# Ensure install directory exists
mkdir -p "$INSTALL_DIR"
mv "$TMPFILE" "$INSTALL_DIR/icebox"

info "Installed to $INSTALL_DIR/icebox"

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  info "Add this to your shell profile:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

echo ""
info "Done! Run 'icebox' to get started."
