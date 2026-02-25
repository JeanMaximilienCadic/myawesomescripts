# awsx2

A fast AWS management CLI and interactive TUI built in Rust. Manages EC2 instances, SSM tunnels, local reverse proxies, and AWS Client VPN with SAML authentication through a single tool.

```
awsx2          # launch interactive TUI
awsx2 list     # CLI mode — list all instances
```

## Features

- **Dual-mode** — full-screen TUI for interactive use, CLI for scripts and automation
- **EC2 management** — list, start, stop, force-stop, switch instance types (GPU/CPU)
- **Smart tunneling** — SSM port-forwarding with ALB-aware routing, security group analysis, and bastion fallback
- **Client VPN** — AWS Client VPN with SAML/SSO authentication, headless browser MFA, and automatic DNS configuration
- **Reverse proxy** — auto-configures nginx + `/etc/hosts` so internal URLs work directly in the browser
- **Cross-platform** — macOS (Homebrew) and Linux (Debian/Ubuntu, RHEL/CentOS, Amazon Linux)

## Prerequisites

- [AWS CLI v2](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) with SSO configured
- [Session Manager Plugin](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)
- SSM Agent running on target EC2 instances
- nginx (only for `--proxy` feature)
- Chromium/Chrome (only for `vpn connect` — headless SAML auth)
- AWS VPN Client or OpenVPN (only for `vpn connect`)

## Installation

```bash
cargo build --release
cp target/release/awsx2 /usr/local/bin/
```

## CLI Usage

Run `awsx2 <command>`. Instance commands accept `--name` or read from the `INSTANCE_NAME` environment variable.

### Instance Management

```bash
awsx2 list                          # List all EC2 instances
awsx2 status --name my-server       # Show instance details
awsx2 start --name my-server        # Start an instance
awsx2 stop --name my-server         # Graceful stop
awsx2 force-stop --name my-server   # Force stop (immediate)
awsx2 switch gpu --name my-server   # Switch to g4dn.4xlarge
awsx2 switch cpu --name my-server   # Switch to m6i.2xlarge
```

### Authentication

```bash
awsx2 login                # SSO login using $AWS_PROFILE
awsx2 login my-profile     # SSO login with explicit profile
```

### DNS Resolution

```bash
awsx2 resolve https://app.internal.example.com
```

Traces the full path: hostname &rarr; DNS &rarr; ALB &rarr; target group &rarr; EC2/Fargate backend.

### Tunnels

**Direct tunnel** to an EC2 instance by name pattern:

```bash
awsx2 tunnel web-server 8080 8000
#              ^pattern  ^local ^remote
```

**URL tunnel** with smart ALB resolution (auto-detects bastion and remote port):

```bash
awsx2 tunnel-url https://app.internal.example.com 8080
```

The resolution chain: URL &rarr; ALB match &rarr; healthy target group &rarr; security group rules &rarr; SSM-online hop instance.
Falls back to trying all available bastions if ALB resolution fails.

**URL tunnel with reverse proxy** so the URL works directly in the browser:

```bash
awsx2 tunnel-url https://app.internal.example.com 8080 --proxy
```

This additionally:
1. Writes an nginx config forwarding port 80 &rarr; localhost:8080
2. Adds `127.0.0.1 app.internal.example.com` to `/etc/hosts`
3. Reloads nginx and flushes DNS cache

**DNS tunnel** (resolve hostname, tunnel to the resolved IP):

```bash
awsx2 tunnel-dns https://app.internal.example.com 8080 8501
```

**Remote tunnel** via a specific bastion to an arbitrary host:

```bash
awsx2 tunnel-remote bastion 10.0.1.42 8080 8501
#                   ^bastion ^target   ^local ^remote
```

**Tunnel management:**

```bash
awsx2 tunnel-test 8080    # Check if port is open
awsx2 tunnel-stop         # Kill all SSM tunnels + clean up proxies
```

### VPN

Connect to AWS Client VPN endpoints that use SAML/SSO authentication. Credentials are saved locally so you only need to enter the MFA code each time.

```bash
# One-time setup — saves credentials to ~/.config/awsx2/vpn.json
awsx2 vpn setup \
  --username user@example.com \
  --password 'secret' \
  --ovpn /path/to/client.ovpn \
  --dns-server 10.0.0.2 \
  --dns-domain '~internal.example.com'

# Connect (prompts for MFA if not provided)
sudo -E awsx2 vpn connect 123456

# Check status
awsx2 vpn status

# Disconnect
sudo -E awsx2 vpn disconnect
```

The connect flow:
1. Sends initial auth to VPN server to obtain SAML challenge URL
2. Launches headless Chromium to complete SSO login (username, password, MFA)
3. Captures SAML response via local HTTP callback
4. Reconnects to VPN with the SAML token (uses AWS patched OpenVPN if available)
5. Configures DNS routing via `resolvectl` for the specified domain

Requires `sudo -E` to create the tun interface and configure DNS. The `-E` flag preserves your AWS environment variables.

## TUI

Launch with `awsx2` (no arguments). Navigate with keyboard — no mouse required.

### Global Keys

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch tabs |
| `?` | Toggle help overlay |
| `q` / `Ctrl+c` | Quit |

### Instances Tab

| Key | Action |
|-----|--------|
| `j` / `k` / `Up` / `Down` | Navigate |
| `g` / `G` | Jump to first / last |
| `/` | Filter by name, ID, or type |
| `Esc` | Clear filter |
| `s` | Start instance |
| `S` | Stop instance |
| `f` | Force-stop instance |
| `r` | Refresh |

