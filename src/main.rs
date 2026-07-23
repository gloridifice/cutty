use anyhow::{Context, Result, anyhow, bail};
use std::env;

#[derive(Debug, PartialEq, Eq)]
struct Args {
    target: Target,
    window_index: Option<usize>,
    list_only: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Pid(u32),
    ProcessName(String),
}

fn main() {
    if let Err(error) = try_main() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let args = parse_args(env::args().skip(1))?;

    #[cfg(windows)]
    {
        windows_capture::run(args)
    }

    #[cfg(not(windows))]
    {
        let _ = args;
        bail!("cutty currently supports Windows only")
    }
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Args> {
    let mut arguments = arguments.into_iter();
    let mut target = None;
    let mut window_index = None;
    let mut list_only = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--pid" => {
                ensure_target_is_unset(&target)?;
                let value = next_value(&mut arguments, "--pid")?;
                let pid = value
                    .parse()
                    .with_context(|| format!("--pid must be a positive integer, got {value:?}"))?;
                if pid == 0 {
                    bail!("--pid must be greater than zero");
                }
                target = Some(Target::Pid(pid));
            }
            "--process" => {
                ensure_target_is_unset(&target)?;
                let name = next_value(&mut arguments, "--process")?;
                if name.is_empty() {
                    bail!("--process cannot be empty");
                }
                target = Some(Target::ProcessName(name));
            }
            "--window" => {
                if window_index.is_some() {
                    bail!("--window may only be supplied once");
                }
                let value = next_value(&mut arguments, "--window")?;
                window_index = Some(value.parse().with_context(|| {
                    format!("--window must be a zero-based integer, got {value:?}")
                })?);
            }
            "--list" => {
                if list_only {
                    bail!("--list may only be supplied once");
                }
                list_only = true;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ => bail!("unknown argument {argument:?}\n\n{}", usage()),
        }
    }

    let target = target.ok_or_else(|| anyhow!("a target is required\n\n{}", usage()))?;
    Ok(Args {
        target,
        window_index,
        list_only,
    })
}

fn ensure_target_is_unset(target: &Option<Target>) -> Result<()> {
    if target.is_some() {
        bail!("specify exactly one of --pid or --process");
    }
    Ok(())
}

fn next_value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    arguments
        .next()
        .ok_or_else(|| anyhow!("{option} requires a value"))
}

fn print_usage() {
    println!("{}", usage());
}

fn usage() -> &'static str {
    "Usage:\n  cutty (--pid PID | --process IMAGE_NAME) [--window INDEX] [--list]\n\nCaptures a visible, non-minimized top-level window as a PNG in the system\ntemporary directory. --process matches an executable filename case-insensitively\n(for example, notepad.exe).\n\nOptions:\n  --pid PID             Target process ID\n  --process IMAGE_NAME  Target executable file name\n  --window INDEX        Choose a zero-based window from --list output\n  --list                List matching windows without taking a screenshot\n  -h, --help            Show this help"
}

