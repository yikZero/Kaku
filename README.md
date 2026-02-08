<div align="center">
  <img src="assets/logo.png" width="120" alt="Kaku Logo" />
  <h1>Kaku</h1>
  <p><em>A fast, out-of-the-box terminal built for AI coding.</em></p>
</div>

<p align="center">
  <a href="https://github.com/tw93/Kaku/stargazers"><img src="https://img.shields.io/github/stars/tw93/Kaku?style=flat-square" alt="Stars"></a>
  <a href="https://github.com/tw93/Kaku/releases"><img src="https://img.shields.io/github/v/tag/tw93/Kaku?label=version&style=flat-square" alt="Version"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/tw93/Kaku/commits"><img src="https://img.shields.io/github/commit-activity/m/tw93/Kaku?style=flat-square" alt="Commits"></a>
  <a href="https://twitter.com/HiTw93"><img src="https://img.shields.io/badge/follow-Tw93-red?style=flat-square&logo=Twitter" alt="Twitter"></a>
</p>

<p align="center">
  <img src="assets/kaku.png" alt="Kaku Screenshot" width="800" />
  <br/>
  Kaku is a deeply customized fork of <a href="https://github.com/wez/wezterm">WezTerm</a>, designed for an <b>out-of-the-box</b> experience.
  <br/>
  <em>🚧 Work In Progress: Kaku is currently in active development. Features may change.</em>
</p>

## Features

- **Zero Config**: Polished defaults with carefully selected fonts and themes.
- **Built-in Shell Suite**: Comes with Starship, z, Syntax Highlighting, and Autosuggestions.
- **macOS Native**: Optimized for macOS with smooth animations.
- **Fast & Lightweight**: GPU-accelerated rendering with a stripped-down, lightweight core.
- **Lua Scripting**: Infinite customization power via Lua.

## Quick Start

### Install

1. 👉 [**Download Kaku DMG**](https://github.com/tw93/Kaku/releases/latest) & Drag to Applications
2. Open Kaku - Right-click Open if blocked
3. Run `sudo xattr -d com.apple.quarantine /Applications/Kaku.app` if needed

> On first launch, Kaku will offer to set up your shell environment automatically.

### First Run Experience

When you launch Kaku for the first time, it will offer to automatically configure your shell environment:
- **Starship Prompt**: Fast, customizable, and cross-shell.
- **z**: Smart directory jumper.
- **Autosuggestions**: Type less, code faster.
- **Syntax Highlighting**: Catch errors before you run them.

> Kaku respects your existing config. It backs up your `.zshrc` before making any changes.

## Usage Guide

### Shortcuts

Kaku comes with intuitive macOS-native shortcuts:

| Action | Shortcut |
|--------|----------|
| **New Tab** | `Cmd + T` |
| **New Window** | `Cmd + N` |
| **Split Pane (Vertical)** | `Cmd + D` |
| **Split Pane (Horizontal)** | `Cmd + Shift + D` |
| **Zoom/Unzoom Pane** | `Cmd + Shift + Enter` |
| **Resize Pane** | `Cmd + Ctrl + Arrows` |
| **Close Tab/Pane** | `Cmd + W` |
| **Navigate Tabs** | `Cmd + [`, `Cmd + ]` or `Cmd + 1-9` |
| **Navigate Panes** | `Cmd + Opt + Arrows` |
| **Clear Screen** | `Cmd + R` |
| **Font Size** | `Cmd + +`, `Cmd + -`, `Cmd + 0` |

### Smart Navigation (z)

Kaku includes `z` (powered by **zoxide**), a smarter way to navigate directories. It remembers where you go, so you can jump there quickly.

- **Jump to a directory**: `z foo` (jumps to `~/work/foo`)
- **Interactive selection**: `zi foo` (select from list)
- **Go back**: `z -`

## Configuration

Kaku uses a prioritized configuration system to ensure stability while allowing customization.

**Config Load Order:**
1. **Environment Variable**: `KAKU_CONFIG_FILE` (if set)
2. **Bundled Config**: `Kaku.app/Contents/Resources/kaku.lua` (Default experience)
3. **User Config**: `~/.kaku.lua` or `~/.config/kaku/kaku.lua`

To customize Kaku, simply create a `~/.kaku.lua` file. It will override the bundled defaults where specified.

## Development

For developers contributing to Kaku:

```bash
# Clone the repository
git clone https://github.com/tw93/Kaku.git
cd Kaku

# Build and verify
cargo check
cargo test

# Build application and DMG
./scripts/build.sh
# Outputs: dist/Kaku.app and dist/Kaku-{version}.dmg

# Build and open immediately
./scripts/build.sh --open

# Clean build artifacts
rm -rf dist target
```

> **Note**: The build script is macOS-only and requires Rust/Cargo installed.

## Support

- If Kaku helped you, star the repo or [share it](https://twitter.com/intent/tweet?url=https://github.com/tw93/Kaku&text=Kaku%20-%20A%20fast,%20out-of-the-box%20terminal%20built%20for%20AI%20coding.) with friends.
- Got ideas or found bugs? Open an issue or PR.
- Like Kaku? <a href="https://miaoyan.app/cats.html?name=Kaku" target="_blank">Buy Tw93 a Coke</a> to support the project! 🥤 Supporters below.

<a href="https://miaoyan.app/cats.html?name=Kaku"><img src="https://miaoyan.app/assets/sponsors.svg" width="1000" loading="lazy" /></a>

## License

MIT License. See [LICENSE](LICENSE.md) for details.
