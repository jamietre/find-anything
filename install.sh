#!/bin/sh
# find-anything installer
# Usage: curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/main/install.sh | sh
#
# Options (environment variables):
#   INSTALL_DIR   Destination directory (default: ~/.local/bin)
#   VERSION       Specific release tag to install (default: latest)

set -e

REPO="jamietre/find-anything"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# ── Detect platform ────────────────────────────────────────────────────────────

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  OS_NAME="linux" ;;
  Darwin) OS_NAME="macos" ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64)          ARCH_NAME="x86_64" ;;
  aarch64 | arm64) ARCH_NAME="aarch64" ;;
  *)
    echo "Unsupported architecture: $ARCH"
    exit 1
    ;;
esac

PLATFORM="${OS_NAME}-${ARCH_NAME}"

# ── Resolve version ────────────────────────────────────────────────────────────

if [ -z "$VERSION" ]; then
  echo "Fetching latest release..."
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  if [ -z "$VERSION" ]; then
    echo "Failed to determine latest version. Set VERSION explicitly and retry."
    exit 1
  fi
fi

echo "Installing find-anything ${VERSION} (${PLATFORM})..."

# ── Download and extract ───────────────────────────────────────────────────────

TARBALL="find-anything-${VERSION}-${PLATFORM}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${TARBALL}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${URL}..."
curl -fsSL "$URL" -o "${TMPDIR}/${TARBALL}"

echo "Extracting..."
tar -xzf "${TMPDIR}/${TARBALL}" -C "${TMPDIR}"

# ── Install binaries ───────────────────────────────────────────────────────────

mkdir -p "$INSTALL_DIR"
EXTRACTED_DIR="${TMPDIR}/find-anything-${VERSION}-${PLATFORM}"

BINARIES="find find-scan find-watch find-server \
  find-extract-text find-extract-pdf find-extract-media find-extract-archive"

for bin in $BINARIES; do
  if [ -f "${EXTRACTED_DIR}/${bin}" ]; then
    install -m 755 "${EXTRACTED_DIR}/${bin}" "${INSTALL_DIR}/${bin}"
  fi
done

echo ""
echo "Installed to: ${INSTALL_DIR}"
echo "  find-server          — search server"
echo "  find-scan            — initial indexer"
echo "  find-watch           — incremental file watcher"
echo "  find                 — command-line search client"
echo "  find-extract-*       — extractor binaries (used by find-watch)"
echo ""

# ── PATH check ────────────────────────────────────────────────────────────────

case ":$PATH:" in
  *":${INSTALL_DIR}:"*)
    echo "Ready! Run 'find-server --help' to get started."
    ;;
  *)
    echo "NOTE: ${INSTALL_DIR} is not in your PATH."
    echo "Add this to your shell profile:"
    echo ""
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    ;;
esac
