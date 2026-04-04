# V0.9.0 Spark ✨

<div align="center">
  <img src="https://raw.githubusercontent.com/tw93/Kaku/main/assets/logo.png" alt="Kaku Logo" width="120" height="120" />
  <h1 style="margin: 12px 0 6px;">Kaku V0.9.0</h1>
  <p><em>A fast, out-of-the-box terminal built for AI coding.</em></p>
</div>

### Changelog

1. **Natural Language to Command**: Type `# <description>` at the prompt, press Enter, and Kaku injects the generated command back ready to run. Saved to shell history. Works in zsh and fish.
2. **Option+Click Cursor Movement**: Click anywhere on the current line to move the cursor to that position. Wide characters and multi-byte input are handled correctly.
3. **Always on Top**: Pin any window above others via the Window menu. Toggle on or off at any time.
4. **Traffic Lights Position**: New `traffic_lights` option in Settings to customize the macOS window control button position.
5. **Performance**: Tab title Lua callbacks are batched with a single Config serialization pass. kaku-remote screen capture is throttled to 60 fps.
6. **Stability Fixes**: Fixed a crash on Option+Click, divide-by-zero in split pane sizing, and unwrap panic in mouse event handling.
7. **Shell and Assistant**: Added MiniMax as a built-in provider preset. Fixed zsh-z update display and heredoc quoting edge cases.

### 更新日志

1. **自然语言生成命令**：输入 `# <描述>` 后按回车，Kaku 将生成的命令注入回提示符，确认后即可运行，并保存到 shell 历史，支持 zsh 和 fish。
2. **Option+Click 移动光标**：点击当前行任意位置即可将光标移动到该位置，正确处理宽字符和多字节输入。
3. **窗口置顶**：通过 Window 菜单将窗口固定在最前，随时可切换开关。
4. **Traffic Lights 位置**：设置中新增 `traffic_lights` 选项，可自定义 macOS 窗口控制按钮的位置。
5. **性能优化**：Tab 标题 Lua 回调改为批量处理，单次序列化 Config；kaku-remote 屏幕捕获限速至 60fps。
6. **稳定性修复**：修复 Option+Click 崩溃、分屏尺寸计算除零、鼠标事件 unwrap panic。
7. **Shell 与 Assistant**：内置新增 MiniMax provider preset；修复 zsh-z 更新显示和 heredoc 引号边界问题。

Special thanks to @fanweixiao and @LanternCX for their contributions to this release.

> https://github.com/tw93/Kaku
