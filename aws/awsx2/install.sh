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
  *) echo "Error: unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) echo "Error: unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="awsx2-${arch}-${os}"

echo "Fetching latest awsx2 release..."

# Get latest release tag matching awsx2-v*
RELEASES=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" 2>&1) || {
  echo "Error: failed to fetch releases from GitHub." >&2
  exit 1
}

TAG=$(echo "$RELEASES" \
  | grep -o '"tag_name": *"awsx2-v[^"]*"' \
  | head -1 \
  | cut -d'"' -f4)

if [ -z "$TAG" ]; then
  echo "Error: no awsx2 release found at https://github.com/${REPO}/releases" >&2
  echo "The release may still be building. Check GitHub Actions and try again in a few minutes." >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/${TARGET}.tar.gz"
echo "Downloading ${TAG} for ${arch}-${os}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

HTTP_CODE=$(curl -fsSL -w '%{http_code}' -o "$TMP/awsx2.tar.gz" "$URL" 2>/dev/null) || {
  echo "Error: failed to download ${URL}" >&2
  echo "Binary for ${TARGET} may not be available in this release." >&2
  exit 1
}

if [ "$HTTP_CODE" != "200" ]; then
  echo "Error: download returned HTTP ${HTTP_CODE}" >&2
  exit 1
fi

tar xz -C "$TMP" -f "$TMP/awsx2.tar.gz" || {
  echo "Error: failed to extract archive." >&2
  exit 1
}

if [ ! -f "$TMP/awsx2" ]; then
  echo "Error: awsx2 binary not found in archive." >&2
  exit 1
fi

chmod +x "$TMP/awsx2"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP/awsx2" "$INSTALL_DIR/awsx2"
else
  echo "Installing to ${INSTALL_DIR} (requires sudo)..."
  sudo mv "$TMP/awsx2" "$INSTALL_DIR/awsx2"
fi

echo "Installed: $("$INSTALL_DIR/awsx2" --version)"
