```
             _   _
    __ _  _| |_| |_ _  _
   / _| || |  _|  _| || |
   \__|\_,_|\__|\__|\_, |
                    |__/
```

`cutty` is a lightweight Windows CLI screenshot tool that captures a process window or monitor and outputs the temporary PNG path.

`cutty` is agent friendly, when using `cutty`'s skill, `cutty` will automatically resize the output image into low resolution for saving your token.

## Quick Start

Download `cutty.exe` from the [releases page](https://github.com/gloridifice/cutty/releases), or [build it yourself](#build-from-source).

### Capture a Window

Capture the preferred titled window for a process by executable name or PID:

```powershell
cutty -P foo.exe # capture window by [P]rocess name -P = --process
cutty -p 1234    # capture window by [p]id          -p = --pid
cutty -m 0       # capture desktop/[m]onitor        -m = --monitor
```

On success, cutty prints only the absolute path of the PNG, for example:

```text
C:\Users\alice\AppData\Local\Temp\cutty-1234-1740000000000-5678-0.png
```

When a process has multiple windows, list its candidate windows first, then capture one by its zero-based index. Replace `1234` with the target process's PID:

```powershell
cutty --pid 1234 --list
cutty --pid 1234 --window 0 # or -w
```

You can also use `cutty --process foo.exe --list` to list candidate windows when exactly one matching process instance is running.

### Capture a Desktop Monitor

Capture the complete visible desktop of one monitor with `--monitor` / `-m`. Monitor `0` is always the primary display. Additional monitors are ordered by virtual-desktop position (top to bottom, then left to right), so the index is stable regardless of Windows' enumeration order:

```powershell
.\cutty.exe --monitor 0
.\cutty.exe --monitor 1 -r 1280b
```

The monitor capture includes the windows currently visible on that display. Protected content and hardware overlays may still be absent.

### Agent Skill

Download the skill files from the [releases page](https://github.com/gloridifice/cutty/releases) or clone this repository. Then copy `skills\cutty-window-inspection` to your agent's skills directory. For agents that use the shared `~\.agents\skills` directory:

```powershell
Copy-Item -Recurse .\skills\cutty-window-inspection ~\.agents\skills\cutty-window-inspection
```

## Build from Source

Requires Windows and stable Rust:

```powershell
cargo build --release
```

The resulting executable is `target\release\cutty.exe`.

## Arguments

Short forms are available for every option: `--pid` / `-p`, `--process` / `-P`, `--window` / `-w`, `--monitor` / `-m`, `--resize` / `-r`, `--resize-vertical` / `-R`, and `--list` / `-l`. Clap also provides `--help` / `-h` and `--version` / `-V`.

Use `--monitor <INDEX>` to capture an entire display monitor. Monitor `0` is primary; additional monitors are ordered by virtual-desktop position, top to bottom and then left to right. It is mutually exclusive with the process target options, `--window`, and `--list`:

```powershell
cutty --monitor 0
cutty --monitor 1 -r 1280b
```

First, list the candidate windows for a process:

```powershell
cutty --pid 1234 --list
cutty --process notepad.exe --list
```

Capture a window by its index in the list:

```powershell
cutty --pid 1234 --window 0
```

Use `-r` / `--resize` to resize the image before saving it as a PNG. The `x` descriptor scales both dimensions proportionally; `h` sets the target height and adjusts the width to preserve the original aspect ratio; `w` sets the target width and adjusts the height automatically. `s` sets the shorter side to the target pixel value, while `b` sets the longer side:

```powershell
cutty --pid 1234 -r 0.5x # Scale both width and height to 0.5 of their original size
cutty --pid 1234 -r 640h # Set height to 640 pixels and preserve the aspect ratio
cutty --pid 1234 -r 640w # Set width to 640 pixels and preserve the aspect ratio
cutty --pid 1234 -r 540s # Set the shorter side to 540 pixels
cutty --pid 1234 -r 1280b # Set the longer side to 1280 pixels
```

Expressions can combine multiple resizing constraints. `min(A, B)` selects the result with the smaller output height, while `max(A, B)` selects the one with the larger output height. Every mode preserves the original aspect ratio, so comparing heights is sufficient. Expressions require at least two values and can be nested:

```powershell
cutty --pid 1234 -r "min(0.5x, 640h)"
cutty --pid 1234 -r "max(0.5x, 640h)"
```

`-R` / `--resize-vertical` accepts exactly the same descriptors and expressions as `--resize`. When both are provided, `--resize` applies only to landscape screenshots and `--resize-vertical` only to portrait screenshots; square screenshots are not resized. If only one is provided, it applies regardless of screenshot orientation:

```powershell
cutty --pid 1234 -r "min(0.5x, 1280w)" -R "max(0.5x, 960h)"
```

Resize factors and pixel values must be greater than zero. Neither `--resize` nor `--resize-vertical` may be used with `--list`, which only lists windows. Without `--window`, the tool captures the first ordinary candidate window with a title. Example output:

```text
C:\Users\alice\AppData\Local\Temp\cutty-1234-1740000000000-5678-0.png
```

The process name must be an executable filename, such as `notepad.exe`, rather than a window title. If several same-named processes have windows, use `--process ... --list` to inspect them, then use `--pid` to resolve the ambiguity.

For complete help:

```powershell
cutty --help
```

## Verification

```powershell
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
