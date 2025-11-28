#!/bin/bash
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[94m'
CYAN='\033[38;2;61;181;255m'
NC='\033[0m'

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

# Check Linux
if [[ "$OSTYPE" != "linux-gnu"* ]]; then
    echo -e "${RED}This script is for Linux only.${NC}"
    exit 1
fi

# Check git
if ! command -v git &>/dev/null; then
    echo -e "${RED}git not found. Please install git.${NC}"
    exit 1
fi

# Check Devbox
if ! command -v devbox &>/dev/null; then
    echo -e "${YELLOW}Devbox not found. Installing...${NC}"
    curl -fsSL https://get.jetify.com/devbox | bash
    export PATH="$HOME/.local/bin:$PATH"
fi

echo -e "${GREEN}Devbox ✓${NC}"

# Create temp build dir
BUILD_DIR=$(mktemp -d)
echo -e "${YELLOW}Using temporary directory: $BUILD_DIR${NC}"

echo -e "${YELLOW}1. Cloning repository...${NC}"
git clone https://github.com/skyline69/balatro-mod-manager.git "$BUILD_DIR/bmm"

cd "$BUILD_DIR/bmm"

echo -e "${YELLOW}2. Installing dependencies with bun inside devbox...${NC}"
devbox run bun install --allow-scripts

echo -e "${YELLOW}3. Building frontend...${NC}"
devbox run bun run build

echo -e "${YELLOW}4. Building Rust backend...${NC}"
cd src-tauri
devbox run cargo build --release
cd ..

echo -e "${YELLOW}5. Creating AppImage…${NC}"
devbox run cargo tauri build

echo -e "${GREEN}Build completed successfully.${NC}"

# AppImage install
APPIMAGE=$(find src-tauri/target/release/bundle/appimage -type f -name "*.AppImage" | head -n1)
if [[ -z "$APPIMAGE" ]]; then
    echo -e "${RED}Error: AppImage not found.${NC}"
    exit 1
fi

APP_ID="balatro-mod-manager"
APP_NAME="Balatro Mod Manager"
INSTALL_DIR="$HOME/.local/bin"
ICON_DIR="$HOME/.local/share/icons/hicolor/512x512/apps"
DESKTOP_DIR="$HOME/.local/share/applications"

echo -e "${YELLOW}6. Installing AppImage into $INSTALL_DIR ...${NC}"
mkdir -p "$INSTALL_DIR"
cp "$APPIMAGE" "$INSTALL_DIR/$APP_ID.AppImage"
chmod +x "$INSTALL_DIR/$APP_ID.AppImage"

echo -e "${YELLOW}7. Installing icon from src-tauri/icons...${NC}"
mkdir -p "$ICON_DIR"
cp src-tauri/icons/512x512.png "$ICON_DIR/$APP_ID.png"

echo -e "${YELLOW}8. Creating desktop entry...${NC}"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/$APP_ID.desktop" <<EOF
[Desktop Entry]
Name=$APP_NAME
Exec=$INSTALL_DIR/$APP_ID.AppImage
Icon=$APP_ID
Type=Application
Categories=Game;Utility;
Terminal=false
EOF

echo -e "${YELLOW}9. Creating terminal alias 'balatro'...${NC}"
ln -sf "$INSTALL_DIR/$APP_ID.AppImage" "$INSTALL_DIR/balatro"

echo -e "${YELLOW}10. Updating icon cache...${NC}"
gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" || true

echo -e "${GREEN}"
echo "--------------------------------------"
echo "Installation complete!"
echo "--------------------------------------"
echo -e "${NC}"

echo "You can now launch the app via:"
echo "✔ your desktop environment application menu"
echo "✔ typing 'balatro' in a terminal"
echo
echo "Installed AppImage:"
echo "$INSTALL_DIR/$APP_ID.AppImage"
echo
echo "Installed icon:"
echo "$ICON_DIR/$APP_ID.png"
echo
echo "Desktop entry:"
echo "$DESKTOP_DIR/$APP_ID.desktop"
echo
echo "Temporary build folder (to clean manually later):"
echo "$BUILD_DIR"
echo
echo "Optional:"
echo "rm -rf $BUILD_DIR"
echo

exit 0

