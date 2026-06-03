# ZenScroll Project Rules

## Universal Principles

### Immutability
ALWAYS create new objects, NEVER mutate existing ones. Immutable data prevents hidden side effects and enables safe concurrency.

### KISS (Keep It Simple)
Prefer the simplest solution that actually works. Avoid premature optimization. Optimize for clarity over cleverness.

### DRY (Don't Repeat Yourself)
Extract repeated logic into shared functions or utilities. Avoid copy-paste implementation drift.

### YAGNI (You Aren't Gonna Need It)
Do not build features or abstractions before they are needed. Start simple, then refactor when the pressure is real.

## File Organization
MANY SMALL FILES > FEW LARGE FILES. 200-400 lines typical, 800 max. Organize by feature/domain, not by type.

## Error Handling
ALWAYS handle errors comprehensively. Never silently swallow errors. Provide context with .context() / .with_context().

## Code Quality Checklist
- Code is readable and well-named
- Functions are small (<50 lines)
- Files are focused (<800 lines)
- No deep nesting (>4 levels)
- Proper error handling — no unwrap() in production
- No hardcoded values (use constants or config)
- cargo clippy -- -D warnings passes
- cargo fmt has been run
- cargo test passes
- Every unsafe block has a // SAFETY: comment

## ZenScroll-Specific Rules

### unsafe blocks
- Wrap ONLY the exact FFI call, never surrounding Rust-safe code
- Each call site gets its own unsafe {} block
- Must have // SAFETY: comment

### IPC Architecture
- UI writes config.json + sends PostMessageW(WM_APP) to daemon
- Daemon receives WM_APP -> config::reload() -> updates state
- Never use named pipes or sockets for IPC

### Daemon State
- DAEMON_CONFIG (Mutex<DaemonConfig>) is the single source of truth
- config::reload() refreshes it from file after each config write
- Hook and tray read from the static, never direct file I/O

### Language
- 所有注释、字符串、日志等能用中文的地方尽量用中文
- 英文仅用于 Rust 标准库/外部 crate 的标识符、关键词等无法避免的场合
- Rust 标识符（结构体/函数/变量/常量/枚举名）保持英文，遵循 Rust 生态惯例
  - 例外：如果某概念没有自然的中文翻译或英文原名已广泛使用（如 `momentum`、`bounce`），优先保持英文
- 用户可见字符串（托盘菜单、UI 标签等）必须用中文
- 日志消息（eprintln!、debug_log!、println!）用中文
- 注释（SAFETY、文档注释、行内注释）说明性文字用中文，但 Win32 API 名等专有名词保留英文以保证精确性

### Physics
- Global speed presets (PRESETS: [ScrollConfig; 3]) define all physics params
- Profiles only define process targeting, not ScrollConfig
- Smartwheel friction: friction + (smartwheel_friction_max - friction) * speed_ratio^3
