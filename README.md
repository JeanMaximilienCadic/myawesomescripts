# My Awesome Scripts Collection 🚀

A curated collection of **15+ useful shell scripts** and system utilities for AWS management, Docker operations, Python development, system administration, network monitoring, backup automation, and more.

## 📁 Repository Structure

```
myawesomescripts/
├── aws/                        # AWS management tools
│   └── awsx                     # EC2 instance management script
├── backup/                     # Backup and file management utilities
│   ├── smart-backup             # Intelligent backup with compression & rotation
│   ├── file-organizer           # Automatic file organization by type/date/size
│   └── duplicate-finder         # Find and remove duplicate files
├── development/                # Development and automation tools
│   ├── git-cleanup              # Git repository maintenance and cleanup
│   ├── project-init             # Quick project initialization for various languages
│   └── code-stats               # Code repository analysis and statistics
├── docker/                     # Docker utilities
│   └── remove_project_images.sh # ECR image cleanup tool
├── network/                    # Network utilities and monitoring
│   ├── port-scanner             # Network port scanning tool
│   ├── network-info             # Comprehensive network information display
│   └── ping-monitor             # Continuous network connectivity monitoring
├── python/                     # Python development tools
│   └── uvx                      # UV package manager wrapper
├── system/                     # System administration utilities
│   ├── system-info              # Comprehensive system information display
│   ├── cleanup                  # System cleanup and maintenance
│   └── monitor                  # Real-time system resource monitoring
├── inactivity-shutdown/        # System service for auto-shutdown
│   ├── inactivity-shutdown.sh   # Main inactivity detection script
│   ├── inactivity-shutdown.service # Systemd service file
│   └── inactivity-shutdown.timer   # Systemd timer for periodic checks
└── README.md                   # This file
```

## 🔧 Scripts Overview

### AWS Tools (`aws/`)

#### `awsx` - EC2 Instance Management
A comprehensive script to manage EC2 instances with support for starting, stopping, type switching, and SSH configuration updates.

**Features:**
- 🟢 Start/stop EC2 instances
- 🔴 Force stop with warning
- 🔄 Switch between GPU and CPU instance types
- 🔧 Automatic SSH config updates
- 📊 Instance status monitoring
- 📋 List all stopped instances

**Prerequisites:**
- AWS CLI configured
- `jq` installed for JSON parsing
- `INSTANCE_NAME` environment variable set

**Usage:**
```bash
export INSTANCE_NAME="your-ec2-instance-name"
./aws/awsx {list|start|stop|force-stop|switch <gpu|cpu>|update-ssh|status}
```

**Examples:**
```bash
# Check instance status
./aws/awsx status

# Start instance and update SSH config
./aws/awsx start

# Switch to GPU instance type
./aws/awsx switch gpu

# List all stopped instances
./aws/awsx list
```

### Docker Tools (`docker/`)

#### `remove_project_images.sh` - ECR Image Cleanup
Interactive script to safely remove Docker images from ECR repositories one project at a time.

**Features:**
- 🔍 Searches for images by ECR prefix
- 📋 Shows detailed image information
- ⚠️  Interactive confirmation for each project
- 🧹 Automatic system cleanup after removal
- 📊 Progress reporting and verification

**Prerequisites:**
- Docker installed and running
- `ECR_PREFIX` environment variable set

**Usage:**
```bash
export ECR_PREFIX="your-ecr-prefix"
./docker/remove_project_images.sh
```

### Python Tools (`python/`)

#### `uvx` - UV Package Manager Wrapper
Convenient wrapper around the UV package manager for system-wide Python package installation.

**Features:**
- 🐍 System-wide package installation
- 🔓 Bypasses system package protection
- 📦 Supports multiple packages at once
- ⚡ Fast installation with UV

**Prerequisites:**
- UV package manager installed
- sudo privileges

**Usage:**
```bash
./python/uvx <package_name> [additional_packages...]
```

**Examples:**
```bash
# Install single package
./python/uvx requests

# Install multiple packages
./python/uvx pandas numpy matplotlib
```

### System Administration (`system/`)

#### `system-info` - System Information Display
Displays comprehensive system information including hardware, network, processes, and resource usage.

**Features:**
- 🖥️ Complete system overview (OS, kernel, uptime)
- ⚡ CPU information and current usage
- 💾 Memory statistics and availability
- 💿 Disk usage for all mounted filesystems
- 🌐 Network interface configuration and public IP
- 👥 Currently logged in users
- 🔥 Top processes by CPU and memory usage
- 📊 System load averages

**Usage:**
```bash
./system/system-info
```

#### `cleanup` - System Cleanup Tool
Comprehensive system cleanup utility that frees disk space by removing temporary files, caches, and old logs.

**Features:**
- 🧹 Removes temporary files and caches
- 📦 Cleans package manager caches (APT, DNF, YUM)
- 🌐 Clears browser caches (Chrome, Firefox, Chromium)
- 👨‍💻 Cleans development tool caches (NPM, Yarn, pip, Cargo)
- 📝 Manages log file rotation
- 🐳 Docker system cleanup
- 📱 Flatpak and Snap maintenance
- 🔍 Before/after disk usage comparison

