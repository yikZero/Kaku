<div align="center">
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
  <img src="assets/kaku.jpeg" alt="Kaku Screenshot" width="1000" />
  <br/>
  Kaku is a deeply customized fork of <a href="https://github.com/wez/wezterm">WezTerm</a>, designed for an out-of-the-box experience.
</p>

## Features

- **Zero Config**: Defaults with JetBrains Mono, opencode theme, macOS font rendering, and low-res font sizing.
- **Built-in Shell Suite**: Pre-loaded Starship, z, Delta, syntax highlighting, autocompletions, and zsh history.
- **Fast & Lightweight**: 40% smaller binary, instant startup, lazy loading, stripped-down GPU-accelerated core.
- **WezTerm-Compatible Config**: Use WezTerm's Lua config directly with full API compatibility and no migration.

## Quick Start

1. [Download Kaku DMG](https://github.com/tw93/Kaku/releases/latest) & Drag to Applications
2. Or install with Homebrew: `brew install tw93/tap/kakuku`
3. Open Kaku. The app is notarized by Apple, so it opens without security warnings
4. On first launch, Kaku will automatically set up your shell environment

## Usage Guide

Kaku comes with intuitive macOS-native shortcuts:

| Action | Shortcut |
| :--- | :--- |
| New Tab | `Cmd + T` |
| New Window | `Cmd + N` |
| Split Pane Vertical | `Cmd + D` |
| Split Pane Horizontal | `Cmd + Shift + D` |
| Toggle Split Direction | `Cmd + Shift + S` |
| Zoom/Unzoom Pane | `Cmd + Shift + Enter` |
| Resize Pane | `Cmd + Ctrl + Arrows` |
| Close Tab/Pane | `Cmd + W` |
| Navigate Tabs | `Cmd + Shift + [`, `Cmd + Shift + ]` or `Cmd + 1-9` |
| Navigate Panes | `Cmd + Opt + Arrows` |
| Clear Screen | `Cmd + K` |
| Font Size | `Cmd + +`, `Cmd + -`, `Cmd + 0` |
| Smart Jump | `z <dir>` |
| Smart Select | `z -l <dir>` |
| Recent Dirs | `z -t` |

## Configuration

Kaku comes with a carefully curated suite of CLI tools, pre-configured for immediate productivity:

- **Starship**: A fast, customizable prompt showing git status, package versions, and execution time.
- **z**: A smarter cd command that learns your most used directories for instant navigation.
- **Delta**: A syntax-highlighting pager for git, diff, and grep output.
- **zsh-completions**: Extended command and subcommand completion definitions.
- **Syntax Highlighting**: Real-time command validation and coloring.
- **Autosuggestions**: Intelligent, history-based completions similar to Fish shell.

Kaku uses `~/.config/kaku/kaku.lua` for configuration, fully compatible with WezTerm's Lua API, with built-in defaults at `Kaku.app/Contents/Resources/kaku.lua` as fallback.

Run `kaku` in your terminal to see all available commands such as `kaku update`, `kaku reset`, and `kaku config`.

## Why Kaku?

I heavily rely on the CLI for both work and personal projects. Tools I've built, like [Mole](https://github.com/tw93/mole) and [Pake](https://github.com/tw93/pake), reflect this.

I used Alacritty for years and learned to value speed and simplicity. As my workflow shifted toward AI-assisted coding, I wanted stronger tab and pane ergonomics. I also explored Kitty, Ghostty, Warp, and iTerm2. Each is strong in different areas, but I still wanted a setup that matched my own balance of performance, defaults, and control.

WezTerm is robust and highly hackable, and I am grateful for its engine and ecosystem. Kaku builds on that foundation with practical defaults for day one use, while keeping full Lua-based customization and a fast, lightweight feel.

So I built Kaku to be that environment: fast, polished, and ready to work.

### Performance

| Metric | Upstream | Kaku | Methodology |
| :--- | :--- | :--- | :--- |
| **Executable Size** | ~67 MB | ~40 MB | Aggressive symbol stripping & feature pruning |
| **Resources Volume** | ~100 MB | ~80 MB | Asset optimization & lazy-loaded assets |
| **Launch Latency** | Standard | Instant | Just-in-time initialization |
| **Shell Bootstrap** | ~200ms | ~100ms | Optimized environment provisioning |

Achieved through aggressive stripping of unused features, lazy loading of color schemes, and shell optimizations.

## FAQ

1. **Why is the Homebrew cask named `kakuku` instead of `kaku`?**

   The name `kaku` conflicts with another package in Homebrew's official repository (an unmaintained music player). `kakuku` is a cute variation that's easy to remember.

2. **Is there a Windows or Linux version?**

   Not at the moment. Kaku is currently macOS-only while we focus on polishing the macOS experience. Windows and Linux versions may come later once the macOS version is mature.

3. **Can I customize the split line between panes?**

   Yes. You can set `split_thickness` in `~/.config/kaku/kaku.lua` to adjust the line width in pixels, default is 2.0. The split line color is controlled by the `split` field in your color scheme.

   ```lua
   config.split_thickness = 1
   ```

4. **Can Kaku use transparent windows on macOS?**

   Yes. You can set `window_background_opacity` and optionally `macos_window_background_blur` in `~/.config/kaku/kaku.lua`. Transparent mode now keeps top/right/bottom padding regions visually consistent to avoid transparent gaps.

## Contributors

Big thanks to all contributors who helped build Kaku. Go follow them! ❤️

<a href="https://github.com/tw93/Kaku/graphs/contributors">
  <img src="./CONTRIBUTORS.svg?v=2" width="1000" />
</a>

## Support

- If Kaku helped you, star the repo or [share it](https://twitter.com/intent/tweet?url=https://github.com/tw93/Kaku&text=Kaku%20-%20A%20fast%20terminal%20built%20for%20AI%20coding.) with friends.
- Got ideas or found bugs? Open an issue/PR or check [CONTRIBUTING.md](CONTRIBUTING.md) for details.
- Like Kaku? <a href="https://miaoyan.app/cats.html?name=Kaku" target="_blank">Buy Tw93 a Coke</a> to support the project! 🥤 Supporters below.

<a href="https://miaoyan.app/cats.html?name=Kaku"><img src="https://miaoyan.app/assets/sponsors.svg" width="1000" loading="lazy" /></a>

## License

MIT License, feel free to enjoy and participate in open source.
