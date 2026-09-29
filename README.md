# Cursor Agent Background Studio

Background Studio 的 Cursor Agent 窗口背景插件，版本 0.1.3。

它只给带 `.agent-panel` 的 Agent 窗口贴背景。编辑器窗口不注入。调试口只监听 `127.0.0.1`，用户目录仍用原来的 `%APPDATA%\Cursor`，不另开空配置。

兼容从 IDE 右上角打开、不带 `bc-window` 类的新版独立 Agent 窗口（通过 Agent 面板与 Glass 菜单栏识别）。插件运行期间自动检查新窗口和丢失的背景层，无需对每个窗口重新应用。

Windows 标题栏使用原生接口支持的 RGBA 背景色，避免 Cursor 的 OKLCH 颜色阻止原生按钮高度更新；检测到异常的整窗高按钮区域时，使用 CSS 像素触发一次有界的高度重同步，随后恢复菜单栏原来的尺寸样式。暂停时取消未完成的更新并恢复原有行内背景样式。

已经开着、且没有调试口的 Cursor 不会被自动关掉。点「应用」，或在插件启用之后重新打开 Cursor，才会用同一份登录配置带上调试口重启。

装了 IDE 背景插件 background-cover 时，它在 Agent 窗口里铺的那层图会在本插件激活期间被隐藏，暂停或恢复后自动回来；编辑器窗口不受影响。

0.1.0 的默认是全透：侧栏、聊天区、文件区、气泡和输入框底色都是 0。背景图最大 16 MB。

开发验证：安装 Rust 和 Node.js 后运行 `cargo test --manifest-path src-tauri/Cargo.toml`。其中 JavaScript 回归测试会执行 Rust 实际生成的注入脚本，覆盖新版/旧版窗口识别、IDE 隔离、背景层恢复、缩放下的按钮高度同步及暂停清理。