**Usage:**
```bash
# User-level cleanup
./system/cleanup

# System-wide cleanup (requires sudo)
sudo ./system/cleanup
```

#### `monitor` - System Resource Monitor
Real-time system resource monitoring with configurable thresholds and alerts.

**Features:**
- 📊 Monitors CPU, memory, disk, and load average
- 🚨 Configurable alert thresholds
- 📈 Shows top resource-consuming processes
- 🔔 Optional system notifications and logging
- ⏱️ Configurable monitoring intervals
- 🎯 Single-run or continuous monitoring modes

**Usage:**
```bash
# Continuous monitoring with default thresholds
./system/monitor

# Custom thresholds and interval
./system/monitor -c 90 -m 95 -d 85 -i 10

# Single check with verbose output
./system/monitor --once -v
```

### Development Tools (`development/`)

#### `git-cleanup` - Git Repository Maintenance
Comprehensive Git repository cleanup and optimization tool.

**Features:**
- 🌿 Removes merged branches safely
- 🗑️ Cleans stash entries
- 🏷️ Manages old tags
- 🌐 Prunes remote tracking branches
- ⚡ Repository optimization and garbage collection
- 🔍 Interactive mode for selective cleanup
- 📊 Repository size reporting
- 🏃‍♂️ Dry-run mode for safe testing

**Usage:**
```bash
# Clean merged branches and optimize
./development/git-cleanup -b -o

# Interactive cleanup with all options
./development/git-cleanup -a -i

# Dry run to see what would be cleaned
./development/git-cleanup -a -n
```

#### `project-init` - Project Initialization Tool
Quickly sets up new projects with proper structure and configurations for various programming languages.

**Features:**
- 🐍 Python projects with virtual environments
- 📦 Node.js and React applications
- 🐹 Go projects with modules
- 🦀 Rust projects with Cargo
- 📁 Generic project templates
- 🔧 Automatic .gitignore creation
- 📄 License and README generation
- 🚀 Initial git commit setup

**Usage:**
```bash
# Create a Python project
./development/project-init my-python-app python

# Create a React application
./development/project-init my-web-app react

# Create a Go project
./development/project-init my-tool go
```

#### `code-stats` - Code Repository Analyzer
Analyzes code repositories and provides detailed statistics about codebase composition and development activity.

**Features:**
- 📊 File type analysis and line counts
- 📈 Git repository statistics (commits, contributors, activity)
- 👥 Author contribution analysis
- 📅 Commit timeline and activity patterns
- 🔧 Code complexity indicators
- 📋 Project size assessment
- 💡 Development insights and recommendations

**Usage:**
```bash
# Basic analysis
./development/code-stats

# Detailed analysis with author and timeline data
./development/code-stats -a -t -d /path/to/repo
```

### Network Utilities (`network/`)

#### `port-scanner` - Network Port Scanner
Fast and flexible port scanning tool with multiple scanning modes and comprehensive reporting.

**Features:**
- 🔍 TCP and UDP port scanning
- 📋 Predefined port lists (common, web, database)
- 🎯 Custom port ranges and specific ports
- ⚡ Multi-threaded scanning for speed
- 🔧 Configurable timeouts and concurrency
- 📊 Service identification for common ports
- 💾 Results export to file
- 🏃‍♂️ Dry-run mode for testing

**Usage:**
```bash
# Scan common ports
./network/port-scanner -h google.com -p common

# Custom port range with fast scan
./network/port-scanner -h 192.168.1.1 -p 1-1000 -t 1 -j 20

# Scan specific services with verbose output
./network/port-scanner -h example.com -p web -v
```

#### `network-info` - Network Information Display
Comprehensive network configuration and connectivity information tool.

**Features:**
- 🌐 Network interface details (IP, MAC, status)
- 🔧 Routing table and gateway information
- 📡 Wireless network information
- 🔗 Active network connections
- 🚀 Basic speed testing
- 🛠️ Available network tools detection
- 💡 Troubleshooting tips and suggestions

**Usage:**
```bash
./network/network-info
```

#### `ping-monitor` - Network Connectivity Monitor
Continuous network connectivity monitoring with statistics and alerting.

**Features:**
- 🎯 Multi-host monitoring
- 📊 Success rate and latency statistics
- 🚨 Configurable failure thresholds and alerts
- 🔔 Sound notifications (optional)
- 📝 Logging to file
- 📈 Real-time status display
- 🤫 Quiet mode for failure-only reporting

**Usage:**
```bash
# Monitor default hosts (Google DNS, Cloudflare, google.com)
./network/ping-monitor

# Custom hosts with alerts
./network/ping-monitor -i 2 -t 5 -s google.com github.com

# Log monitoring with sound alerts
./network/ping-monitor -l ping.log -s 192.168.1.1
```

### Backup & File Management (`backup/`)

#### `smart-backup` - Intelligent Backup Tool
Advanced backup solution with compression, rotation, and verification capabilities.

