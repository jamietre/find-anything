#!/bin/sh
# find-anything client installer
# Installs find-scan, find-watch, find-anything, and the extractor binaries.
# Configures the client to talk to a running find-anything server.
#
# Usage: curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/master/install.sh | sh
#
# For server installation use install-server.sh instead:
#   curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/master/install-server.sh | sh
#
# SKIP_CONFIG=1 skips all interactive prompts (binaries only, no config written)

set -e

# Print the appropriate `systemctl restart find-watch` command for this machine.
print_restart_cmd() {
  set +e
  if systemctl --user status find-watch >/dev/null 2>&1; then
    echo "  systemctl --user restart find-watch"
  elif systemctl status find-watch >/dev/null 2>&1; then
    echo "  sudo systemctl restart find-watch"
  else
    echo "  systemctl --user restart find-watch   # or: sudo systemctl restart find-watch"
  fi
  set -e
}

REPO="jamietre/find-anything"

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

LATEST_VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
if [ -z "$LATEST_VERSION" ]; then
  LATEST_VERSION="(unknown)"
fi

if [ -n "$VERSION" ]; then
  echo "Latest version: ${LATEST_VERSION}"
  echo "Using VERSION override: ${VERSION}"
else
  VERSION="$LATEST_VERSION"
  echo "Latest version: ${VERSION}"
fi

if [ -z "$VERSION" ] || [ "$VERSION" = "(unknown)" ]; then
  echo "Could not determine latest version. Set VERSION explicitly and retry." >&2
  exit 1
fi

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# ── Install directory prompt ───────────────────────────────────────────────────

if [ "${SKIP_CONFIG:-0}" != "1" ]; then
  printf "Install directory [%s]: " "$INSTALL_DIR"
  read -r INSTALL_DIR_INPUT </dev/tty
  INSTALL_DIR="${INSTALL_DIR_INPUT:-$INSTALL_DIR}"
fi

echo ""
echo "Installing find-anything ${VERSION} (${PLATFORM}) to ${INSTALL_DIR}..."

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
      echo ""
      echo "Restart the watcher to pick up the new binary:"
      echo ""
      print_restart_cmd
      echo ""
      exit 0
      ;;
  esac
fi

echo "Client configuration"
echo "  find-anything server URL and token (from your server's server.toml)."
echo ""

while true; do
  printf "Server URL [http://localhost:8765]: "
  read -r SERVER_URL </dev/tty
  SERVER_URL="${SERVER_URL:-http://localhost:8765}"

  # Test connectivity (no auth needed — just checking the server is up)
  printf "Checking server connectivity... "
  if curl -fsS --max-time 5 "${SERVER_URL}/" >/dev/null 2>&1; then
    echo "OK"
    break
  else
    echo "no response"
    echo ""
    echo "WARNING: Could not reach ${SERVER_URL}"
    echo "  Make sure the server is running and the URL is correct."
    echo ""
    printf "Re-enter URL, or press enter to continue anyway? [re-enter/continue]: "
    read -r CONN_CHOICE </dev/tty
    case "${CONN_CHOICE:-re-enter}" in
      c|co|con|cont|conti|contin|continu|continue) break ;;
      *) echo "" ;;
    esac
  fi
done

printf "Bearer token (from server.toml): "
read -r TOKEN </dev/tty

if [ -z "$TOKEN" ]; then
  echo "Token cannot be empty." >&2
  exit 1
fi

DEFAULT_SOURCE_NAME="$(hostname | cut -d. -f1)"
printf "Source name (identifies this machine in search results) [%s]: " "$DEFAULT_SOURCE_NAME"
read -r SOURCE_NAME </dev/tty
SOURCE_NAME="${SOURCE_NAME:-$DEFAULT_SOURCE_NAME}"

printf "Directories to index (semicolon-separated) [%s]: " "$HOME"
read -r DIRS_INPUT </dev/tty
if [ -z "$DIRS_INPUT" ]; then
  DIRS_INPUT="$HOME"
fi

# ── Write client.toml ─────────────────────────────────────────────────────────

mkdir -p "$CONFIG_DIR"

# Build TOML paths array from semicolon-separated input
PATHS_TOML=""
FIRST=1
OLD_IFS="$IFS"
IFS=';'
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
IFS="$OLD_IFS"

# Escape URL, token, and source name for TOML
SERVER_URL_ESC="$(printf '%s' "$SERVER_URL" | sed 's/\\/\\\\/g; s/"/\\"/g')"
TOKEN_ESC="$(printf '%s' "$TOKEN" | sed 's/\\/\\\\/g; s/"/\\"/g')"
SOURCE_NAME_ESC="$(printf '%s' "$SOURCE_NAME" | sed 's/\\/\\\\/g; s/"/\\"/g')"

cat > "$CONFIG_FILE" <<EOF
[server]
url   = "$SERVER_URL_ESC"
token = "$TOKEN_ESC"

[[sources]]
name  = "$SOURCE_NAME_ESC"
paths = [$PATHS_TOML]
# base_url = ""   # Optional: public URL prefix for file links in search results

[scan]
# max_file_size_mb = 10   # Skip files larger than this (MB)
# max_line_length  = 120    # Wrap long lines at this column (0 = disable)
# follow_symlinks  = false
# include_hidden   = false  # Index dot-files and dot-directories
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
  # systemd user session is active — install as a user service
  SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
  SERVICE_FILE="$SYSTEMD_USER_DIR/find-watch.service"
  mkdir -p "$SYSTEMD_USER_DIR"

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

elif command -v systemctl >/dev/null 2>&1 && [ -d "/run/systemd/system" ]; then
  # systemd is present but user session is unavailable (e.g. Synology DSM).
  # Write the unit file to a temp location and instruct the user to install it.
  UNIT_STAGING="$HOME/.config/find-anything/find-watch.service"
  cat > "$UNIT_STAGING" <<EOF
[Unit]
Description=find-anything file watcher
After=network.target

[Service]
User=$(id -un)
ExecStart=${INSTALL_DIR}/find-watch --config ${CONFIG_FILE}
Restart=on-failure
RestartSec=5s
Environment=RUST_LOG=find_watch=info
Environment=PATH=${INSTALL_DIR}:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=multi-user.target
EOF

  echo ""
  echo "systemd user sessions are not supported on this system (e.g. Synology DSM)."
  echo "A system-level service unit has been written to:"
  echo "  $UNIT_STAGING"
  echo ""
  echo "To install and enable it, run:"
  echo ""
  echo "  sudo mv $UNIT_STAGING /etc/systemd/system/find-watch.service"
  echo "  sudo systemctl daemon-reload"
  echo "  sudo systemctl enable find-watch"
  echo "  sudo systemctl start find-watch"

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
  # No systemd at all
  echo ""
  echo "Autostart not configured (systemd not detected)."
  echo "Start find-watch manually:"
  echo ""
  echo "  ${INSTALL_DIR}/find-watch --config ${CONFIG_FILE}"
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
echo "If upgrading, restart the watcher to pick up the new binary:"
echo ""
print_restart_cmd
echo ""
echo "First-time setup — run the initial scan:"
echo ""
echo "  find-scan --config $CONFIG_FILE --full"
echo ""
echo "This indexes all configured directories. Run it once before"
echo "find-watch will have anything useful to keep up to date."
