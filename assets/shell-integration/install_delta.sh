#!/bin/bash
# Kaku - Delta Installation Script
# Installs and configures delta for beautiful git diffs

set -euo pipefail

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# Determine resource directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESOURCES_DIR="${RESOURCES_DIR:-$SCRIPT_DIR}"

# Paths
USER_CONFIG_DIR="$HOME/.config/kaku/zsh"
USER_BIN_DIR="$USER_CONFIG_DIR/bin"
VENDOR_DELTA="$RESOURCES_DIR/../vendor/delta"

# Check if running in app bundle
if [[ ! -f "$VENDOR_DELTA" ]]; then
    VENDOR_DELTA="/Applications/Kaku.app/Contents/Resources/vendor/delta"
fi

echo -e "${BOLD}Delta Installation${NC}"
echo -e "${NC}Git diff beautifier for better code review${NC}"
echo ""

# Check if delta is already installed in Kaku user bin.
# Even if already installed, still continue to apply git config defaults.
if command -v delta &> /dev/null && [[ "$(command -v delta)" == "$USER_BIN_DIR/delta" ]]; then
    echo -e "${GREEN}✓${NC} Delta binary already installed"
else
    # Check if vendor delta exists
    if [[ ! -f "$VENDOR_DELTA" ]]; then
        echo -e "${YELLOW}⚠${NC}  Delta binary not found in vendor directory"
        echo -e "${NC}    Expected: $VENDOR_DELTA${NC}"
        echo ""
        echo "You can install delta manually:"
        echo "  brew install git-delta"
        exit 1
    fi

    # Create bin directory
    mkdir -p "$USER_BIN_DIR"

    # Copy delta binary
    echo -n "  Installing delta binary... "
    cp "$VENDOR_DELTA" "$USER_BIN_DIR/delta"
    chmod +x "$USER_BIN_DIR/delta"
    echo -e "${GREEN}done ✅${NC}"
fi

# Configure git to use delta
echo -n "  Configuring git... "
git config --global core.pager "delta"
git config --global interactive.diffFilter "delta --color-only"
git config --global delta.navigate true
git config --global delta.pager "less --mouse --wheel-lines=3 -R -F -X"
git config --global delta.line-numbers true
git config --global delta.side-by-side true
git config --global delta.line-fill-method "spaces"
echo -e "${GREEN}done ✅${NC}"

# Set delta theme
echo -n "  Applying Kaku-aligned style... "
git config --global delta.syntax-theme "Coldark-Dark"
git config --global delta.file-style "omit"
git config --global delta.file-decoration-style "omit"
git config --global delta.hunk-header-style "file line-number syntax"

# Clear previously customized colors so the selected syntax theme controls rendering.
for key in \
  delta.hunk-header-file-style \
  delta.hunk-header-line-number-style \
  delta.hunk-header-decoration-style \
  delta.line-numbers-left-style \
  delta.line-numbers-right-style \
  delta.line-numbers-minus-style \
  delta.line-numbers-plus-style \
  delta.file-added-label \
  delta.file-copied-label \
  delta.file-modified-label \
  delta.file-removed-label \
  delta.file-renamed-label \
  delta.minus-style \
  delta.minus-emph-style \
  delta.plus-style \
  delta.plus-emph-style; do
  git config --global --unset-all "$key" 2>/dev/null || true
done
echo -e "${GREEN}done ✅${NC}"

echo ""
echo -e "${GREEN}${BOLD}✓ Delta installed successfully!${NC}"
echo -e "${NC}  Default view: side-by-side with line numbers${NC}"
echo ""
echo -e "${BOLD}Usage:${NC}"
echo -e "  ${NC}Delta works automatically with git commands:${NC}"
echo "    git diff          # View changes with syntax highlighting"
echo "    git diff --staged # View staged changes"
echo "    git show          # View commit details"
echo "    git log -p        # View commit history with diffs"
echo ""
echo -e "${NC}  No need to learn new commands - delta just makes git better!${NC}"
echo ""
