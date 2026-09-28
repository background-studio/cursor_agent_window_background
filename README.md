# Cursor Agent Background Studio

Background Studio 的 Cursor Agent 窗口背景插件，版本 0.1.2。

它只给带 `.agent-panel` 的 Agent 窗口贴背景。编辑器窗口不注入。调试口只监听 `127.0.0.1`，用户目录仍用原来的 `%APPDATA%\Cursor`，不另开空配置。

已经开着、且没有调试口的 Cursor 不会被自动关掉。点「应用」，或在插件启用之后重新打开 Cursor，才会用同一份登录配置带上调试口重启。

0.1.0 的默认是全透：侧栏、聊天区、文件区、气泡和输入框底色都是 0。背景图最大 16 MB。
