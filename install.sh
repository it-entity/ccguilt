#!/bin/bash
# ccguilt installer
#   Gitea (LAN):  curl -sSL http://git-aeoru.local:3000/entity/ccguilt/raw/branch/master/install.sh | bash
#   GitHub:       curl -sSL https://raw.githubusercontent.com/it-entity/ccguilt/master/install.sh | bash
# The binary is downloaded from Gitea when reachable, otherwise from GitHub.
set -e

GITEA_URL="http://git-aeoru.local:3000"
GITEA_REPO="entity/ccguilt"
GITHUB_REPO="it-entity/ccguilt"
VERSION="latest"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="ccguilt"

echo "==================================================================="
echo "  CCGUILT INSTALLER"
echo "  Claude Code Guilt Trip — because the planet wasn't suffering enough"
echo "==================================================================="
echo ""

# Detect OS
OS=$(uname -s)
case "$OS" in
    Linux)  OS_TAG="linux" ;;
    Darwin) OS_TAG="macos" ;;
    *)
        echo "Error: Unsupported OS: $OS"
        exit 1
        ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)   ARCH_TAG="x86_64" ;;
    aarch64|arm64)   ARCH_TAG="aarch64" ;;
    *)
        echo "Error: Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

ASSET_NAME="ccguilt-${OS_TAG}-${ARCH_TAG}"

if command -v curl &>/dev/null; then
    fetch() { curl -sSL --connect-timeout "$3" -o "$2" "$1"; }
    fetch_text() { curl -sSL --connect-timeout "$2" "$1"; }
elif command -v wget &>/dev/null; then
    fetch() { wget -q --connect-timeout="$3" --tries=1 -O "$2" "$1"; }
    fetch_text() { wget -q --connect-timeout="$2" --tries=1 -O - "$1"; }
else
    echo "Error: curl or wget required"
    exit 1
fi

# A real binary, not an HTML/JSON error page
is_binary() {
    [ -s "$1" ] || return 1
    if command -v file &>/dev/null; then
        file "$1" | grep -q "executable"
    else
        [ "$(wc -c < "$1")" -gt 102400 ]
    fi
}

# Gitea first (self-hosted, LAN only): resolve the tag, then download the asset
gitea_download() {
    local tag="$VERSION"
    if [ "$tag" = "latest" ]; then
        tag=$(fetch_text "${GITEA_URL}/api/v1/repos/${GITEA_REPO}/releases/latest" 3 2>/dev/null \
            | grep -o '"tag_name":"[^"]*"' | head -1 | cut -d'"' -f4)
    fi
    [ -n "$tag" ] || return 1
    fetch "${GITEA_URL}/${GITEA_REPO}/releases/download/${tag}/${ASSET_NAME}" "$TMP" 3 2>/dev/null \
        && is_binary "$TMP"
}

# GitHub fallback (public)
github_download() {
    local url
    if [ "$VERSION" = "latest" ]; then
        url="https://github.com/${GITHUB_REPO}/releases/latest/download/${ASSET_NAME}"
    else
        url="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${ASSET_NAME}"
    fi
    fetch "$url" "$TMP" 10 2>/dev/null && is_binary "$TMP"
}

echo "Detected: ${OS} ${ARCH}"
echo ""

TMP=$(mktemp)
echo "Downloading ${ASSET_NAME} from Gitea (${GITEA_URL})..."
if gitea_download; then
    echo "Downloaded from Gitea."
else
    echo "Gitea unavailable or has no ${ASSET_NAME}; trying GitHub..."
    if github_download; then
        echo "Downloaded from GitHub."
    else
        echo "Error: could not download a valid binary from Gitea or GitHub."
        echo "Check: ${GITEA_URL}/${GITEA_REPO}/releases"
        echo "       https://github.com/${GITHUB_REPO}/releases"
        rm -f "$TMP"
        exit 1
    fi
fi

chmod +x "$TMP"

# Install
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
elif command -v sudo &>/dev/null; then
    echo "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
    sudo chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
    mv "$TMP" "${INSTALL_DIR}/${BINARY_NAME}"
    echo "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
    echo ""
    echo "Make sure ${INSTALL_DIR} is in your PATH:"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
fi

echo ""
echo "Done! Run 'ccguilt --version' to verify."
echo "Run 'ccguilt daily' to see your environmental destruction."

# Auto-register MCP server if Claude Code is installed
if command -v claude &>/dev/null; then
    echo ""
    echo "Detected Claude Code — registering ccguilt MCP server..."
    if "${INSTALL_DIR}/${BINARY_NAME}" --setup-mcp 2>&1; then
        echo "  Open a new Claude Code session and try: \"how much CO2 have I burned today?\""
    else
        echo "  (MCP auto-registration failed — run 'ccguilt --setup-mcp' manually if you want it.)"
    fi
fi

echo ""
echo "Remember: this report was installed using energy. You're welcome, planet."
