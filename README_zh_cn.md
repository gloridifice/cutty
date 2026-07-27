# cutty

Windows 命令行截图工具：可按 PID 或进程的可执行文件名定位其顶层窗口，也可截取整个显示器，并将 PNG **始终写入系统临时目录**。成功时标准输出只会打印生成文件的完整路径，方便脚本直接接收。

## 构建

需要 Windows 和稳定版 Rust：

```powershell
cargo build --release
```

生成文件为 `target\release\cutty.exe`。

## 使用

每个选项都提供短形式：`--pid` / `-p`、`--process` / `-P`、`--window` / `-w`、`--monitor` / `-m`、`--resize` / `-r`、`--resize-vertical` / `-R` 和 `--list` / `-l`。Clap 还提供 `--help` / `-h` 与 `--version` / `-V`。

使用 `--monitor <INDEX>` 可截取指定显示器的完整可见桌面。`0` 始终是主显示器；其余显示器按虚拟桌面位置排序，先上后下、同一高度先左后右。它不能与进程目标选项、`--window` 或 `--list` 一起使用：

```powershell
cutty --monitor 0
cutty --monitor 1 -r 1280b
```

显示器截图包含当前显示在该屏幕上的窗口；受保护内容和硬件叠加层仍可能无法截取。

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

## 验证

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
