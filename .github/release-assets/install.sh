#!/bin/sh
set -eu

REPO="Skiley/j18n"
INSTALL_DIR="${J18N_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"

case "$(uname -s)" in
  Linux*)  os="unknown-linux-musl" ;;
  Darwin*) os="apple-darwin" ;;
  *) echo "j18n: unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64)  arch="x86_64" ;;
  aarch64|arm64) arch="aarch64" ;;
  *) echo "j18n: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

target="${arch}-${os}"
archive="j18n-cli-${target}.tar.xz"

if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${archive}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $archive..."
curl -fsSL "$url" -o "$tmp/$archive"
tar -xJf "$tmp/$archive" -C "$tmp"

mkdir -p "$INSTALL_DIR"
mv "$tmp/j18n-cli-${target}/j18n" "$INSTALL_DIR/j18n"
chmod +x "$INSTALL_DIR/j18n"

echo "Installed j18n to $INSTALL_DIR/j18n"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
