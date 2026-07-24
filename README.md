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

可使用 `-r` / `--resize` 在保存 PNG 前缩放图像。描述符 `x` 按比例缩放宽高，`h` 指定目标高度、宽度按原始宽高比适应，`w` 则指定目标宽度、高度自动适应。`s` 将较短边设为目标像素值，`b` 将较长边设为目标像素值：

```powershell
cutty --pid 1234 -r 0.5x # 宽和高均缩放为原来的 0.5 倍
cutty --pid 1234 -r 640h # 高度为 640 像素，宽度保持比例
cutty --pid 1234 -r 640w # 宽度为 640 像素，高度保持比例
cutty --pid 1234 -r 540s # 较短边为 540 像素
cutty --pid 1234 -r 1280b # 较长边为 1280 像素
```

可用表达式组合多个缩放限制。`min(A, B)` 取输出高度较小的结果，`max(A, B)` 取输出高度较大的结果；所有模式均保持原始宽高比，因此比较高度即可。表达式至少需要两个值，且可嵌套：

```powershell
cutty --pid 1234 -r "min(0.5x, 640h)"
cutty --pid 1234 -r "max(0.5x, 640h)"
```

`-R` / `--resize-vertical` 接受与 `--resize` 完全相同的描述符和表达式。两者都提供时，`--resize` 只用于宽大于高的横向截图，`--resize-vertical` 只用于高大于宽的纵向截图；正方形截图不缩放。若只提供其中一个参数，则不论截图方向均使用该参数：

```powershell
cutty --pid 1234 -r "min(0.5x, 1280w)" -R "max(0.5x, 960h)"
```

缩放比例和像素值必须大于零；`--resize` 和 `--resize-vertical` 都不能和只列出窗口的 `--list` 一同使用。未传 `--window` 时，工具优先截取第一个普通、有标题的候选窗口。输出示例：

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
