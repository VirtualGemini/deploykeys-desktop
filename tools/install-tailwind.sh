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
uname_s="$(uname -s)"
case "$uname_s" in
    Darwin) os="macos"; ext="" ;;
    Linux)  os="linux"; ext="" ;;
    MINGW*|MSYS*|CYGWIN*) os="windows"; ext=".exe" ;;
    *) echo "unsupported OS: $uname_s" >&2; exit 1 ;;
esac
DEST="$DEST_DIR/tailwindcss$ext"

case "$(uname -m)" in
    arm64|aarch64) arch="arm64" ;;
    x86_64|amd64)  arch="x64" ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
esac

asset="tailwindcss-${os}-${arch}${ext}"

# sha256 checksums from the official v4.3.0 release sha256sums.txt.
case "$asset" in
    tailwindcss-macos-arm64) sum="56b4bbc62dbdc4614a78930d9c6986423a2ec63e4e640144a59a5d95c914322e" ;;
    tailwindcss-macos-x64)   sum="2ba252f770817091e6d0d12a84e0dd531bcc29aad1bfd9d976a3aff1a071b67a" ;;
    tailwindcss-linux-arm64) sum="8f48dcb72be3b351c10563c5329b4638ba8516820dc3b3a1609625a166e87cbd" ;;
    tailwindcss-linux-x64)   sum="73f0e5459054e5cfaa8ab6f3b940f3fbe0f13cc7fd83bc24e7c655033c203400" ;;
    tailwindcss-windows-x64.exe) sum="" ;;
    *) echo "no checksum for $asset" >&2; exit 1 ;;
esac

url="https://github.com/tailwindlabs/tailwindcss/releases/download/${VERSION}/${asset}"
# Resumable partial path next to DEST so a dropped connection can continue
# rather than restart the 76 MB transfer. Kept on failure for the next run;
# removed on success. Also gitignored (tools/tailwindcss*).
part="$DEST.partial"

checksum() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        sha256sum "$1" | cut -d' ' -f1
    fi
}

release_checksum() {
    curl -fSL --no-progress-meter --proto '=https' --tlsv1.2 \
        "https://github.com/tailwindlabs/tailwindcss/releases/download/${VERSION}/sha256sums.txt" |
        awk -v asset="$asset" '$2 == asset { print $1 }'
}

if [ -z "$sum" ]; then
    sum="$(release_checksum)"
    if [ -z "$sum" ]; then
        echo "could not find checksum for $asset in ${VERSION} sha256sums.txt" >&2
        exit 1
    fi
fi

# Skip the download entirely if a verified binary is already installed.
if [ -f "$DEST" ] && [ "$(checksum "$DEST")" = "$sum" ]; then
    echo "Tailwind CLI already installed and verified -> $DEST"
    exit 0
fi

echo "Downloading $asset ($VERSION)..."
# -C - resumes from the partial file; --retry rides out transient drops.
curl -fSL --no-progress-meter --proto '=https' --tlsv1.2 \
    --retry 5 --retry-delay 2 --retry-connrefused \
    -C - -o "$part" "$url"

echo "Verifying checksum..."
actual="$(checksum "$part")"
if [ "$actual" != "$sum" ]; then
    echo "checksum mismatch for $asset" >&2
    echo "  expected: $sum" >&2
    echo "  actual:   $actual" >&2
    echo "removing corrupt download: $part" >&2
    rm -f "$part"
    exit 1
fi

mv "$part" "$DEST"
chmod +x "$DEST"
echo "Installed Tailwind CLI -> $DEST"
