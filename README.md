<h1 align="center">
  My Awesome Scripts
</h1>

<p align="center">
  A curated collection of shell scripts and system utilities for AWS management, Docker operations, development automation, system administration, network monitoring, backup, and virtualization.
</p>

<p align="center">
  <a href="#modules">Modules</a> &bull;
  <a href="#code-structure">Code structure</a> &bull;
  <a href="#code-design">Code design</a> &bull;
  <a href="#installing-the-application">Installing the application</a> &bull;
  <a href="#environments">Environments</a> &bull;
  <a href="#running-the-application">Running the application</a> &bull;
  <a href="#changelog">Changelog</a>
</p>

---

# Modules

| Module | Description |
| --- | --- |
| **aws/** | AWS management tools &mdash; EC2 instance lifecycle and S3 bucket mounting |
| **backup/** | Backup and file management &mdash; smart backups, file organizer, duplicate finder |
| **development/** | Development automation &mdash; git cleanup, project init, code stats, LLM-powered commits |
| **docker/** | Docker utilities &mdash; ECR image cleanup |
| **inactivity-shutdown/** | Systemd service for automatic shutdown after user inactivity |
| **network/** | Network utilities &mdash; port scanner, network info, ping monitor |
| **python/** | Python tooling &mdash; UV package manager wrapper |
| **system/** | System administration &mdash; system info, cleanup, resource monitor |
| **virtualization/** | VM and container provisioning &mdash; Lima config, LXD setup scripts |
| **agents/** | Claude Code agent definitions for automated commits, PRs, changelogs, and more |

# Code structure

```
myawesomescripts/
├── aws/
│   ├── awsx                           # EC2 instance management (start/stop/switch/ssh)
│   └── s3/
│       └── mount_s3                   # Mount S3 buckets via rclone
├── backup/
│   ├── duplicate-finder               # Find and remove duplicate files by hash
│   ├── file-organizer                 # Organize files by type, date, or size
│   └── smart-backup                   # Archive, incremental, and sync backups
├── development/
│   ├── code-stats                     # Repository analysis and statistics
│   ├── git-cleanup                    # Git maintenance and branch cleanup
│   ├── gitit                          # LLM-powered commit messages and PR drafts
│   └── project-init                   # Scaffold new projects (Python, Node, Go, Rust)
├── docker/
│   └── remove_project_images.sh       # Interactive ECR image removal
├── inactivity-shutdown/
│   ├── inactivity-shutdown.sh         # Inactivity detection script
│   ├── inactivity-shutdown.service    # Systemd service unit
│   └── inactivity-shutdown.timer      # Systemd timer (every 5 min)
├── network/
│   ├── network-info                   # Network interfaces, connectivity, speed test
│   ├── ping-monitor                   # Continuous multi-host ping monitoring
│   └── port-scanner                   # TCP/UDP port scanning with service detection
├── python/
│   └── uvx                            # System-wide Python package install via UV
├── system/
│   ├── cleanup                        # Temp files, caches, logs, Docker cleanup
│   ├── monitor                        # Real-time CPU/memory/disk/load monitoring
│   └── system-info                    # Hardware, network, and process overview
├── virtualization/
│   ├── lima.yaml                      # Lima VM config (Ubuntu 24.04 + Docker)
│   └── lxd/
│       └── sbin/
│           ├── gh_auth                # GitHub CLI authentication helper
│           ├── gh_ssh                 # GitHub SSH key registration
│           ├── mc_alias               # MinIO client alias setup
│           └── mount_s3fs             # S3FS bucket mounting
├── agents/
│   └── commands/                      # Claude Code agent definitions
├── .gitignore
└── README.md
```

# Code design

All scripts in this repository follow a consistent set of conventions:

- **Portable Bash** &mdash; Scripts target `#!/bin/bash` and rely on common Unix utilities (`curl`, `jq`, `rsync`, `tar`, etc.). No exotic interpreters are required.
- **Colored output** &mdash; Every interactive script defines ANSI color variables (`RED`, `GREEN`, `YELLOW`, etc.) and resets with `NC` for readable terminal output.
- **Fail-fast with `set -e`** &mdash; Scripts exit immediately on errors to prevent cascading failures.
- **Dry-run support** &mdash; Destructive operations (backups, file moves, deletions, git cleanup) offer a `--dry-run` / `-n` flag so users can preview changes safely.
- **Interactive confirmation** &mdash; Scripts that delete data prompt for confirmation before proceeding (e.g., `remove_project_images.sh`, `duplicate-finder`, `git-cleanup`).
- **Self-contained** &mdash; Each script is a standalone file with built-in usage/help text (`-h` / `--help`). No shared libraries or sourced dependencies between modules.
- **Environment-driven configuration** &mdash; External configuration (API keys, instance names, thresholds) is passed via environment variables rather than hardcoded values.

# Installing the application

### Prerequisites

- **Bash 4+** (for associative arrays used in several scripts)
- **Git** (for development tools)
- **AWS CLI** (for `aws/` scripts)
- **jq** (JSON parsing in AWS and gitit scripts)
- **curl** (used across network and development scripts)
- **rsync** (for smart-backup sync and incremental modes)
- **Docker** (for `docker/` scripts)

### Setup

```bash
# Clone the repository
git clone <repository-url> myawesomescripts
cd myawesomescripts

# Make all scripts executable
find . -type f \( -name "*.sh" -o -path "*/aws/*" -o -path "*/development/*" \
  -o -path "*/network/*" -o -path "*/backup/*" -o -path "*/python/*" \
  -o -path "*/system/*" \) -exec chmod +x {} \;

# Install common dependencies (Debian/Ubuntu)
sudo apt-get install jq curl wget rsync netcat-openbsd bc tree
```

### Optional dependencies

| Dependency | Required by |
| --- | --- |
| `uv` (Python) | `python/uvx` |
| `rclone` | `aws/s3/mount_s3` |
| `s3fs` | `virtualization/lxd/sbin/mount_s3fs` |
| `mc` (MinIO Client) | `virtualization/lxd/sbin/mc_alias` |
| `gh` (GitHub CLI) | `virtualization/lxd/sbin/gh_auth`, `gh_ssh` |
| `wireless-tools` | `network/network-info` (WiFi details) |
| `beep` / `speaker-test` | `network/ping-monitor` (sound alerts) |

# Environments

The following environment variables are used across scripts:

| Variable | Used by | Description |
| --- | --- | --- |
| `INSTANCE_NAME` | `aws/awsx` | Name tag of the target EC2 instance |
| `ECR_PREFIX` | `docker/remove_project_images.sh` | ECR repository prefix to filter images |
| `LLM_PROVIDER` | `development/gitit` | LLM provider: `nvidia` (default) or `openai` |
| `LLM_API_KEY` | `development/gitit` | API key for the LLM provider |
| `LLM_MODEL` | `development/gitit` | Model name (optional, uses provider default) |
| `LLM_API_URL` | `development/gitit` | Custom API endpoint (optional) |
| `NVIDIA_API_KEY` | `development/gitit` | Fallback API key for NVIDIA provider |
| `OPENAI_API_KEY` | `development/gitit` | Fallback API key for OpenAI provider |
| `GITHUB_TOKEN` | `virtualization/lxd/sbin/gh_auth` | GitHub personal access token |
| `BUCKET_NAME` | `virtualization/lxd/sbin/mount_s3fs` | S3 bucket name to mount |
| `API_STORAGE` | `virtualization/lxd/sbin/mount_s3fs`, `mc_alias` | S3-compatible storage endpoint URL |
| `AWS_ACCESS_KEY_ID` | `virtualization/lxd/sbin/mount_s3fs`, `mc_alias` | AWS/S3 access key |
| `AWS_SECRET_ACCESS_KEY` | `virtualization/lxd/sbin/mount_s3fs`, `mc_alias` | AWS/S3 secret key |
| `MC_ALIAS` | `virtualization/lxd/sbin/mc_alias` | MinIO alias name (default: `pvt`) |
| `CPU_THRESHOLD` | `system/monitor` | CPU alert threshold percentage (default: 80) |
| `MEMORY_THRESHOLD` | `system/monitor` | Memory alert threshold percentage (default: 85) |
| `DISK_THRESHOLD` | `system/monitor` | Disk alert threshold percentage (default: 90) |
| `LOAD_THRESHOLD` | `system/monitor` | Load average alert threshold (default: 4.0) |
| `INACTIVITY_THRESHOLD` | `inactivity-shutdown/inactivity-shutdown.sh` | Seconds before shutdown (default: 3600) |

# Running the application

### AWS

```bash
# Manage EC2 instances
export INSTANCE_NAME="my-instance"
./aws/awsx status
./aws/awsx start
./aws/awsx switch gpu
```

### Backup

```bash
# Archive backup with compression
./backup/smart-backup -s /home/user -d /backup/home -c xz

# Organize files by type
./backup/file-organizer -s ~/Downloads

# Find duplicate files
./backup/duplicate-finder /home/user/Pictures
```

### Development

```bash
# Generate a commit message from staged changes
./development/gitit

# Generate a PR draft
./development/gitit --pr

# Clean merged git branches
./development/git-cleanup -b -o

# Initialize a new Python project
./development/project-init my-app python

# Analyze repository stats
./development/code-stats -a -t
```

### Docker

```bash
export ECR_PREFIX="my-ecr-prefix"
./docker/remove_project_images.sh
```

### Network

```bash
# Display network information
./network/network-info

# Monitor connectivity
./network/ping-monitor -i 2 google.com github.com

# Scan ports
./network/port-scanner -h example.com -p common
```

### System

```bash
# System overview
./system/system-info

# Cleanup temp files and caches
./system/cleanup          # user-level
sudo ./system/cleanup     # system-wide

# Monitor resources
./system/monitor --once
./system/monitor -c 90 -m 95 -i 10
```

### Python

```bash
./python/uvx requests pandas numpy
```

### Inactivity shutdown

```bash
# Install as a systemd service
sudo cp inactivity-shutdown/inactivity-shutdown.sh /usr/local/bin/
sudo chmod +x /usr/local/bin/inactivity-shutdown.sh
sudo cp inactivity-shutdown/inactivity-shutdown.service /etc/systemd/system/
sudo cp inactivity-shutdown/inactivity-shutdown.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now inactivity-shutdown.timer
```

# Changelog

See [CHANGELOG.md](CHANGELOG.md) for a detailed list of changes.
