#!/bin/sh
# find-anything client installer
# Installs find-scan, find-watch, find, and the extractor binaries.
# Configures the client to talk to a running find-anything server.
#
# Usage: curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/master/install.sh | sh
#
# For server installation use install-server.sh instead:
#   curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/master/install-server.sh | sh
#
# Options (environment variables):
#   INSTALL_DIR   Destination directory (default: ~/.local/bin)
#   VERSION       Specific release tag to install (default: latest)
#   SKIP_CONFIG   Set to 1 to skip the configuration prompts

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
  armv7l)          ARCH_NAME="armv7" ;;
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

BINARIES="find-anything find-scan find-watch find-server \
  find-extract-text find-extract-pdf find-extract-media find-extract-archive \
  find-extract-html find-extract-office find-extract-epub"

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
echo "  find-anything        — command-line search client"
echo "  find-extract-*       — extractor binaries (used by find-watch)"
echo ""

# ── PATH check ────────────────────────────────────────────────────────────────

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo "NOTE: ${INSTALL_DIR} is not in your PATH."
    echo "Add this to your shell profile:"
    echo ""
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    ;;
esac

# ── Configuration ─────────────────────────────────────────────────────────────

if [ "${SKIP_CONFIG:-0}" = "1" ]; then
  echo "Skipping configuration (SKIP_CONFIG=1)."
  exit 0
fi

# Determine config directory
if [ -n "$XDG_CONFIG_HOME" ]; then
  CONFIG_DIR="$XDG_CONFIG_HOME/find-anything"
else
  CONFIG_DIR="$HOME/.config/find-anything"
fi
CONFIG_FILE="$CONFIG_DIR/client.toml"

if [ -f "$CONFIG_FILE" ]; then
  echo "Configuration already exists at $CONFIG_FILE"
  printf "Re-configure? [y/N] "
  read -r RECONFIGURE </dev/tty
  case "$RECONFIGURE" in
    y|Y) ;;
    *)
      echo "Skipping configuration. Existing config preserved."
      exit 0
      ;;
  esac
fi

echo "Client configuration"
echo "  find-anything server URL and token (from your server's server.toml)."
echo ""

printf "Server URL [http://localhost:8765]: "
read -r SERVER_URL </dev/tty
SERVER_URL="${SERVER_URL:-http://localhost:8765}"

printf "Bearer token (from server.toml): "
# Try to disable echo for password-style input; fall back gracefully
if command -v stty >/dev/null 2>&1; then
  stty -echo </dev/tty 2>/dev/null || true
  read -r TOKEN </dev/tty
  stty echo </dev/tty 2>/dev/null || true
  echo ""
else
  read -r TOKEN </dev/tty
fi

if [ -z "$TOKEN" ]; then
  echo "Token cannot be empty." >&2
  exit 1
fi

printf "Directories to index (space-separated) [%s]: " "$HOME"
read -r DIRS_INPUT </dev/tty
if [ -z "$DIRS_INPUT" ]; then
  DIRS_INPUT="$HOME"
fi

# ── Write client.toml ─────────────────────────────────────────────────────────

mkdir -p "$CONFIG_DIR"

# Build TOML paths array from space-separated input
PATHS_TOML=""
FIRST=1
for dir in $DIRS_INPUT; do
  # Escape backslashes and quotes (unlikely on Linux but be safe)
  escaped="$(printf '%s' "$dir" | sed 's/\\/\\\\/g; s/"/\\"/g')"
  if [ "$FIRST" = "1" ]; then
    PATHS_TOML="\"$escaped\""
    FIRST=0
  else
    PATHS_TOML="$PATHS_TOML, \"$escaped\""
  fi
done

# Escape URL and token for TOML
SERVER_URL_ESC="$(printf '%s' "$SERVER_URL" | sed 's/\\/\\\\/g; s/"/\\"/g')"
TOKEN_ESC="$(printf '%s' "$TOKEN" | sed 's/\\/\\\\/g; s/"/\\"/g')"

cat > "$CONFIG_FILE" <<EOF
[server]
url   = "$SERVER_URL_ESC"
token = "$TOKEN_ESC"