Columns: Instance ID, Name, Type, State, SSM Status, Tunnel, Private IP.
States are color-coded: green = running, red = stopped, yellow = pending/stopping.

### Tunnels Tab

| Key | Action |
|-----|--------|
| `j` / `k` / `Up` / `Down` | Navigate |
| `n` | New tunnel by instance name (wizard) |
| `u` | New tunnel by URL (smart ALB resolution) |
| `b` | New tunnel via bastion (wizard) |
| `d` / `Delete` | Stop selected tunnel |
| `A` | Stop all tunnels |
| `r` | Refresh |

Each tunnel shows real-time status with latency measurement:
- `● OK 42ms` — tunnel active, measured round-trip
- `▲ OPEN` — port open, not yet probed
- `◌ DOWN` — tunnel unreachable

Tunnels auto-refresh every ~15 seconds.

### Tools Tab

| Key | Action |
|-----|--------|
| `j` / `k` / `Up` / `Down` | Navigate menu |
| `Enter` | Execute |

Available tools:
- **Switch Profile** — select from `~/.aws/config` profiles
- **Switch Region** — change AWS region
- **Login** — SSO login
- **Resolve URL** — trace DNS to backend resource
- **Test Port** — check if a tunnel port is open
- **Stop All Tunnels** — kill all SSM sessions

### VPN Tab

| Key | Action |
|-----|--------|
| `j` / `k` / `Up` / `Down` | Navigate menu |
| `Enter` | Execute |
| `r` | Refresh status |

Available actions:
- **Connect** — enter MFA code and connect to VPN
- **Disconnect** — stop active VPN session
- **Setup** — configure SSO credentials and .ovpn path (multi-step wizard)
- **Status** — check VPN connection state, IP, and PID

## Reverse Proxy

The `--proxy` flag on `tunnel-url` sets up nginx so the original hostname works in your browser over the SSM tunnel.

### How It Works

1. Writes a site config to nginx (`proxy_pass` to the tunnel's local port)
2. Adds a `/etc/hosts` entry pointing the hostname to `127.0.0.1`
3. Reloads nginx and flushes the DNS cache
4. `awsx2 tunnel-stop` cleans everything up automatically

### Platform Support

| | macOS (Homebrew) | Linux (Debian/Ubuntu) | Linux (RHEL/CentOS) |
|---|---|---|---|
| **Config path** | `/opt/homebrew/etc/nginx/servers/` | `sites-available/` + symlink to `sites-enabled/` | `/etc/nginx/conf.d/` |
| **Nginx reload** | `nginx -s reload` | `systemctl reload nginx` | `systemctl reload nginx` |
| **DNS flush** | `dscacheutil` + `mDNSResponder` | `resolvectl` / `systemd-resolve` | `resolvectl` / `nscd` |

### nginx Installation

```bash
# macOS
brew install nginx

# Debian / Ubuntu
sudo apt install nginx

# RHEL / CentOS / Amazon Linux
sudo yum install nginx
```

## Architecture

```
awsx2
├── main.rs          # Entry point, CLI (clap) + TUI event loop
├── aws.rs           # AWS CLI wrapper (EC2, SSM, ALB, SG, DNS)
├── tunnel.rs        # SSM tunnel lifecycle (start, detect, stop, probe)
├── proxy.rs         # nginx reverse proxy + /etc/hosts management
├── vpn.rs           # Client VPN with SAML auth (headless Chrome, config persistence)
├── models.rs        # Domain types (Instance, TunnelProcess, VpnConfig, etc.)
├── error.rs         # Error types (AppError enum with thiserror)
└── tui/
    ├── app.rs       # Application state, background task channels
    ├── ui.rs        # Layout, colors, popup rendering
    └── pages/
        ├── instances.rs   # Instances tab (table + key handlers)
        ├── tunnels.rs     # Tunnels tab (table + creation wizards)
        ├── tools.rs       # Tools tab (menu + actions)
        └── vpn.rs         # VPN tab (connect, disconnect, setup)
```

**Key design decisions:**
- Shells out to `aws` CLI rather than using the AWS SDK — leverages existing SSO/credential configuration with zero extra setup
- Tunnels are detached child processes, discovered by parsing `ps` output for `session-manager-plugin`
- TUI runs background operations on threads, communicates via `mpsc` channels
- No runtime dependencies beyond the AWS CLI and session manager plugin

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | Terminal UI framework |
| `crossterm` | Terminal I/O (raw mode, key events) |
| `clap` | CLI argument parsing with env var support |
| `serde` + `serde_json` | AWS CLI JSON output parsing |
| `thiserror` | Error type derivation |
| `libc` | Unix signal handling (SIGTERM for tunnel cleanup) |
| `headless_chrome` | Chrome DevTools Protocol for SAML browser automation |
| `tiny_http` | Lightweight HTTP server for SAML callback listener |
| `regex` | Parsing SAML URL and session ID from OpenVPN output |
| `url` | URL parsing for SAML form data extraction |
| `dirs` | Platform-correct config directory (`~/.config/awsx2/`) |
| `tempfile` | Secure temporary files for OpenVPN configs and credentials |

## Environment Variables

| Variable | Used by |
|----------|---------|
| `AWS_PROFILE` | Default profile for all AWS operations |
| `AWS_DEFAULT_REGION` | Default region |
| `INSTANCE_NAME` | Default instance name for CLI commands |

## License

MIT
