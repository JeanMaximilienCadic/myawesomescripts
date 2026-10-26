#!/usr/bin/env bash
# devbox.sh — temporary, resource-capped LXD dev containers on the LAN for feature-from-zip.
# macOS bash 3.2 compatible. State per box lives in ~/.cache/ffz/<name>.env
#
#   devbox.sh pick [mem_gib]                 first reachable LAN host with lxc + enough free RAM
#   devbox.sh up <name> [cpu] [mem] [disk]   launch + wait for cloud-init (defaults 4 / 8GiB / 40GiB)
#   devbox.sh run <name> <cmd...>            run as foo in /home/foo/work/repo (bash -lc)
#   devbox.sh root <name> <cmd...>           run as root
#   devbox.sh sync <name> <local_repo_dir>   rsync Mac working copy -> box (respects .gitignore; FFZ_DIR=<dir> for a 2nd checkout)
#   devbox.sh pull <name> <remote_rel> <local_path>   copy a file/dir back from the box
#   devbox.sh tunnel <name> <port> [lport]   ssh -L 127.0.0.1:lport -> box:port (foreground; run in background)
#   devbox.sh ip <name> | info <name> | list | down <name>
set -euo pipefail

STATE_DIR="${HOME}/.cache/ffz"
HOSTS="${FFZ_HOSTS:-x399 i9 cloudcompute}"
IMAGE="${FFZ_IMAGE:-ubuntu:24.04}"
HERE="$(cd "$(dirname "$0")" && pwd)"
# Working copy inside the box, under /home/foo/work. FFZ_DIR=<name> keeps a
# second checkout (say, a bugfix branch) beside the main one.
DIR="${FFZ_DIR:-repo}"
mkdir -p "$STATE_DIR"

die() { echo "devbox: $*" >&2; exit 1; }

# Resolve lxc binary on a host (zk0node02/i9/cloudcompute only have the snap path).
lxc_bin() { ssh -o ConnectTimeout=6 "$1" 'command -v lxc || ls /snap/bin/lxc' 2>/dev/null | head -1; }

load() {
  local f="$STATE_DIR/$1.env"
  [ -f "$f" ] || die "no state for '$1' (expected $f)"
  # shellcheck disable=SC1090
  . "$f"
}

remote() { ssh -o ConnectTimeout=10 "$FFZ_HOST" "$@"; }

as_foo() {
  local name="$1"; shift
  local cmd="cd /home/foo/work/$DIR 2>/dev/null || cd /home/foo; $*"
  remote "$FFZ_LXC exec $name -- runuser -u foo -- env HOME=/home/foo USER=foo bash -lc $(printf %q "$cmd")"
}

cmd="${1:-}"; shift || true
case "$cmd" in
  pick)
    need_kb=$(( ${1:-8} * 1024 * 1024 + 4 * 1024 * 1024 ))   # cap + 4GiB headroom
    for h in $HOSTS; do
      bin=$(lxc_bin "$h") || continue
      [ -n "$bin" ] || continue
      avail=$(ssh -o ConnectTimeout=6 "$h" "awk '/MemAvailable/{print \$2}' /proc/meminfo" 2>/dev/null || echo 0)
      if [ "${avail:-0}" -ge "$need_kb" ]; then echo "$h"; exit 0; fi
      echo "devbox: $h has only $((avail/1024/1024))GiB available, skipping" >&2
    done
    die "no LAN host with enough free memory among: $HOSTS"
    ;;

  up)
    name="${1:?name}"; cpu="${2:-4}"; mem="${3:-8GiB}"; disk="${4:-40GiB}"
    FFZ_HOST="${FFZ_HOST:-$("$0" pick "$(echo "$mem" | tr -dc 0-9)")}"
    FFZ_LXC=$(lxc_bin "$FFZ_HOST"); [ -n "$FFZ_LXC" ] || die "lxc not found on $FFZ_HOST"
    remote "$FFZ_LXC launch $IMAGE $name \
      -c limits.cpu=$cpu -c limits.memory=$mem -c limits.memory.enforce=hard \
      -c limits.processes=8192 -c security.nesting=true \
      -c user.ffz=1 -c user.ffz.created=$(date +%Y-%m-%dT%H:%M:%S) \
      -d root,size=$disk"
    echo "devbox: waiting for cloud-init on $FFZ_HOST/$name (package upgrade, can take a few minutes)..." >&2
    remote "$FFZ_LXC exec $name -- cloud-init status --wait" >/dev/null || true
    remote "$FFZ_LXC exec $name -- bash -c 'command -v rsync >/dev/null || (apt-get update -qq && apt-get install -y -qq rsync); mkdir -p /home/foo/work/repo && chown -R foo:foo /home/foo/work'"
    ip=""
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      ip=$(remote "$FFZ_LXC list $name -f csv -c 4" | grep -oE '([0-9]+\.){3}[0-9]+ \(eth0\)' | cut -d' ' -f1 | head -1 || true)
      [ -n "$ip" ] && break; sleep 3
    done
    [ -n "$ip" ] || die "$name has no IPv4 (try: devbox.sh root $name dhcpcd -4 -t 15 eth0)"
    cat > "$STATE_DIR/$name.env" <<STATE
