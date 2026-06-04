# ZenScroll native Win32 build

This directory contains a low-memory native Win32 implementation of ZenScroll.

## Why

The Rust/GPUI UI is visually rich but keeps a much larger working set while the
program is open. This implementation keeps the user-facing behavior in a single
small Win32 GUI process and has been measured by the requester at about 0.9 MB
working set on Windows.

## Features

- Single `.exe` GUI process.
- Dark card-style control panel.
- Tray icon with enable/disable, autostart, show control panel, and quit actions.
- Starts hidden in the system tray by default.
- Global low-level mouse wheel hook.
- Smooth wheel injection for the window under the cursor, not just the focused
  browser window.
- Explorer-specific ListView pixel scrolling path for smoother folder scrolling.
- Config persisted at `%APPDATA%\\ZenScroll\\config.json`.
- Autostart toggled through a `ZenScroll` logon scheduled task with an HKCU Run fallback.

## Build from macOS with mingw-w64

```bash
cd native-win32
x86_64-w64-mingw32-windres app.rc -O coff -o app.res
x86_64-w64-mingw32-gcc -Os -s -mwindows -municode -Wall -Wextra -Werror \
  ZenScroll.c app.res -o ZenScroll.exe \
  -luser32 -lshell32 -lgdi32 -ladvapi32 -lkernel32
```

The application manifest requests administrator privileges because global input
hooks and injected wheel events should run with a consistent integrity level.
