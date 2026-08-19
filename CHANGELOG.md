# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- awsx2 persistent, self-healing tunnels: `tunnel --keep-alive`, `tunnel-up`, `tunnel-list`, `tunnel-rm`, and `tunnel-install`/`tunnel-uninstall` (macOS LaunchAgent). A supervisor auto-reconnects dropped SSM tunnels; desired state persists in `~/.config/awsx2/tunnels.json`. `tunnel-stop` now also stops the supervisor (`aws/awsx2`)
- `/review-pr-queue` Claude Code agent: works the cross-repo `review-requested:@me` backlog end to end — discovers PRs, labels and assigns them, reviews each diff with findings verified against the repository, and posts one review per PR (`agents/commands/review-pr-queue.md`)
- Claude Code agents for automated commit, PR, changelog, readme, label, link-issue, taskfile, and Snyk PR management (`agents/commands/`)
- `gitit` auto-commit script with LLM-powered commit message and PR draft generation (`development/gitit`)
- S3 bucket mounting via rclone (`aws/s3/mount_s3`)
- MinIO client alias helper (`virtualization/lxd/sbin/mc_alias`)
- LXD provisioning scripts for GitHub auth, SSH keys, and S3FS mounts (`virtualization/lxd/sbin/`)
- Lima VM configuration for Ubuntu 24.04 with Docker and dev tooling (`virtualization/lima.yaml`)
- Inactivity auto-shutdown systemd service (`inactivity-shutdown/`)
- EC2 instance management script with start/stop/switch/SSH config (`aws/awsx`)
- Docker ECR image cleanup script (`docker/remove_project_images.sh`)
- UV package manager wrapper for system-wide Python installs (`python/uvx`)
- System administration utilities: `system-info`, `cleanup`, `monitor` (`system/`)
- Development tools: `git-cleanup`, `project-init`, `code-stats` (`development/`)
- Network utilities: `port-scanner`, `network-info`, `ping-monitor` (`network/`)
- Backup tools: `smart-backup`, `file-organizer`, `duplicate-finder` (`backup/`)

### Fixed
- Script fixes and updates across multiple utilities