FFZ_HOST=$FFZ_HOST
FFZ_LXC=$FFZ_LXC
FFZ_NAME=$name
FFZ_IP=$ip
FFZ_LIMITS="cpu=$cpu mem=$mem disk=$disk"
STATE
    echo "devbox: $name up on $FFZ_HOST ip=$ip ($cpu cpu, $mem, $disk)"
    ;;

  run)  name="${1:?name}"; shift; load "$name"; as_foo "$name" "$@" ;;
  root) name="${1:?name}"; shift; load "$name"
        remote "$FFZ_LXC exec $name -- bash -lc $(printf %q "$*")" ;;

  sync)
    name="${1:?name}"; src="${2:?local repo dir}"; load "$name"
    [ -d "$src/.git" ] || [ -f "$src/.git" ] || die "$src is not a git working copy"
    # Global excludes as well as the .gitignore merge: --delete does not
    # honour per-directory merge rules on the receiving side, and wiped the
    # box's node_modules on the first resync. A worktree's .git is a file
    # naming a Mac path, useless in the box, so it is not copied either.
    FFZ_HOST="$FFZ_HOST" FFZ_LXC="$FFZ_LXC" FFZ_CT="$name" \
    rsync -az --delete --filter=':- .gitignore' --exclude=.DS_Store --exclude=.git \
      --exclude=node_modules/ --exclude=.venv/ --exclude=target/ --exclude=dist/ \
      --exclude=coverage/ --exclude=__pycache__/ --exclude=playwright-report/ --exclude=test-results/ \
      --exclude=.pytest_cache/ --exclude=.ruff_cache/ --exclude=.mypy_cache/ \
      -e "$HERE/lxc-rsh.sh" "${src%/}/" "box:/home/foo/work/$DIR/"
    ;;

  pull)
    name="${1:?name}"; rel="${2:?remote path relative to repo}"; dst="${3:?local path}"; load "$name"
    FFZ_HOST="$FFZ_HOST" FFZ_LXC="$FFZ_LXC" FFZ_CT="$name" \
    rsync -az -e "$HERE/lxc-rsh.sh" "box:/home/foo/work/$DIR/$rel" "$dst"
    ;;

  tunnel)
    name="${1:?name}"; port="${2:?port}"; lport="${3:-$port}"; load "$name"
    echo "devbox: http://localhost:$lport -> $FFZ_HOST -> $FFZ_IP:$port (Ctrl-C / TaskStop to close)" >&2
    exec ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 \
      -L "127.0.0.1:$lport:$FFZ_IP:$port" "$FFZ_HOST"
    ;;

  ip)   load "${1:?name}"; echo "$FFZ_IP" ;;
  info) load "${1:?name}"; cat "$STATE_DIR/$1.env" ;;

  list)
    for h in $HOSTS; do
      bin=$(lxc_bin "$h") || continue; [ -n "$bin" ] || continue
      ssh -o ConnectTimeout=6 "$h" "$bin list user.ffz=1 -f csv -c ns" 2>/dev/null | sed "s/^/$h,/"
    done
    ;;

  down)
    name="${1:?name}"; load "$name"
    remote "$FFZ_LXC delete --force $name"
    rm -f "$STATE_DIR/$name.env"
    echo "devbox: $name deleted from $FFZ_HOST"
    ;;

  *) sed -n '2,15p' "$0"; exit 1 ;;
esac
