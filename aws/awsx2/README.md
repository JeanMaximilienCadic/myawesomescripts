# awsx2

A fast AWS management CLI and interactive TUI built in Rust. Manages EC2 instances, SSM tunnels, and local reverse proxies through a single tool.

```
awsx2          # launch interactive TUI
awsx2 list     # CLI mode — list all instances
```

## Features

- **Dual-mode** — full-screen TUI for interactive use, CLI for scripts and automation
- **EC2 management** — list, start, stop, force-stop, switch instance types (GPU/CPU)
- **Smart tunneling** — SSM port-forwarding with ALB-aware routing, security group analysis, and bastion fallback
- **Reverse proxy** — auto-configures nginx + `/etc/hosts` so internal URLs work directly in the browser
- **Cross-platform** — macOS (Homebrew) and Linux (Debian/Ubuntu, RHEL/CentOS, Amazon Linux)

## Prerequisites

- [AWS CLI v2](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html) with SSO configured
- [Session Manager Plugin](https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html)
- SSM Agent running on target EC2 instances
- nginx (only for `--proxy` feature)

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
├── models.rs        # Domain types (Instance, TunnelProcess, etc.)
├── error.rs         # Error types (AppError enum with thiserror)
└── tui/
    ├── app.rs       # Application state, background task channels
    ├── ui.rs        # Layout, colors, popup rendering
    └── pages/
        ├── instances.rs   # Instances tab (table + key handlers)
        ├── tunnels.rs     # Tunnels tab (table + creation wizards)
        └── tools.rs       # Tools tab (menu + actions)
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

## Environment Variables

| Variable | Used by |
|----------|---------|
| `AWS_PROFILE` | Default profile for all AWS operations |
| `AWS_DEFAULT_REGION` | Default region |
| `INSTANCE_NAME` | Default instance name for CLI commands |

## License

MIT
