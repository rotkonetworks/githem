#!/bin/sh
set -e

# Fetch latest release version from GitHub API
get_latest_version() {
    curl -sSL "https://api.github.com/repos/rotkonetworks/githem/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"v?([^"]+)".*/\1/'
}

VERSION=$(get_latest_version)
BASE_URL="https://github.com/rotkonetworks/githem/releases/download/v${VERSION}"

detect_os() {
    case "$(uname -s)" in
        Linux*)
            if [ -f /etc/arch-release ]; then
                echo "arch"
            elif [ -f /etc/nixos/configuration.nix ]; then
                echo "nixos"
            else
                echo "linux"
            fi
            ;;
        Darwin*) echo "macos" ;;
        MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
        *) echo "unsupported" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x64" ;;
        aarch64|arm64) echo "arm64" ;;
        *) echo "unsupported" ;;
    esac
}

download_verify() {
    url="$1"
    dest="$2"
    
    echo "Downloading $url..."
    curl -sSL "$url" -o "$dest"
    
    if command -v sha256sum >/dev/null 2>&1; then
        curl -sSL "$url.sha512" -o "$dest.sha512" 2>/dev/null && {
            echo "Verifying..."
            sha512sum -c "$dest.sha512" 2>/dev/null || echo "Warning: checksum verification failed"
            rm -f "$dest.sha512"
        } || echo "Skipping verification (no checksum file)"
    fi
}

install_arch() {
    echo "Arch Linux detected. Installing to /usr/local/bin..."
    echo "Note: Consider creating an AUR package for better integration"
    
    binary="githem-linux-$1"
    download_verify "$BASE_URL/$binary" "/tmp/$binary"
    
    sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
    rm -f "/tmp/$binary"
}

install_nixos() {
    echo "NixOS detected. Installing to ~/.local/bin..."
    
    mkdir -p "$HOME/.local/bin"
    binary="githem-linux-$1"
    download_verify "$BASE_URL/$binary" "$HOME/.local/bin/githem"
    chmod +x "$HOME/.local/bin/githem"
    
    if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
        echo "Add ~/.local/bin to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
}

install_linux() {
    echo "Generic Linux detected. Installing to /usr/local/bin..."
    
    binary="githem-linux-$1"
    download_verify "$BASE_URL/$binary" "/tmp/$binary"
    
    sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
    rm -f "/tmp/$binary"
}

install_macos() {
    echo "macOS detected. Installing to /usr/local/bin..."
    
    binary="githem-macos-$1"
    download_verify "$BASE_URL/$binary" "/tmp/$binary"
    
    sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
    rm -f "/tmp/$binary"
}

install_windows() {
    echo "Windows detected. Please run the PowerShell installer instead:"
    echo "iwr -useb get.githem.com/install.ps1 | iex"
    exit 1
}

main() {
    os=$(detect_os)
    arch=$(detect_arch)
    
    if [ "$os" = "unsupported" ] || [ "$arch" = "unsupported" ]; then
        echo "Unsupported OS/architecture combination"
        exit 1
    fi
    
    if [ -z "$VERSION" ]; then
        echo "Failed to fetch latest version"
        exit 1
    fi
    
    echo "Detected: $os ($arch)"
    echo "Installing githem v${VERSION}..."
    
    case "$os" in
        arch) install_arch "$arch" ;;
        nixos) install_nixos "$arch" ;;
        linux) install_linux "$arch" ;;
        macos) install_macos "$arch" ;;
        windows) install_windows ;;
    esac
    
    echo "Installation complete. Run 'githem --version' to verify."
}

main "$@"
