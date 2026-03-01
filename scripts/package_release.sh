#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="mouthwrite-linux"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
ARCH="$(uname -m)"

if [[ -z "${VERSION}" ]]; then
  echo "Failed to read version from Cargo.toml"
  exit 1
fi

PKG_BASENAME="${APP_NAME}-${VERSION}-linux-${ARCH}"
PKG_DIR="dist/${PKG_BASENAME}"
TARBALL="dist/${PKG_BASENAME}.tar.gz"

echo "Building release binary..."
cargo build --release

echo "Preparing package directory: ${PKG_DIR}"
rm -rf "${PKG_DIR}"
mkdir -p "${PKG_DIR}"

cp target/release/mouthwrite-linux "${PKG_DIR}/"
cp config_template.toml "${PKG_DIR}/"
cp -r assets "${PKG_DIR}/"
cp packaging/systemd/mouthwrite.service "${PKG_DIR}/"
cp README.md "${PKG_DIR}/"

if command -v strip >/dev/null 2>&1; then
  strip "${PKG_DIR}/mouthwrite-linux" || true
fi

mkdir -p dist
echo "Creating tarball: ${TARBALL}"
tar -C dist -czf "${TARBALL}" "${PKG_BASENAME}"

echo "Done."
ls -lh "${TARBALL}"
