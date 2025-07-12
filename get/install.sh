#!/bin/sh
set -e

get_latest_version() {
    curl -sSL "https://api.github.com/repos/rotkonetworks/githem/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"v?([^"]+)".*/\1/'
}

detect_platform() {
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

is_windows() {
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) return 0 ;;
        *) return 1 ;;
    esac
}

run_powershell_installer() {
    echo "Windows detected. Running PowerShell installer..."
    
    if command -v powershell >/dev/null 2>&1; then
        powershell -Command "& {Set-ExecutionPolicy Bypass -Scope Process -Force; iwr -useb get.githem.com/install.ps1 | iex}"
    elif command -v pwsh >/dev/null 2>&1; then
        pwsh -Command "& {Set-ExecutionPolicy Bypass -Scope Process -Force; iwr -useb get.githem.com/install.ps1 | iex}"
    else
        echo "PowerShell not found. Please install PowerShell or run:"
        echo "iwr -useb get.githem.com/install.ps1 | powershell -"
        exit 1
    fi
}

download_verify() {
    url="$1"
    dest="$2"
    
    echo "Downloading $url..."
    curl -sSL "$url" -o "$dest"
    
    if command -v sha512sum >/dev/null 2>&1; then
        curl -sSL "$url.sha512" -o "$dest.sha512" 2>/dev/null && {
            echo "Verifying checksum..."
            sha512sum -c "$dest.sha512" 2>/dev/null || echo "Warning: checksum verification failed"
            rm -f "$dest.sha512"
        } || echo "Skipping verification (no checksum file)"
    fi
}

install_unix() {
    platform="$1"
    arch="$2"
    
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        echo "Failed to fetch latest version"
        exit 1
    fi
    
    BASE_URL="https://github.com/rotkonetworks/githem/releases/download/v${VERSION}"
    
    case "$platform" in
        arch)
            echo "Arch Linux detected. Installing to /usr/local/bin..."
            binary="githem-linux-$arch"
            download_verify "$BASE_URL/$binary" "/tmp/$binary"
            sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
            rm -f "/tmp/$binary"
            ;;
        nixos)
            echo "NixOS detected. Installing to ~/.local/bin..."
            mkdir -p "$HOME/.local/bin"
            binary="githem-linux-$arch"
            download_verify "$BASE_URL/$binary" "$HOME/.local/bin/githem"
            chmod +x "$HOME/.local/bin/githem"
            if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
                echo "Add ~/.local/bin to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
            fi
            ;;
        linux)
            echo "Generic Linux detected. Installing to /usr/local/bin..."
            binary="githem-linux-$arch"
            download_verify "$BASE_URL/$binary" "/tmp/$binary"
            sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
            rm -f "/tmp/$binary"
            ;;
        macos)
            echo "macOS detected. Installing to /usr/local/bin..."
            binary="githem-macos-$arch"
            download_verify "$BASE_URL/$binary" "/tmp/$binary"
            sudo install -m 755 "/tmp/$binary" /usr/local/bin/githem
            rm -f "/tmp/$binary"
            ;;
    esac
    
    echo "Installation complete. Run 'githem --version' to verify."
}

main() {
    if is_windows; then
        run_powershell_installer
        return
    fi
    
    platform=$(detect_platform)
    arch=$(detect_arch)
    
    if [ "$platform" = "unsupported" ] || [ "$arch" = "unsupported" ]; then
        echo "Unsupported OS/architecture combination"
        echo "Detected: $(uname -s) $(uname -m)"
        exit 1
    fi
    
    echo "Detected: $platform ($arch)"
    echo "Installing githem..."
    
    install_unix "$platform" "$arch"
}

main "$@"