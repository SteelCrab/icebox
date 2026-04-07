#!/usr/bin/env bash
# Icebox installer
# Usage: curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
#
# Supports:
#   - macOS (aarch64, x86_64)
#   - Linux glibc (x86_64, aarch64)
#   - Linux musl  (x86_64, aarch64)
#   - Linux armv7 (Raspberry Pi 2/3)
#
# Override install location: ICEBOX_INSTALL_DIR=/path bash
set -euo pipefail

REPO="SteelCrab/icebox"
INSTALL_DIR="${ICEBOX_INSTALL_DIR:-$HOME/.local/bin}"

info()  { printf "\033[1;34m=>\033[0m %s\n" "$1"; }
error() { printf "\033[1;31merror:\033[0m %s\n" "$1" >&2; exit 1; }

# --- Detect platform ---
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="x86_64-apple-darwin" ;;
      *)             error "Unsupported macOS architecture: $ARCH" ;;
    esac
    ;;
  Linux)
    # Detect libc (musl vs glibc)
    if ldd --version 2>&1 | grep -qi musl; then
      LIBC="musl"
    else
      LIBC="gnu"
    fi
    case "$ARCH" in
      x86_64)
        TARGET="x86_64-unknown-linux-${LIBC}"
        ;;
      aarch64|arm64)
        TARGET="aarch64-unknown-linux-${LIBC}"
        ;;
      armv7l)
        TARGET="armv7-unknown-linux-gnueabihf"
        ;;
      *)
        error "Unsupported Linux architecture: $ARCH. See https://github.com/$REPO/releases for available binaries."
        ;;
    esac
    ;;
  *)
    error "Unsupported OS: $OS. See https://github.com/$REPO/releases for available binaries."
    ;;
esac

ASSET="icebox-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/latest/download/$ASSET"

info "Detected platform: $TARGET"
info "Downloading $ASSET..."

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if ! curl -fsSL "$URL" -o "$TMPDIR/$ASSET"; then
  error "Download failed: $URL
Check https://github.com/$REPO/releases for available binaries."
fi

# --- Extract ---
info "Extracting..."
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"

if [[ ! -f "$TMPDIR/icebox" ]]; then
  error "Archive did not contain an 'icebox' binary."
fi

chmod +x "$TMPDIR/icebox"

# --- Install ---
mkdir -p "$INSTALL_DIR"
mv "$TMPDIR/icebox" "$INSTALL_DIR/icebox"

info "Installed to $INSTALL_DIR/icebox"

# --- PATH check ---
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  info "Add this to your shell profile (~/.zshrc, ~/.bashrc):"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

echo ""
info "Done! Run 'icebox' to get started."
