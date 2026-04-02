#!/bin/sh
set -e

BASE_URL="https://skippr.io/releases"
INSTALL_DIR="/usr/local/bin"
BINARY="skippr"

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    local os arch target

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin)
            case "$arch" in
                arm64|aarch64) target="aarch64-apple-darwin" ;;
                x86_64)        target="x86_64-apple-darwin" ;;
                *)             err "Unsupported macOS architecture: $arch" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64|amd64)  target="x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
                *)             err "Unsupported Linux architecture: $arch" ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*)
            err "Windows detected. Install with PowerShell instead:  irm https://skippr.io/install.ps1 | iex"
            ;;
        *)
            err "Unsupported operating system: $os"
            ;;
    esac

    local tag url tmpdir

    say "Detecting latest release..."
    tag="$(curl -fsSL "$BASE_URL/latest.txt")"

    if [ -z "$tag" ]; then
        err "Could not determine latest release."
    fi

    say "Latest release: $tag"

    url="${BASE_URL}/${tag}/${BINARY}-${target}.tar.gz"

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    say "Downloading $BINARY for $target..."
    curl -fSL --progress-bar "$url" -o "$tmpdir/skippr.tar.gz"

    say "Extracting..."
    tar -xzf "$tmpdir/skippr.tar.gz" -C "$tmpdir"

    local bin_path="$tmpdir/$BINARY-$target/$BINARY"
    if [ ! -f "$bin_path" ]; then
        bin_path="$(find "$tmpdir" -name "$BINARY" -type f | head -1)"
    fi

    if [ ! -f "$bin_path" ]; then
        err "Binary not found in archive."
    fi

    chmod +x "$bin_path"

    if [ -w "$INSTALL_DIR" ]; then
        mv "$bin_path" "$INSTALL_DIR/$BINARY"
    else
        say "Installing to $INSTALL_DIR (requires sudo)..."
        sudo mv "$bin_path" "$INSTALL_DIR/$BINARY"
    fi

    say ""
    say "  skippr $tag installed to $INSTALL_DIR/$BINARY"
    say ""
    say "  Get started:"
    say "    skippr user login"
    say "    skippr init my-project"
    say "    skippr connect warehouse snowflake"
    say "    skippr connect source mssql"
    say "    skippr run"
    say ""
}

say() {
    printf '%s\n' "$1"
}

err() {
    say "Error: $1" >&2
    exit 1
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "Required command not found: $1"
    fi
}

main "$@"
