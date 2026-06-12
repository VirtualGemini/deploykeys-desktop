#!/bin/sh
# Download the pinned standalone Tailwind CSS CLI into tools/tailwindcss.
#
# The binary is gitignored on purpose: it is a 76 MB platform-specific
# executable, and committing it once already produced a corrupt copy whose
# invalid code signature caused macOS to SIGKILL any process that read it
# (git, Trunk's pre_build hook). Downloading on demand keeps the repo clean
# and lets us verify integrity by checksum.
set -eu

VERSION="v4.3.0"
DEST_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="$DEST_DIR/tailwindcss"

case "$(uname -s)" in
    Darwin) os="macos" ;;
    Linux)  os="linux" ;;
    *) echo "unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    arm64|aarch64) arch="arm64" ;;
    x86_64|amd64)  arch="x64" ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
esac

asset="tailwindcss-${os}-${arch}"

# sha256 checksums from the official v4.3.0 release sha256sums.txt.
case "$asset" in
    tailwindcss-macos-arm64) sum="56b4bbc62dbdc4614a78930d9c6986423a2ec63e4e640144a59a5d95c914322e" ;;
    tailwindcss-macos-x64)   sum="2ba252f770817091e6d0d12a84e0dd531bcc29aad1bfd9d976a3aff1a071b67a" ;;
    tailwindcss-linux-arm64) sum="8f48dcb72be3b351c10563c5329b4638ba8516820dc3b3a1609625a166e87cbd" ;;
    tailwindcss-linux-x64)   sum="73f0e5459054e5cfaa8ab6f3b940f3fbe0f13cc7fd83bc24e7c655033c203400" ;;
    *) echo "no checksum for $asset" >&2; exit 1 ;;
esac

url="https://github.com/tailwindlabs/tailwindcss/releases/download/${VERSION}/${asset}"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

echo "Downloading $asset ($VERSION)..."
curl -fSL --no-progress-meter --proto '=https' --tlsv1.2 -o "$tmp" "$url"

echo "Verifying checksum..."
if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$tmp" | cut -d' ' -f1)"
else
    actual="$(sha256sum "$tmp" | cut -d' ' -f1)"
fi

if [ "$actual" != "$sum" ]; then
    echo "checksum mismatch for $asset" >&2
    echo "  expected: $sum" >&2
    echo "  actual:   $actual" >&2
    exit 1
fi

mv "$tmp" "$DEST"
trap - EXIT
chmod +x "$DEST"
echo "Installed Tailwind CLI -> $DEST"
