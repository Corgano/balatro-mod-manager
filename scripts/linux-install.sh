#!/bin/bash
set -euo pipefail

# Default to building from the local checkout. Pass --clone if you want to build
# from a fresh GitHub clone instead (useful for CI).
USE_LOCAL=true
if [[ "${1:-}" == "--clone" ]]; then
    USE_LOCAL=false
    echo "Using fresh GitHub clone instead of local checkout"
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[94m'
CYAN='\033[38;2;61;181;255m'
NC='\033[0m'

BUILD_DIR=""
CLEANUP_DONE=false
CONTAINER_NAME=""
APPIMAGE_DEST=""
ICON_PATH=""
DESKTOP_ENTRY=""
SYMLINKS=()
PODMAN_PID=""
PREEXIST_APPIMAGE=false
PREEXIST_ICON=false
PREEXIST_DESKTOP=false
PREEXIST_SYMLINKS=()

cleanup() {
    local exit_code=${1:-$?}

    if [[ "$CLEANUP_DONE" == true ]]; then
        return
    fi
    CLEANUP_DONE=true

    if [[ -n "$CONTAINER_NAME" ]] && command -v podman &>/dev/null; then
        podman rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi

    if [[ "$exit_code" -ne 0 ]]; then
        if [[ -n "$APPIMAGE_DEST" && "$PREEXIST_APPIMAGE" == false && -e "$APPIMAGE_DEST" ]]; then
            rm -f "$APPIMAGE_DEST" || true
        fi

        if [[ -n "$ICON_PATH" && "$PREEXIST_ICON" == false && -e "$ICON_PATH" ]]; then
            rm -f "$ICON_PATH" || true
        fi

        if [[ -n "$DESKTOP_ENTRY" && "$PREEXIST_DESKTOP" == false && -e "$DESKTOP_ENTRY" ]]; then
            rm -f "$DESKTOP_ENTRY" || true
        fi

        for i in "${!SYMLINKS[@]}"; do
            local link_path="${SYMLINKS[$i]}"
            local preexisting="${PREEXIST_SYMLINKS[$i]}"
            if [[ "$preexisting" == false && -n "$link_path" && -e "$link_path" ]]; then
                rm -f "$link_path" || true
            fi
        done
    fi

    if [[ "$USE_LOCAL" == false && -n "$BUILD_DIR" && -d "$BUILD_DIR" ]]; then
        if [[ "$exit_code" -eq 0 ]]; then
            echo -e "${YELLOW}9. Cleaning up temporary directory...${NC}"
        else
            echo -e "${YELLOW}Cleaning up temporary directory...${NC}"
        fi
        rm -rf "$BUILD_DIR"
    fi
}

on_interrupt() {
    echo -e "\n${YELLOW}Cancellation received (CTRL+C). Cleaning up...${NC}"
    if [[ -n "$PODMAN_PID" ]] && kill -0 "$PODMAN_PID" >/dev/null 2>&1; then
        kill -INT "$PODMAN_PID" >/dev/null 2>&1 || true
    fi
    if [[ -n "$CONTAINER_NAME" ]] && command -v podman &>/dev/null; then
        podman kill "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    cleanup 130
    exit 130
}

trap on_interrupt INT TERM
trap 'cleanup $?' EXIT

echo -e "${CYAN}"
cat << "EOF"
    ____  __  _____  ___            ____           __        ____
   / __ )/  |/  /  |/  /           /  _/___  _____/ /_____ _/ / /
  / __  / /|_/ / /|_/ /  ______    / // __ \/ ___/ __/ __ `/ / /
 / /_/ / /  / / /  / /  /_____/  _/ // / / (__  ) /_/ /_/ / / /
/_____/_/  /_/_/  /_/           /___/_/ /_/____/\__/\__,_/_/_/

EOF
echo -e "${NC}"

echo -e "${GREEN}Balatro Mod Manager Linux Builder & Installer${NC}"
echo "---------------------------------------------"
echo "Started at $(date)"

# Ensure script runs under Linux
if [[ "$OSTYPE" != "linux-gnu"* && "$OSTYPE" != "linux"* ]]; then
    echo -e "${RED}This script is for Linux only.${NC}"
    exit 1