[[sources]]
name  = "home"
paths = [$PATHS_TOML]
# base_url = ""   # Optional: public URL prefix for file links in search results

[scan]
# max_file_size_kb = 1024   # Skip files larger than this (KB)
# max_line_length  = 120    # Wrap long lines at this column (0 = disable)
# follow_symlinks  = false
# include_hidden   = false  # Index dot-files and dot-directories
# ocr              = false  # OCR image files (requires tesseract)
# exclude = [               # Glob patterns to skip (these are the defaults)
#   "**/.git/**",
#   "**/node_modules/**",
#   "**/target/**",
#   "**/__pycache__/**",
#   "**/.next/**",
#   "**/dist/**",
#   "**/.cache/**",
#   "**/.tox/**",
#   "**/.venv/**",
#   "**/venv/**",
#   "**/*.pyc",
#   "**/*.class",
# ]

[scan.archives]
# enabled   = true
# max_depth = 10   # Max nesting depth for archives-within-archives

[watch]
# debounce_ms   = 500   # Wait this long (ms) after last change before re-indexing
# extractor_dir = ""    # Path to find-extract-* binaries (default: auto-detect)
EOF

echo ""
echo "Configuration written to: $CONFIG_FILE"
echo "  Edit this file to add more sources, change exclude patterns, etc."

# ── Install systemd user service ──────────────────────────────────────────────

echo ""
echo "Setting up find-watch service..."

if command -v systemctl >/dev/null 2>&1 && systemctl --user status >/dev/null 2>&1; then
  # systemd is available
  SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
  SERVICE_FILE="$SYSTEMD_USER_DIR/find-watch.service"
  mkdir -p "$SYSTEMD_USER_DIR"

  # Write the service unit with the actual binary path
  cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=find-anything file watcher
After=network.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/find-watch --config ${CONFIG_FILE}
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=find_watch=info
Environment=PATH=${INSTALL_DIR}:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=default.target
EOF

  systemctl --user daemon-reload
  systemctl --user enable find-watch
  systemctl --user start find-watch

  echo ""
  echo "find-watch systemd user service installed and started."
  echo "  Status:  systemctl --user status find-watch"
  echo "  Logs:    journalctl --user -u find-watch -f"
  echo "  Stop:    systemctl --user stop find-watch"

elif [ "$OS_NAME" = "macos" ]; then
  # macOS: suggest launchd
  PLIST_DIR="$HOME/Library/LaunchAgents"
  PLIST_FILE="$PLIST_DIR/com.jamietre.find-watch.plist"
  mkdir -p "$PLIST_DIR"

  cat > "$PLIST_FILE" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.jamietre.find-watch</string>
  <key>ProgramArguments</key>
  <array>
    <string>${INSTALL_DIR}/find-watch</string>
    <string>--config</string>
    <string>${CONFIG_FILE}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${HOME}/Library/Logs/find-watch.log</string>
  <key>StandardErrorPath</key>
  <string>${HOME}/Library/Logs/find-watch.log</string>
</dict>
</plist>
EOF

  launchctl load "$PLIST_FILE"

  echo ""
  echo "find-watch launchd agent installed and started."
  echo "  Status:  launchctl list com.jamietre.find-watch"
  echo "  Logs:    tail -f ~/Library/Logs/find-watch.log"
  echo "  Stop:    launchctl unload $PLIST_FILE"

else
  # Non-systemd Linux: print manual instructions
  echo ""
  echo "Autostart not configured (systemd user session not detected)."
  echo "Run find-watch manually:"
  echo ""
  echo "  ${INSTALL_DIR}/find-watch --config ${CONFIG_FILE}"
  echo ""
  echo "Or add it to your session startup script."
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "Installation complete!"
echo ""
echo "  Server:    $SERVER_URL"
echo "  Binaries:  $INSTALL_DIR"
echo ""
echo "  Config:    $CONFIG_FILE"
echo "    ^ Edit this file to add sources, change excludes, etc."
echo ""
echo "Next step — run the initial scan:"
echo ""
echo "  find-scan --config $CONFIG_FILE --full"
echo ""
echo "This indexes all configured directories. Run it once before"
echo "find-watch will have anything useful to keep up to date."
