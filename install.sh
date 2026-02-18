#!/bin/sh
# Trust installer — downloads a pre-built binary from GitHub Releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/matiasvillaverde/trust/main/install.sh | sh
#
# Environment variables:
#   TRUST_VERSION  — version to install (default: latest)
#   INSTALL_DIR    — destination directory (default: ~/.local/bin)

set -eu

REPO="matiasvillaverde/trust"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# ---------- helpers ----------

die() { printf 'Error: %s\n' "$1" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not found"
}

# ---------- detect platform ----------

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Darwin) OS_TAG="apple-darwin" ;;
  Linux)  OS_TAG="unknown-linux-gnu" ;;
  *)      die "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64)            ARCH_TAG="x86_64" ;;
  aarch64|arm64)     ARCH_TAG="aarch64" ;;
  *)                 die "Unsupported architecture: $ARCH" ;;
esac

# On macOS, prefer the universal binary
if [ "$OS" = "Darwin" ]; then
  TARGET="universal-apple-darwin"
else
  TARGET="${ARCH_TAG}-${OS_TAG}"
fi

# ---------- resolve version ----------

need curl

if [ -z "${TRUST_VERSION:-}" ]; then
  TRUST_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')
  [ -n "$TRUST_VERSION" ] || die "Could not determine latest version"
fi

TAG="v${TRUST_VERSION}"
ARCHIVE="${TAG}-${TARGET}.tar.gz"
CHECKSUM_FILE="${ARCHIVE}.sha256"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

echo "Installing trust ${TAG} for ${TARGET}..."

# ---------- download ----------

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fSL -o "${TMPDIR}/${ARCHIVE}" "${BASE_URL}/${ARCHIVE}" \
  || die "Failed to download ${BASE_URL}/${ARCHIVE}"

# ---------- verify checksum (if available) ----------

if curl -fSL -o "${TMPDIR}/${CHECKSUM_FILE}" "${BASE_URL}/${CHECKSUM_FILE}" 2>/dev/null; then
  echo "Verifying checksum..."
  cd "$TMPDIR"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$CHECKSUM_FILE" || die "Checksum verification failed"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$CHECKSUM_FILE" || die "Checksum verification failed"
  else
    echo "Warning: no sha256 tool found, skipping checksum verification"
  fi
  cd - >/dev/null
else
  echo "Warning: checksum file not available, skipping verification"
fi

# ---------- extract & install ----------

mkdir -p "$INSTALL_DIR"
tar xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"
# The binary is named 'trust' inside the archive
cp "${TMPDIR}/trust" "${INSTALL_DIR}/trust"
chmod +x "${INSTALL_DIR}/trust"

echo "Installed trust to ${INSTALL_DIR}/trust"

# ---------- PATH check ----------

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "Warning: ${INSTALL_DIR} is not in your PATH."
    echo "Add it by running:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo "Or add the line above to your shell profile (~/.bashrc, ~/.zshrc, etc.)"
    ;;
esac

# ---------- Linux notes ----------

if [ "$OS" = "Linux" ]; then
  echo ""
  echo "Note: trust requires libdbus-1-3 at runtime."
  echo "Install it with:"
  echo "  Ubuntu/Debian: sudo apt-get install libdbus-1-3"
  echo "  Fedora/RHEL:   sudo dnf install dbus-libs"
fi

echo ""
echo "Run 'trust --help' to get started."