fi

# Require git
if ! command -v git &>/dev/null; then
    echo -e "${RED}git not found. Please install git.${NC}"
    exit 1
fi

# Require podman
if ! command -v podman &>/dev/null; then
    echo -e "${RED}Podman not found. Please install podman and try again.${NC}"
    echo -e "${YELLOW}Hint: On many distros: sudo apt install podman  OR  sudo dnf install podman${NC}"
    exit 1
fi

echo -e "${GREEN}Podman ✓${NC}"

PODMAN_IMAGE="ubuntu:24.04"
CONTAINER_NAME="bmm-build-$$"

############################################
# SELECT SOURCE (LOCAL OR GITHUB CLONE)
############################################
if [[ "$USE_LOCAL" == true ]]; then
    echo -e "${YELLOW}Using local repository source${NC}"
    SCRIPT_DIR=$(dirname "$(realpath "$0")")
    REPO_DIR="$SCRIPT_DIR/.."
    cd "$REPO_DIR"
    echo -e "${GREEN}Current local repo path:${NC} $REPO_DIR"
    echo -e "${GREEN}Branch selected:${NC} $(git branch --show-current || echo 'unknown')"
else
    BUILD_DIR=$(mktemp -d)
    echo -e "${YELLOW}Using temporary directory: $BUILD_DIR${NC}"
    echo -e "${YELLOW}1. Cloning repository...${NC}"
    git clone https://github.com/skyline69/balatro-mod-manager.git "$BUILD_DIR/bmm"
    REPO_DIR="$BUILD_DIR/bmm"
    cd "$REPO_DIR"
fi

############################################
# BUILD INSIDE PODMAN CONTAINER
############################################
echo -e "${YELLOW}2. Building inside Podman...${NC}"

podman run --rm \
    -v "$REPO_DIR":/workspace:Z \
    -w /workspace \
    --name "$CONTAINER_NAME" \
    "$PODMAN_IMAGE" \
    bash -lc '
        set -euo pipefail
        export DEBIAN_FRONTEND=noninteractive

        echo "Updating APT and installing system dependencies..."
        apt-get update
        apt-get install -y \
            build-essential curl git ca-certificates unzip \
            libgtk-3-0 libgtk-3-dev libgdk-pixbuf-2.0-0 libgdk-pixbuf2.0-dev \
            libcanberra-gtk3-module libcanberra-gtk-module libcanberra-gtk3-dev \
            libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev \
            libayatana-appindicator3-dev librsvg2-dev \
            libx11-dev libxext-dev libxfixes-dev libxi-dev libxrandr-dev libxcursor-dev libxinerama-dev \
            libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
            libavif-dev libaom-dev libdav1d-dev libbrotli-dev libssl-dev zlib1g-dev \
            pkg-config patchelf desktop-file-utils xdg-utils \
            libfuse2 file fuse &&

        echo "Installing Bun..."
        curl -fsSL https://bun.sh/install | bash
        export BUN_INSTALL="/root/.bun"
        export PATH="$BUN_INSTALL/bin:$PATH"

        echo "Installing Rust toolchain via rustup..."
        curl https://sh.rustup.rs -sSf | sh -s -- -y
        source "$HOME/.cargo/env"

		echo "Installing cargo-binstall..."
		curl -L --proto '=https' --tlsv1.2 -sSf \
		  https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

		echo "Installing Tauri CLI via cargo-binstall..."
		cargo binstall tauri-cli -y

        echo "Installing JS deps with bun..."
        bun install --allow-scripts

        echo "Building frontend (bun run build)..."
        bun run build

        echo "Building Rust backend (cargo build --release)..."
        cd src-tauri
        cargo build --release

        echo "Building Tauri bundles (deb, rpm, AppImage)..."
        cargo tauri build
        echo "Container build complete."
    ' &
PODMAN_PID=$!
if ! wait "$PODMAN_PID"; then
    PODMAN_EXIT=$?
else
    PODMAN_EXIT=0
fi
PODMAN_PID=""

