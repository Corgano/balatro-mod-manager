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
DOWNLOAD_DIR=""
CLEANUP_DONE=false
FLATPAK_BUNDLE=""
PREEXIST_FLATPAK=false

cleanup() {
    local exit_code=${1:-$?}

    if [[ "$CLEANUP_DONE" == true ]]; then
        return
    fi
    CLEANUP_DONE=true

    if [[ "$exit_code" -ne 0 ]]; then
        if [[ -n "$FLATPAK_BUNDLE" && "$PREEXIST_FLATPAK" == false && -e "$FLATPAK_BUNDLE" ]]; then
            rm -f "$FLATPAK_BUNDLE" || true
        fi
    fi

    if [[ "$USE_LOCAL" == false && -n "$BUILD_DIR" && -d "$BUILD_DIR" ]]; then
        if [[ "$exit_code" -eq 0 ]]; then
            echo -e "${YELLOW}4. Cleaning up temporary directory...${NC}"
        else
            echo -e "${YELLOW}Cleaning up temporary directory...${NC}"
        fi
        rm -rf "$BUILD_DIR"
    fi

    if [[ -n "$DOWNLOAD_DIR" && -d "$DOWNLOAD_DIR" ]]; then
        rm -rf "$DOWNLOAD_DIR"
    fi
}

on_interrupt() {
    echo -e "\n${YELLOW}Cancellation received (CTRL+C). Cleaning up...${NC}"
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

# Require curl
if ! command -v curl &>/dev/null; then
    echo -e "${RED}curl not found. Please install curl and try again.${NC}"
    exit 1
fi

# Require flatpak
if ! command -v flatpak &>/dev/null; then
    echo -e "${RED}Flatpak not found. Please install flatpak and try again.${NC}"
    exit 1
fi

echo -e "${GREEN}Flatpak ✓${NC}"

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
# BUILD + INSTALL FLATPAK
############################################
echo -e "${YELLOW}2. Installing Flatpak bundle (release preferred)...${NC}"

DOWNLOAD_DIR=$(mktemp -d)
FLATPAK_BUNDLE="$DOWNLOAD_DIR/balatro-mod-manager.flatpak"
RELEASE_URL=""
RELEASE_JSON=""
if RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/skyline69/balatro-mod-manager/releases" 2>/dev/null); then
    RELEASE_URL=$(printf '%s' "$RELEASE_JSON" | \
        grep -Eo '"browser_download_url":\s*"[^"]+\.flatpak"' | \
        head -n1 | cut -d '"' -f4 || true)
fi

if [[ -n "$RELEASE_URL" ]]; then
    echo -e "${YELLOW}Downloading latest release Flatpak...${NC}"
    if curl -fsSL "$RELEASE_URL" -o "$FLATPAK_BUNDLE"; then
        echo -e "${YELLOW}Installing Flatpak bundle...${NC}"
        flatpak install --user -y --reinstall "$FLATPAK_BUNDLE"
        cleanup 0
        echo -e "${GREEN}"
        echo "--------------------------------------"
        echo "Installation complete!"
        echo "--------------------------------------"
        echo -e "${NC}"

        echo "You can now launch the app via:"
        echo "  • Terminal: flatpak run io.balatro.ModManager"
        echo
        echo "Flatpak bundle downloaded from:"
        echo "  $RELEASE_URL"
        echo
        exit 0
    fi
fi

echo -e "${YELLOW}Release download failed; building Flatpak locally...${NC}"

# Require flatpak-builder for local build fallback
if ! command -v flatpak-builder &>/dev/null; then
    echo -e "${RED}flatpak-builder not found. Please install flatpak-builder and try again.${NC}"
    exit 1
fi
echo -e "${GREEN}flatpak-builder ✓${NC}"

RUNTIMES=(
    "org.gnome.Platform//47"
    "org.gnome.Sdk//47"
    "org.freedesktop.Sdk.Extension.node20//24.08"
    "org.freedesktop.Sdk.Extension.rust-stable//24.08"
)

MISSING_RUNTIMES=()
for runtime in "${RUNTIMES[@]}"; do
    if ! flatpak info "$runtime" >/dev/null 2>&1; then
        MISSING_RUNTIMES+=("$runtime")
    fi
done

if [[ "${#MISSING_RUNTIMES[@]}" -gt 0 ]]; then
    echo -e "${YELLOW}Installing Flatpak runtimes (first-time setup)...${NC}"
    flatpak install --user -y "${MISSING_RUNTIMES[@]}"
fi

flatpak-builder --force-clean --repo=repo build-dir packaging/flatpak/io.balatro.ModManager.json

FLATPAK_BUNDLE="$REPO_DIR/balatro-mod-manager.flatpak"
if [[ -e "$FLATPAK_BUNDLE" ]]; then
    PREEXIST_FLATPAK=true
fi

flatpak build-bundle repo "$FLATPAK_BUNDLE" io.balatro.ModManager master

echo -e "${YELLOW}3. Installing Flatpak bundle...${NC}"
flatpak install --user -y --reinstall "$FLATPAK_BUNDLE"

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
echo "  • Terminal: flatpak run io.balatro.ModManager"
echo
echo "Flatpak bundle saved at:"
echo "  $FLATPAK_BUNDLE"
echo
echo "Build artifacts:"
echo "  build-dir/ (Flatpak build dir)"
echo "  repo/ (Flatpak repo)"
echo

exit 0
