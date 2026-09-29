# Cursor Agent Background Studio

Background Studio 的 Cursor Agent 窗口背景插件，版本 0.1.4-beta.1。

[本版发布说明](docs/releases/0.1.4-beta.1.md)。独立窗口媒体分发需要配套 Background Studio 0.3.6-beta.2 或更高版本。

它只给带 `.agent-panel` 的 Agent 窗口贴背景。编辑器窗口不注入。调试口只监听 `127.0.0.1`，用户目录仍用原来的 `%APPDATA%\Cursor`，不另开空配置。

兼容从 IDE 右上角打开、不带 `bc-window` 类的新版独立 Agent 窗口（通过 Agent 面板与 Glass 菜单栏识别）。插件运行期间自动检查新窗口和丢失的背景层，无需对每个窗口重新应用。

Windows 标题栏使用原生接口支持的 RGBA 背景色，避免 Cursor 的 OKLCH 颜色阻止原生按钮高度更新；检测到异常的整窗高按钮区域时，使用 CSS 像素触发一次有界的高度重同步，随后恢复菜单栏原来的尺寸样式。暂停时取消未完成的更新并恢复原有行内背景样式。

已经开着、且没有调试口的 Cursor 不会被自动关掉。点「应用」，或在插件启用之后重新打开 Cursor，才会用同一份登录配置带上调试口重启。

装了 IDE 背景插件 background-cover 时，它在 Agent 窗口里铺的那层图会在本插件激活期间被隐藏，暂停或恢复后自动回来；编辑器窗口不受影响。

## 多窗口独立轮播

与支持 `perWindowMedia` 的新版 Background Studio 宿主配合时，每个 Agent 窗口独立选图、独立计时；共用 Profile 的图库、显示设置、轮播顺序与间隔。文件夹源和播放列表都可使用，新开窗口不会改变已有窗口的图片。优先选择其他窗口未使用、且不是自己上一张的文件；图库不足时允许重复，只有一张图时保持该图。

插件每 2 秒发现 Agent 窗口，宿主每 2 秒检查分配与轮播，因此新窗口通常在约 2–4 秒加媒体下载时间后获得背景。重建或丢失的背景层会使用该窗口原来的配置恢复。关闭窗口后清理其媒体状态；暂停/恢复会清除注入并暂停分发，不会被轮询重新启用。

新版 worker 的 `hello.capabilities.perWindowMedia` 为 `true`。宿主通过 `status.targetIds` 发现 Agent，通过带 `independentWindows: true`、`targetId` 的 `configure` 定向更新。没有 `targetId` 的独立模式配置不覆盖已分配的窗口。旧宿主仍可使用原来的单媒体协议，但所有窗口同步显示同一张图；要使用独立轮播必须同时更新宿主与插件。

独立轮播不让网页或 worker 读取本机文件路径，不预下载整个图库；每次仍校验宿主回环 URL、MIME、大小和 SHA-256。能力声明的单媒体上限修正为既有内联实现实际支持的 16 MiB。

0.1.0 的默认是全透：侧栏、聊天区、文件区、气泡和输入框底色都是 0。背景图最大 16 MB。

开发验证：安装 Rust 和 Node.js 后运行 `cargo test --manifest-path src-tauri/Cargo.toml`。其中 JavaScript 回归测试会执行 Rust 实际生成的注入脚本，覆盖新版/旧版窗口识别、IDE 隔离、背景层恢复、缩放下的按钮高度同步及暂停清理。