if [[ "$PODMAN_EXIT" -ne 0 ]]; then
    exit "$PODMAN_EXIT"
fi

echo -e "${GREEN}Build inside Podman completed successfully.${NC}"

############################################
# INSTALL APPIMAGE ON HOST
############################################

echo -e "${YELLOW}3. Installing AppImage on host...${NC}"

# Prefer the newest AppImage anywhere under target (handles both default and
# target triple paths, e.g., target/x86_64-unknown-linux-gnu/release/...).
APPIMAGE=$(
    find target -path "*bundle/appimage/*.AppImage" -type f -print0 2>/dev/null |
    xargs -r -0 ls -t |
    head -n1 || true
)

if [[ -z "$APPIMAGE" ]]; then
    echo -e "${RED}Error: AppImage not found in target/release/bundle/appimage.${NC}"
    echo "Check that cargo tauri build actually produced an AppImage inside the container."
    exit 1
fi

APP_ID="balatro-mod-manager"
APP_NAME="Balatro Mod Manager"
INSTALL_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"
DESKTOP_DIR="$HOME/.local/share/applications"
APPIMAGE_DEST="$INSTALL_DIR/$APP_ID.AppImage"
ICON_PATH="$ICON_DIR/$APP_ID.png"
DESKTOP_ENTRY="$DESKTOP_DIR/$APP_ID.desktop"
SYMLINKS=("$INSTALL_DIR/balatro-mod-manager" "$INSTALL_DIR/balatro")

[[ -e "$APPIMAGE_DEST" ]] && PREEXIST_APPIMAGE=true
[[ -e "$ICON_PATH" ]] && PREEXIST_ICON=true
[[ -e "$DESKTOP_ENTRY" ]] && PREEXIST_DESKTOP=true
for link in "${SYMLINKS[@]}"; do
    if [[ -e "$link" ]]; then
        PREEXIST_SYMLINKS+=("true")
    else
        PREEXIST_SYMLINKS+=("false")
    fi
done

echo -e "${YELLOW}4. Copying AppImage to $INSTALL_DIR ...${NC}"
mkdir -p "$INSTALL_DIR"
cp "$APPIMAGE" "$APPIMAGE_DEST"
chmod +x "$APPIMAGE_DEST"

echo -e "${YELLOW}5. Installing icon from src-tauri/icons...${NC}"
mkdir -p "$ICON_DIR"
if [[ -f src-tauri/icons/512x512.png ]]; then
    cp src-tauri/icons/512x512.png "$ICON_PATH"
else
    echo -e "${YELLOW}Warning: src-tauri/icons/512x512.png not found, skipping icon install.${NC}"
fi

echo -e "${YELLOW}6. Creating desktop entry...${NC}"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_ENTRY" <<EOF
[Desktop Entry]
Name=$APP_NAME
Exec=$APPIMAGE_DEST
Icon=$APP_ID
Type=Application
Categories=Game;Utility;
Terminal=false
EOF

chmod +x "$DESKTOP_DIR/$APP_ID.desktop"

echo -e "${YELLOW}7. Creating terminal aliases...${NC}"
ln -sf "$APPIMAGE_DEST" "${SYMLINKS[0]}"
ln -sf "$APPIMAGE_DEST" "${SYMLINKS[1]}"

echo -e "${YELLOW}8. Updating icon cache (if available)...${NC}"
gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

############################################
# CLEANUP (IF WE CLONED)
############################################
cleanup 0

echo -e "${GREEN}"
echo "--------------------------------------"
echo "Installation complete!"
echo "--------------------------------------"
echo -e "${NC}"

echo "You can now launch the app via:"
echo "  • Desktop application menu (Balatro Mod Manager)"
echo "  • Terminal: balatro  or  balatro-mod-manager"
echo
echo "AppImage installed at:"
echo "  $INSTALL_DIR/$APP_ID.AppImage"
echo
echo "Bundled packages (kept in the repo directory):"
echo "  AppImage dir : target/release/bundle/appimage"
echo "  Deb packages : target/release/bundle/deb"
echo "  RPM packages : target/release/bundle/rpm"
echo

exit 0
