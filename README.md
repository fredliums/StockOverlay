# StockOverlay V0.1

Windows 自选股行情悬浮窗。悬浮窗只显示已添加的股票；搜索、添加、删除、排序和显示设置都在系统托盘的“设置”中完成。行情来自腾讯公开接口。

## 使用

- 默认 `Ctrl+Alt+S` 显示或隐藏悬浮窗，`Ctrl+Alt+L` 锁定或解锁。快捷键可在设置中修改。
- 未锁定时，按住悬浮窗内容拖动，从边缘调整大小。锁定后窗口允许鼠标穿透；可用快捷键或托盘“解锁”恢复操作。
- 关闭悬浮窗会隐藏到托盘；通过托盘“退出”结束程序。
- 设置中的盘口数量单位为“手”。一档顺序为卖一 / 买一。行情源不可用时保留上次报价并显示逐股状态；恢复连接后自动刷新。
- 配置位于 `%APPDATA%\StockOverlay\config.json`。独立 EXE 仍使用此位置，卸载安装包前可备份该文件。

## 运行条件

发布目标为 Windows 10 1803 或更新版本、Windows 11，x64。程序需要 Microsoft Edge WebView2 Runtime。独立 EXE 不包含运行时；NSIS 安装包在缺少运行时时在线下载安装，因此首次安装可能需要网络连接。参见 [Tauri 的 Windows 安装说明](https://v2.tauri.app/distribute/windows-installer/) 和 [WebView2 前提](https://v2.tauri.app/start/prerequisites/)。

## 从源码构建

需要 Node.js、npm、Rust stable 的 `x86_64-pc-windows-msvc` 工具链，以及 Visual Studio C++ Build Tools。仓库根目录执行：

```powershell
npm ci
npm run test:unit
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run tauri -- build --bundles nsis --ci
New-Item -ItemType Directory -Force release | Out-Null
Copy-Item src-tauri/target/release/stock-overlay.exe release/StockOverlay.exe
Copy-Item src-tauri/target/release/bundle/nsis/StockOverlay_0.1.0_x64-setup.exe release/StockOverlay-Setup.exe
```

`release/` 下的两个文件分别为独立程序和安装包，不纳入 Git。安装包按当前配置采用 WebView2 在线 bootstrapper；构建时 Tauri 可能下载 NSIS 工具。

## 当前已知限制

- 当前验证机器只有单显示器，多显示器拔插行为尚未实机验收。
- 悬浮窗左右边缘可调整宽度；上下边缘在当前版本可能无法调整高度。
- 发行物的干净环境安装与卸载、以及连续 8 小时运行验收尚未完成。