**Features:**
- 📦 Multiple backup modes (archive, incremental, sync)
- 🗜️ Compression options (gzip, bzip2, xz, none)
- 🔄 Automatic backup rotation and cleanup
- ✅ Backup integrity verification
- 📋 Exclude patterns and filtering
- 📝 Comprehensive logging
- 🏃‍♂️ Dry-run mode for testing
- 💾 Space-efficient incremental backups

**Usage:**
```bash
# Simple archive backup
./backup/smart-backup -s /home/user -d /backup/home

# Incremental backup with compression
./backup/smart-backup -s /data -d /backup/data -i -c xz -r 7

# Sync mode with exclusions
./backup/smart-backup -s /var/www -d /backup/web -y -e '*.log' -e 'tmp/*'
```

#### `file-organizer` - Automatic File Organizer
Organizes files automatically by type, date, or size with customizable rules.

**Features:**
- 📁 Organization by file type, date, or size
- 🔄 Recursive directory processing
- 📋 Copy or move modes
- 🏃‍♂️ Dry-run for safe testing
- 📝 Operation logging
- 🔙 Undo script generation
- 📊 Organization statistics
- 🎯 Customizable file type mappings

**Usage:**
```bash
# Organize by file type
./backup/file-organizer -s /home/user/Downloads

# Organize by date recursively
./backup/file-organizer -s /photos -o date -r

# Copy files organized by size with logging
./backup/file-organizer -s /data -o size -c -l organize.log
```

#### `duplicate-finder` - Duplicate File Detector
Finds and manages duplicate files using content-based hash comparison.

**Features:**
- 🔍 Content-based duplicate detection (MD5, SHA1, SHA256)
- 📏 Size-based filtering (min/max file sizes)
- 🎯 Include/exclude pattern matching
- 🗑️ Safe duplicate removal with confirmation
- 💬 Interactive mode for selective handling
- 📊 Space usage analysis and reporting
- 📝 Results export to file
- 🏃‍♂️ Dry-run mode for testing

**Usage:**
```bash
# Find duplicates in directory
./backup/duplicate-finder /home/user/Pictures

# Advanced filtering with size limits
./backup/duplicate-finder -s 1M -a sha256 /data /backup

# Interactive removal with pattern filtering
./backup/duplicate-finder --include '*.jpg' -i -d /photos
```

### System Services (`inactivity-shutdown/`)

#### Inactivity Auto-Shutdown Service
A systemd service that automatically shuts down the system after a period of user inactivity.

**Features:**
- 🕐 Configurable inactivity threshold (default: 1 hour)
- 👤 Detects SSH sessions, GUI activity, and running applications
- ⚠️  60-second warning before shutdown
- 📝 Comprehensive logging
- 🔄 Periodic checks every 5 minutes

**Components:**
- `inactivity-shutdown.sh` - Main detection script
- `inactivity-shutdown.service` - Systemd service definition
- `inactivity-shutdown.timer` - Timer for periodic execution

**Installation:**
```bash
# Copy script to system location
sudo cp inactivity-shutdown/inactivity-shutdown.sh /usr/local/bin/
sudo chmod +x /usr/local/bin/inactivity-shutdown.sh

# Install systemd service and timer
sudo cp inactivity-shutdown/inactivity-shutdown.service /etc/systemd/system/
sudo cp inactivity-shutdown/inactivity-shutdown.timer /etc/systemd/system/

# Enable and start the service
sudo systemctl daemon-reload
sudo systemctl enable inactivity-shutdown.timer
sudo systemctl start inactivity-shutdown.timer
```

**Configuration:**
Edit `/usr/local/bin/inactivity-shutdown.sh` to modify:
- `INACTIVITY_THRESHOLD`: Time in seconds before shutdown (default: 3600)
- Activity detection methods and sensitivity

## 🚀 Quick Start

1. **Clone or download this repository:**
   ```bash
   git clone <repository-url> myawesomescripts
   cd myawesomescripts
   ```

2. **Make scripts executable:**
   ```bash
   find . -name "*.sh" -exec chmod +x {} \;
   chmod +x aws/* development/* network/* backup/* python/* system/*
   ```

3. **Set up environment variables as needed:**
   ```bash
   export INSTANCE_NAME="your-ec2-instance"
   export ECR_PREFIX="your-ecr-prefix"
   ```

4. **Install system dependencies:**
   ```bash
   # Essential tools for most scripts
   sudo apt-get install jq curl wget rsync netcat-openbsd bc tree

   # For specific functionality
   pip install uv                    # For uvx (Python package manager)
   sudo apt-get install wireless-tools  # For WiFi info in network-info
   sudo apt-get install beep            # For sound alerts in ping-monitor
   ```

## 🔒 Security Considerations

- **AWS Scripts**: Ensure AWS credentials are properly configured with least-privilege access
- **Docker Scripts**: Review ECR prefixes carefully to avoid unintended image deletion
- **Python Scripts**: UV wrapper uses sudo - review packages before installation
- **System Services**: Inactivity shutdown runs as root - review script before installation

## 🤝 Contributing

Feel free to submit issues, feature requests, or pull requests to improve these scripts!

## 📄 License

These scripts are provided as-is for educational and practical use. Please review and test thoroughly before using in production environments.

---

**Happy Scripting! 🎉**
