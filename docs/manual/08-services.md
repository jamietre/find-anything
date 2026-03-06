# Running as a Service

[← Manual home](README.md)

---

## systemd (Linux)

### Server service

The server install script (`install-server.sh`) creates and enables a systemd service automatically. To manage it manually:

```sh
# Status
systemctl status find-server

# Start / stop / restart
systemctl start find-server
systemctl stop find-server
systemctl restart find-server

# View logs (follow)
journalctl -u find-server -f

# View last 100 lines
journalctl -u find-server -n 100
```

**Service file location:** `/etc/systemd/system/find-server.service` (system install) or `~/.config/systemd/user/find-server.service` (user install).

Example service file (system install):

```ini
[Unit]
Description=find-anything server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/find-server --config /etc/find-anything/server.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Client watcher service (find-watch)

The client install script (`install.sh`) creates a **user** systemd service for `find-watch`:

```sh
# Status
systemctl --user status find-watch

# Start / stop / restart
systemctl --user start find-watch
systemctl --user stop find-watch
systemctl --user restart find-watch

# View logs
journalctl --user -u find-watch -f
```

**Service file location:** `~/.config/systemd/user/find-watch.service`

Example service file:

```ini
[Unit]
Description=find-anything file watcher
After=network.target

[Service]
Type=simple
ExecStart=%h/.local/bin/find-watch
Restart=on-failure
RestartSec=5
Environment=FIND_ANYTHING_CONFIG=%h/.config/find-anything/client.toml

[Install]
WantedBy=default.target
```

**Note:** `find-watch` does not perform an initial scan on startup. Run `find-scan` at least once before starting `find-watch`. If `find-watch` was offline for a period, run `find-scan` again to catch up on missed changes, then restart `find-watch`.

### Enabling linger (user services on headless machines)

By default, user systemd services only run while the user is logged in. On headless servers, enable lingering so `find-watch` runs even when you are not logged in:

```sh
loginctl enable-linger $USER
```

---

## Windows service

The Windows installer registers `find-watch` as a Windows service that starts automatically at login. To manage it:

**PowerShell (as Administrator):**

```powershell
# Status
Get-Service find-watch

# Start / stop
Start-Service find-watch
Stop-Service find-watch

# Restart
Restart-Service find-watch

# View recent events (Event Viewer alternative)
Get-EventLog -LogName Application -Source find-watch -Newest 20
```

**Service configuration:** `%APPDATA%\find-anything\client.toml`

The initial scan runs automatically during installation. To re-scan manually, open a terminal and run:

```cmd
find-scan
```

---

## Docker

For running `find-server` in a container:

```sh
git clone https://github.com/jamietre/find-anything
cd find-anything
cp examples/server.toml server.toml
# Edit server.toml: set a strong token, data_dir = /data
docker compose up -d
```

**`docker-compose.yml` (excerpt):**

```yaml
services:
  find-server:
    image: ghcr.io/jamietre/find-anything:latest
    ports:
      - "8765:8765"
    volumes:
      - ./server.toml:/etc/find-anything/server.toml:ro
      - find-data:/data
    restart: unless-stopped

volumes:
  find-data:
```

**Pinning a version:**

```yaml
image: ghcr.io/jamietre/find-anything:v0.2.4
```

**Viewing logs:**

```sh
docker compose logs -f find-server
```

**Restarting:**

```sh
docker compose restart find-server
```

**The client tools** (`find-scan`, `find-watch`) run on client machines outside the container and connect to the server over the network. Only the server runs in Docker.

---

[← Administration](07-administration.md) | [Next: Troubleshooting →](09-troubleshooting.md)
