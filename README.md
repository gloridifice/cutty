# cutty

Windows 命令行截图工具：按 PID 或进程的可执行文件名定位其顶层窗口，并将 PNG **始终写入系统临时目录**。成功时标准输出只会打印生成文件的完整路径，方便脚本直接接收。

## 调研与方案

Windows 并没有一个“按进程截图”的单一 API：一个进程可以没有窗口，也可以同时拥有多个顶层窗口。因此本工具分成两步：

1. `EnumWindows` 枚举顶层窗口，使用 `GetWindowThreadProcessId` 归属到进程；`--process` 再以 `QueryFullProcessImageNameW` 取得的可执行文件名（不区分大小写）匹配。名称命中多个 PID 时拒绝猜测，要求调用方改用 `--pid`。
2. 对选定的可见、非最小化窗口创建内存 DC，用 `PrintWindow(..., PW_RENDERFULLCONTENT)` 绘制窗口，再经 `GetDIBits` 读为 BGRX 像素并编码为 PNG。

选择 `PrintWindow` 而不是从桌面 `BitBlt` 复制屏幕区域，是因为后者会把遮挡窗口、其他窗口或桌面一起截进去；`PrintWindow` 请求目标窗口自行渲染，通常可以截取被遮挡、未置前的传统 Win32 窗口，因此不会为了截图激活或抢焦点。`PW_RENDERFULLCONTENT` 会请求支持它的窗口绘制完整内容。

工具在任何窗口/DC 调用之前设为 **Per-Monitor DPI Awareness V2**。否则在 125%、150% 等缩放屏幕上，`GetWindowRect` 会被 DPI 虚拟化为逻辑像素，而 `PrintWindow` 的 DC 以该逻辑尺寸创建，导致输出 PNG 比真实窗口小一个缩放比例。现在矩形与位图均使用目标显示器的物理像素，PNG 尺寸应与窗口的物理 `GetWindowRect` 尺寸相同。

此方案没有使用交互式的 Windows Graphics Capture picker，因而适合非交互 CLI，也不需要屏幕录制授权。对于已经显示的普通窗口，工具只使用 `PrintWindow`，因此可以保持其不在前台。若目标窗口最小化或隐藏，工具会以 `SW_SHOWNOACTIVATE` **临时恢复而不激活**，等待重绘后在完成或失败时恢复原窗口状态。

`PrintWindow` 是协作式 API：某些 GPU/Chromium/UWP 窗口会返回成功但只给出单色占位图。对临时恢复的窗口，工具会检测这种近乎单色的结果，回退到桌面 `BitBlt` 复制已重绘的物理区域；这会让该窗口短暂显示在屏幕上，但不会激活它。该回退不会用于原本已经显示的背景窗口，避免把遮挡它的其他窗口错误截入。受保护内容、提升权限的目标或无响应窗口仍可能无法捕获。

## 构建

需要 Windows 和稳定版 Rust：

```powershell
cargo build --release
```

生成文件为 `target\release\cutty.exe`。

## 使用

先列出指定进程的候选窗口：

```powershell
cutty --pid 1234 --list
cutty --process notepad.exe --list
```

从列表中按序号截图：

```powershell
cutty --pid 1234 --window 0
```

未传 `--window` 时，工具优先截取第一个普通、有标题的候选窗口。输出示例：

```text
C:\Users\alice\AppData\Local\Temp\cutty-1234-1740000000000-5678-0.png
```

进程名必须是可执行文件名，例如 `notepad.exe`，而不是窗口标题。若同名进程存在多个有窗口的实例，先用 `--process ... --list` 查阅也仍需改用 `--pid` 消除歧义。

完整帮助：

```powershell
cutty --help
```

## 参考资料

- [EnumWindows function](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumwindows)
- [PrintWindow function](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-printwindow)
- [ShowWindow function](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-showwindow)
- [BitBlt function](https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-bitblt)
- [GetDIBits function](https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-getdibits)（位图仍被选入 DC 时不能读取）
- [SetProcessDpiAwarenessContext function](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext)
- [Screen capture - Windows apps](https://learn.microsoft.com/windows/uwp/audio-video-camera/screen-capture)

## 验证

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