#[cfg(windows)]
mod windows_capture {
    use super::{Args, Target};
    use anyhow::{Context, Result, anyhow, bail};
    use image::{DynamicImage, ImageFormat, RgbImage};
    use std::collections::BTreeSet;
    use std::env;
    use std::fs::OpenOptions;
    use std::mem::size_of;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC, HGDIOBJ, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        PW_RENDERFULLCONTENT, SW_HIDE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SetWindowPlacement,
        ShowWindowAsync, WINDOWPLACEMENT, WS_EX_TOOLWINDOW,
    };
    use windows::core::{BOOL, PWSTR};

    static OUTPUT_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[derive(Debug, Clone)]
    struct WindowInfo {
        hwnd: HWND,
        pid: u32,
        title: String,
        is_visible: bool,
        is_minimized: bool,
        is_tool_window: bool,
    }

    pub(super) fn run(args: Args) -> Result<()> {
        enable_per_monitor_dpi_awareness()?;
        let pid = resolve_pid(&args.target)?;
        let mut windows = windows_for_pid(pid)?;
        if windows.is_empty() {
            bail!("process {pid} has no top-level window to capture");
        }

        // EnumWindows is z-order dependent. Prefer an already shown normal application window.
        windows.sort_by_key(|window| {
            (
                !window.is_visible || window.is_minimized,
                window.is_tool_window,
                window.title.is_empty(),
            )
        });

        if args.list_only {
            print_windows(&windows);
            return Ok(());
        }

        let index = args.window_index.unwrap_or(0);
        let window = windows.get(index).ok_or_else(|| {
            anyhow!(
                "window index {index} is out of range; process {pid} has {} matching window(s). Run with --list to inspect them",
                windows.len()
            )
        })?;

        // PrintWindow cannot reliably render a minimized or hidden Chromium window. Show it
        // without activation just long enough to obtain a frame, then restore its exact state.
        let restore_window_state = ensure_window_is_shown(window)
            .with_context(|| format!("could not show window {index} of process {pid}"))?;
        let (width, height, pixels) =
            capture_window(window.hwnd, restore_window_state.is_some())
                .with_context(|| format!("could not capture window {index} of process {pid}"))?;
        let image = RgbImage::from_raw(width, height, pixels)
            .ok_or_else(|| anyhow!("captured pixel buffer has an invalid size"))?;
        let output = temporary_output_path(pid);
        save_png(&image, &output)?;

        println!("{}", output.display());
        Ok(())
    }

    fn enable_per_monitor_dpi_awareness() -> Result<()> {
        // Without this, GetWindowRect is DPI-virtualized when this executable runs on a
        // scaled monitor (for example 125%). PrintWindow then receives a logical-size DC,
        // producing a PNG smaller than the physical window. This must happen before any
        // HWND or DC work in this process.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
            .context("could not enable per-monitor DPI awareness")
    }

    fn resolve_pid(target: &Target) -> Result<u32> {
        match target {
            Target::Pid(pid) => Ok(*pid),
            Target::ProcessName(name) => {
                let pids: Vec<_> = all_top_level_windows()?
                    .into_iter()
                    // A visible minimized window has a user-facing surface; hidden helper
                    // windows do not. This keeps --process unambiguous for multi-process apps.
                    .filter(|window| window.is_visible)
                    .map(|window| window.pid)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .filter(|pid| {
                        process_image_name(*pid)
                            .map(|image_name| image_name.eq_ignore_ascii_case(name))
                            .unwrap_or(false)
                    })
                    .collect();

                match pids.as_slice() {
                    [] => bail!(
                        "no top-level window belongs to a process named {name:?}; use --pid if the process name is not an executable filename"
                    ),
                    [pid] => Ok(*pid),
                    _ => bail!(
                        "process name {name:?} is ambiguous; matching process IDs: {}. Use --pid to choose one",
                        pids.iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            }
        }
    }

    fn process_image_name(pid: u32) -> Result<String> {
        // A 32 Ki UTF-16 buffer is the documented maximum Win32 path length, including long paths.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .with_context(|| format!("cannot inspect process {pid}"))?;
        let mut path = vec![0_u16; 32_768];
        let mut path_length = path.len() as u32;
        let query_result = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut path_length,
            )
        };
        // The handle is no longer needed regardless of whether querying succeeded.
        unsafe { windows::Win32::Foundation::CloseHandle(process) }
            .context("failed to close process handle")?;
        query_result.with_context(|| format!("cannot read executable name for process {pid}"))?;

        let full_path = String::from_utf16_lossy(&path[..path_length as usize]);
        Path::new(&full_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("process {pid} returned an invalid executable path"))
    }

    fn windows_for_pid(pid: u32) -> Result<Vec<WindowInfo>> {
        Ok(all_top_level_windows()?
            .into_iter()
            .filter(|window| window.pid == pid)
            .collect())
    }

    fn all_top_level_windows() -> Result<Vec<WindowInfo>> {
        let mut windows = Vec::new();
        unsafe {
            EnumWindows(
                Some(collect_window),
                LPARAM((&mut windows as *mut Vec<WindowInfo>) as isize),
            )
        }
        .context("failed to enumerate top-level windows")?;
        Ok(windows)
    }

    unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // lparam was created from a mutable Vec in all_top_level_windows and EnumWindows invokes
        // this callback synchronously, so it remains valid for this call.
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return BOOL(1);
        }

        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        windows.push(WindowInfo {
            hwnd,
            pid,
            title: unsafe { window_title(hwnd) },
            is_visible: unsafe { IsWindowVisible(hwnd).as_bool() },
            is_minimized: unsafe { IsIconic(hwnd).as_bool() },
            is_tool_window: ex_style & WS_EX_TOOLWINDOW.0 != 0,
        });
        BOOL(1)
    }

    unsafe fn window_title(hwnd: HWND) -> String {
        let length = unsafe { GetWindowTextLengthW(hwnd) } as usize;
        if length == 0 {
            return String::new();
        }
        let mut buffer = vec![0_u16; length + 1];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) }.max(0) as usize;
        String::from_utf16_lossy(&buffer[..copied])
    }

    fn print_windows(windows: &[WindowInfo]) {
        for (index, window) in windows.iter().enumerate() {
            let kind = if window.is_tool_window {
                "tool"
            } else {
                "normal"
            };
            let state = if window.is_minimized {
                "minimized"
            } else if window.is_visible {
                "shown"
            } else {
                "hidden"
            };
            println!(
                "{index}\tpid={}\ttype={kind}\tstate={state}\ttitle={:?}",
                window.pid, window.title
            );
        }
    }

    fn ensure_window_is_shown(window: &WindowInfo) -> Result<Option<WindowPlacementRestorer>> {
        if window.is_visible && !window.is_minimized {
            return Ok(None);
        }

        let restorer = WindowPlacementRestorer::save(window.hwnd)?;
        // This command restores a minimized window but does not activate it. It can cause a brief
        // repaint, which is necessary for applications (notably Chromium) that do not maintain a
        // capturable surface while minimized.
        unsafe {
            let _ = ShowWindowAsync(window.hwnd, SW_SHOWNOACTIVATE);
        }

        for _ in 0..10 {
            if unsafe { IsWindowVisible(window.hwnd).as_bool() }
                && !unsafe { IsIconic(window.hwnd).as_bool() }
            {
                // Chromium recreates its GPU surface asynchronously after restore. Give it a
                // short paint interval before PrintWindow asks for the frame.
                thread::sleep(Duration::from_millis(300));
                return Ok(Some(restorer));
            }
            thread::sleep(Duration::from_millis(50));
        }

        bail!("window remained hidden or minimized after a non-activating restore request")
    }

    struct WindowPlacementRestorer {
        hwnd: HWND,
        placement: WINDOWPLACEMENT,
        was_visible: bool,
        was_minimized: bool,
    }

    impl WindowPlacementRestorer {
        fn save(hwnd: HWND) -> Result<Self> {
            let mut placement = WINDOWPLACEMENT {
                length: size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            unsafe { GetWindowPlacement(hwnd, &mut placement) }
                .context("cannot save the window placement")?;
            Ok(Self {
                hwnd,
                placement,
                was_visible: unsafe { IsWindowVisible(hwnd).as_bool() },
                was_minimized: unsafe { IsIconic(hwnd).as_bool() },
            })
        }
    }

    impl Drop for WindowPlacementRestorer {
        fn drop(&mut self) {
            // Restoration is best effort: the target process may have exited after capture.
            let _ = unsafe { SetWindowPlacement(self.hwnd, &self.placement) };
            // GetWindowPlacement alone does not reliably restore a minimized state for all
            // Chromium windows, so preserve the observed visibility state explicitly.
            unsafe {
                if self.was_minimized {
                    let _ = ShowWindowAsync(self.hwnd, SW_SHOWMINNOACTIVE);
                } else if !self.was_visible {
                    let _ = ShowWindowAsync(self.hwnd, SW_HIDE);
                }
            }
        }
    }

    fn capture_window(hwnd: HWND, allow_desktop_fallback: bool) -> Result<(u32, u32, Vec<u8>)> {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut rect) }.context("cannot read window bounds")?;
        let width = u32::try_from(rect.right - rect.left).context("window width is invalid")?;
        let height = u32::try_from(rect.bottom - rect.top).context("window height is invalid")?;
        if width == 0 || height == 0 {
            bail!("window has an empty rectangle");
        }
        let width_i32 = i32::try_from(width).context("window is too wide to capture")?;
        let height_i32 = i32::try_from(height).context("window is too tall to capture")?;
        let pixel_count = usize::try_from(width)
            .ok()
            .and_then(|width| {
                usize::try_from(height)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .ok_or_else(|| anyhow!("window is too large to capture"))?;
        let byte_count = pixel_count
            .checked_mul(4)
            .ok_or_else(|| anyhow!("window is too large to capture"))?;

        let surface = CaptureSurface::new(width_i32, height_i32)?;
        let print_window_succeeded = unsafe {
            PrintWindow(
                hwnd,
                surface.memory_dc,
                PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT),
            )
        }
        .as_bool();
        let mut bgra = read_bitmap(&surface, width_i32, height_i32, height, byte_count)?;

        // Chromium-based windows commonly accept PrintWindow yet produce an almost uniform
        // placeholder when restored from minimized state. In that exact case the window was
        // deliberately shown without activation above, so a desktop copy contains its rendered
        // GPU surface. Do not use this fallback for an already-visible background window: it
        // could include whatever happens to occlude that window.
        if allow_desktop_fallback && (!print_window_succeeded || is_likely_placeholder(&bgra)) {
            select_capture_bitmap(&surface)?;
            unsafe {
                BitBlt(
                    surface.memory_dc,
                    0,
                    0,
                    width_i32,
                    height_i32,
                    Some(surface.display_dc),
                    rect.left,
                    rect.top,
                    SRCCOPY,
                )
            }
            .context("could not copy the temporarily shown window from the desktop")?;
            bgra = read_bitmap(&surface, width_i32, height_i32, height, byte_count)?;
        } else if !print_window_succeeded {
            bail!("PrintWindow was rejected by the target window");
        }

        // GDI provides 32-bit BGRX pixels. PNG RGB avoids treating the undefined X byte as alpha.
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        for pixel in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        Ok((width, height, rgb))
    }

    fn select_capture_bitmap(surface: &CaptureSurface) -> Result<()> {
        let selected = unsafe { SelectObject(surface.memory_dc, HGDIOBJ(surface.bitmap.0)) };
        if selected.is_invalid() {
            bail!("could not select the captured bitmap into the memory device context");
        }
        Ok(())
    }

    fn read_bitmap(
        surface: &CaptureSurface,
        width: i32,
        height: i32,
        height_u32: u32,
        byte_count: usize,
    ) -> Result<Vec<u8>> {
        // GetDIBits requires its bitmap not to be selected into a device context.
        let selected_bitmap = unsafe { SelectObject(surface.memory_dc, surface.previous_bitmap) };
        if selected_bitmap.is_invalid() {
            bail!("could not detach the captured bitmap from the memory device context");
        }

        let mut info = bitmap_info(width, -height);
        let mut bgra = vec![0_u8; byte_count];
        let lines = unsafe {
            GetDIBits(
                surface.display_dc,
                surface.bitmap,
                0,
                height_u32,
                Some(bgra.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if lines != height {
            bail!("could only read {lines} of {height} bitmap rows");
        }
        Ok(bgra)
    }

    fn is_likely_placeholder(bgra: &[u8]) -> bool {
        let Some(reference) = bgra.chunks_exact(4).nth(bgra.len() / 8) else {
            return true;
        };
        let total = bgra.len() / 4;
        let similar = bgra
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[..3]
                    .iter()
                    .zip(&reference[..3])
                    .all(|(value, reference)| value.abs_diff(*reference) <= 4)
            })
            .count();
        similar * 100 >= total * 98
    }

    fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
        BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    struct CaptureSurface {
        display_dc: HDC,
        memory_dc: HDC,
        bitmap: HBITMAP,
        previous_bitmap: HGDIOBJ,
    }

    impl CaptureSurface {
        fn new(width: i32, height: i32) -> Result<Self> {
            let display_dc = unsafe { GetDC(None) };
            if display_dc.is_invalid() {
                bail!("could not acquire a display device context");
            }

            let memory_dc = unsafe { CreateCompatibleDC(Some(display_dc)) };
            if memory_dc.is_invalid() {
                unsafe { ReleaseDC(None, display_dc) };
                bail!("could not create a memory device context");
            }

            let bitmap = unsafe { CreateCompatibleBitmap(display_dc, width, height) };
            if bitmap.is_invalid() {
                unsafe {
                    let _ = DeleteDC(memory_dc);
                    ReleaseDC(None, display_dc);
                }
                bail!("could not allocate a bitmap for the window");
            }

            let previous_bitmap = unsafe { SelectObject(memory_dc, HGDIOBJ(bitmap.0)) };
            if previous_bitmap.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(bitmap.0));
                    let _ = DeleteDC(memory_dc);
                    ReleaseDC(None, display_dc);
                }
                bail!("could not select the bitmap into the memory device context");
            }

            Ok(Self {
                display_dc,
                memory_dc,
                bitmap,
                previous_bitmap,
            })
        }
    }

    impl Drop for CaptureSurface {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.memory_dc, self.previous_bitmap);
                let _ = DeleteObject(HGDIOBJ(self.bitmap.0));
                let _ = DeleteDC(self.memory_dc);
                ReleaseDC(None, self.display_dc);
            }
        }
    }

    fn temporary_output_path(pid: u32) -> PathBuf {
        let milliseconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let sequence = OUTPUT_COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "cutty-{pid}-{milliseconds}-{}-{sequence}.png",
            std::process::id()
        ))
    }

    fn save_png(image: &RgbImage, output: &Path) -> Result<()> {
        // create_new prevents a very unlikely name collision from overwriting an existing temp file.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .with_context(|| format!("cannot create {}", output.display()))?;
        if let Err(error) =
            DynamicImage::ImageRgb8(image.clone()).write_to(&mut file, ImageFormat::Png)
        {
            let _ = std::fs::remove_file(output);
            return Err(error).with_context(|| format!("cannot write {}", output.display()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Args, Target, parse_args};

    #[test]
    fn parses_pid_target_and_options() {
        assert_eq!(
            parse_args(["--pid", "42", "--window", "3", "--list"].map(str::to_owned)).unwrap(),
            Args {
                target: Target::Pid(42),
                window_index: Some(3),
                list_only: true,
            }
        );
    }

    #[test]
    fn parses_process_target() {
        assert_eq!(
            parse_args(["--process", "notepad.exe"].map(str::to_owned)).unwrap(),
            Args {
                target: Target::ProcessName("notepad.exe".to_owned()),
                window_index: None,
                list_only: false,
            }
        );
    }

    #[test]
    fn rejects_multiple_targets() {
        assert!(
            parse_args(["--pid", "42", "--process", "notepad.exe"].map(str::to_owned)).is_err()
        );
    }
}
