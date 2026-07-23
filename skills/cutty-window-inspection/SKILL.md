---
name: cutty-window-inspection
description: Captures Windows application windows with cutty and visually inspects the resulting PNG with the image-capable read tool. Use whenever a task requires seeing, checking, debugging, or verifying the current visual state of a desktop window or GUI, including layout, rendering, dialogs, controls, and screenshots.
compatibility: Windows only; requires cutty on PATH or a built target/release/cutty.exe.
---

# Visual window inspection with cutty

Use this workflow whenever visual evidence from a Windows application is needed. Do not guess a window's appearance from source code, logs, accessibility text, or window titles when a screenshot can answer the question.

## Capture and inspect

1. Identify the target by PID when known. Otherwise use its executable filename (for example, `notepad.exe`, not a window title).
2. If the process may have multiple windows, list them first:

   ```bash
   cutty --pid <PID> --list
   # or, when exactly one matching process instance exists:
   cutty --process <IMAGE_NAME.exe> --list
   ```

3. Capture the desired window. Omit `--window` to use cutty's preferred normal titled window, or pass the zero-based index shown by `--list`:

   ```bash
   cutty --pid <PID> --window <INDEX>
   # or
   cutty --process <IMAGE_NAME.exe>
   ```

   If `cutty` is not on `PATH` while working in its repository, use `target/release/cutty.exe` instead. Build it first with `cargo build --release` if necessary.

4. The successful command's stdout is the absolute path of a temporary PNG. Immediately call the `read` tool on that exact path. The command alone does not inspect the image.
5. Report only observations supported by the image. Distinguish visible facts from hypotheses.
6. After an interaction, rebuild, reload, resize, or other visual change, take a new screenshot and read the new PNG; do not rely on an earlier capture.

## Choosing the target

- Prefer `--pid` when several instances of the same executable are running.
- If `--process` reports ambiguity, obtain the intended PID and retry with `--pid`; do not choose an instance arbitrarily.
- Use `--list` and select `--window <INDEX>` when the default capture is not the relevant window.
- If the intended process or window cannot be identified safely, ask the user for its executable name, PID, or distinguishing window title.

## Safety and limitations

- Do not activate, focus, move, close, or otherwise manipulate a window merely to inspect it. Cutty normally captures without taking focus and temporarily restores hidden or minimized windows itself when required.
- A protected, elevated, unresponsive, GPU-rendered, or otherwise unsupported window may fail or yield an unusable image. State the limitation rather than claiming a visual result.
- Screenshots are written to the system temporary directory. They may contain sensitive information; do not copy, publish, or retain them unnecessarily.
