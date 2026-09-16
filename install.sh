#!/usr/bin/env sh
set -e

# GHL (Gojo & Haru Language) One-Line Unix/Linux/macOS Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/ofjorque/ghl/main/install.sh | sh

REPO="ofjorque/ghl"
INSTALL_DIR="${GHL_INSTALL_DIR:-$HOME/.ghl/bin}"

printf "\033[1;36m(=^･ω･^=) Installing GHL (Gojo & Haru Language)...\033[0m\n"

# Detect OS and Architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux)
        TARGET_OS="unknown-linux-gnu"
        ;;
    darwin)
        TARGET_OS="apple-darwin"
        ;;
    *)
        printf "\033[1;31mUnsupported OS: %s\033[0m\n" "$OS"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    arm64|aarch64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        printf "\033[1;31mUnsupported architecture: %s\033[0m\n" "$ARCH"
        exit 1
        ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"
ARCHIVE_NAME="ghl-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ARCHIVE_NAME}"

printf "Target platform: \033[1;33m%s\033[0m\n" "$TARGET"
printf "Downloading from: %s\n" "$DOWNLOAD_URL"

mkdir -p "$INSTALL_DIR"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ARCHIVE_NAME"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$DOWNLOAD_URL" -O "$TMP_DIR/$ARCHIVE_NAME"
else
    printf "\033[1;31mError: curl or wget is required to download GHL.\033[0m\n"
    exit 1
fi

tar -xzf "$TMP_DIR/$ARCHIVE_NAME" -C "$INSTALL_DIR"
chmod +x "$INSTALL_DIR/ghl"

printf "\n\033[1;32m(U・ᴥ・U) GHL successfully installed to: %s/ghl\033[0m\n" "$INSTALL_DIR"

# Check if INSTALL_DIR is in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf "\n\033[1;33mAction required:\033[0m Add GHL to your PATH by adding this to your ~/.bashrc or ~/.zshrc:\n"
        printf "    \033[1;36mexport PATH=\"%s:\$PATH\"\033[0m\n\n" "$INSTALL_DIR"
        ;;
esac

printf "Run '\033[1;36mghl --help\033[0m' or '\033[1;36mghl repl\033[0m' to get started!\n"
