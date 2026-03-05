#!/bin/bash
set -euo pipefail

REPO="JeanMaximilienCadic/myawesomescripts"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="apple-darwin" ;;
  Linux)  os="unknown-linux-gnu" ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="awsx2-${arch}-${os}"

# Get latest release tag matching awsx2-v*
TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=10" \
  | grep -o '"tag_name": *"awsx2-v[^"]*"' \
  | head -1 \
  | cut -d'"' -f4)

if [ -z "$TAG" ]; then
  echo "No awsx2 release found." >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/${TARGET}.tar.gz"
echo "Installing awsx2 ${TAG#awsx2-} (${arch}-${os})..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" | tar xz -C "$TMP"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP/awsx2" "$INSTALL_DIR/awsx2"
else
  sudo mv "$TMP/awsx2" "$INSTALL_DIR/awsx2"
fi

echo "Installed: $(awsx2 --version)"
