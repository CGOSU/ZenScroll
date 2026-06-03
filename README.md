# ZenScroll

**系统级鼠标滚轮平滑滚动优化器**

通过 `WH_MOUSE_LL` 全局钩子拦截鼠标滚轮事件，注入自定义物理引擎驱动的平滑滚动，取代 Chrome / Readest / Firefox 等应用的原生滚动。

![License](https://img.shields.io/badge/license-MIT-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)

---

## 架构

```
ZenScroll/
├── zen-scroll-core/          # 物理引擎（库）
├── zen-scroll-daemon/        # 系统守护进程（需要管理员权限）
│   ├── hook.rs               #   WH_MOUSE_LL 钩子安装与回调
│   ├── detect.rs             #   前台进程识别（GetForegroundWindow）
│   ├── smoother.rs           #   物理注入（SendInput 模拟硬件滚动）
│   ├── profile.rs            #   内置应用匹配表 + 自定义覆盖
│   ├── config.rs             #   配置持久化 + 全局配置静态 + IPC 重载
│   ├── log.rs                #   调试日志（时间戳 + 运行时开关）
│   ├── tray.rs               #   系统托盘图标 + WM_APP IPC 处理
│   └── main.rs               #   入口 + 消息泵
└── zen-scroll-ui/            # 用户控制面板（GPU 加速，无权限要求）
    └── main.rs               #   三档速度预设 + 配置文件读写 + 守护进程 IPC
```

### zen-scroll-core

物理仿真引擎，含三个核心组件：

- **`ScrollConfig`** — 可调物理参数：摩擦力(`friction`)、Smartwheel 低速/高速摩擦、回弹力(`bounce_tension`)、加速度(`scroll_accel`)、最大/最小速度
- **`PhysicsState`** — 三阶段状态机：`Idle → Momentum → Bouncing`，离散时间步进积分
- **`Scroller`** — 高阶封装，管理双轴(`ScrollAxis`)物理状态 + 视口/内容尺寸边界

**插件系统** — 基于 `Plugin` trait + `PluginManager` 的扩展机制：
- `PullToRefresh` — 下拉刷新
- `InfiniteScroll` — 触底加载更多  
- `Snap` — 滚动吸附到指定位置
- `Scrollbar` — 自定义滚动条尺寸计算

### zen-scroll-daemon

Windows 系统级服务，工作在应用之外：

1. **`hook.rs`** — `SetWindowsHookExW(WH_MOUSE_LL)` 安装全局低级别鼠标钩子，拦截原始滚动事件
2. **`detect.rs`** — `GetForegroundWindow` → `GetWindowThreadProcessId` → `QueryFullProcessImageNameW` 识别前台进程名
3. **`smoother.rs`** — `SmoothInjector` 将原始 120 刻度值转为指数衰减速度，通过 `SendInput` 以 8ms 间隔注入硬件级滚动（兼容 Chrome Raw Input）
4. **`profile.rs`** — 内置 Chrome / Readest / Firefox 三组应用匹配规则 + 自定义进程名覆盖
5. **`config.rs`** — JSON 配置读写 + 全局 `DAEMON_CONFIG` 静态 + `reload()` 运行时重载
6. **`log.rs`** — 调试日志模块，运行时通过 `debug` 配置开关，输出带时间戳
7. **`tray.rs`** — 系统托盘图标 + `WM_APP` IPC 处理器（接收 UI 发来的配置重载信号）

原理：拦截 → 吃掉原始事件(`LRESULT(1)`) → 计算平滑曲线 → 连续 `SendInput` 微量注入

### zen-scroll-ui

基于 [gpui](https://github.com/zed-industries/gpui) 的 GPU 加速用户控制面板，用户主入口程序：

- 三档全局速度预设：慢 / 正常 / 快
- 启用/禁用开关
- 读写 `%APPDATA%/ZenScroll/config.json`
- 通过 `FindWindowW` + `PostMessageW(WM_APP)` 向守护进程发送配置重载信号
- 守护进程收到信号后重新加载配置并更新运行状态

---

## 快速开始

### 构建

```bash
cargo build --release
```

### 运行

**守护进程**（需要管理员权限，`WH_MOUSE_LL` 需要 Administrator）：

```bash
cargo run -r -p zen-scroll-daemon
```

托盘图标出现后，左键切换启用/禁用，右键菜单可退出。

**控制面板**（无权限要求）：

```bash
cargo run -r -p zen-scroll-ui

控制面板读写同一份配置文件，并通过 `WM_APP` 窗口消息通知守护进程重载。

---

## 内置应用匹配

| 应用 | 匹配进程 |
|------|----------|
| Chrome | chrome.exe, msedge.exe, brave.exe, opera.exe |
| Readest | readest.exe |
| Firefox | firefox.exe |

> 物理参数由全局速度预设（慢/正常/快）决定，不再按应用区分。

## 自定义配置

在 `%APPDATA%/ZenScroll/config.json` 中可添加 `custom_profiles` 覆盖内置应用匹配规则，或修改 `speed_preset` 选择预设：

```json
{
  "enabled": true,
  "speed_preset": 1,
  "debug": false,
  "custom_profiles": [
    {
      "name": "Chrome",
      "process_names": ["chrome.exe", "msedge.exe", "brave.exe"]
    }
  ]
}
```

- `enabled` — 是否启用平滑滚动
- `speed_preset` — 全局速度预设：`0`=慢, `1`=正常, `2`=快
- `debug` — 设为 `true` 输出详细调试日志（含时间戳），默认关闭
- `custom_profiles` — 覆盖内置应用匹配规则，`name` 必须匹配 Chrome / Readest / Firefox；`process_names` 为匹配的进程名列表

## 参数详解

每个参数直接影响滚动手感。`zen-scroll-ui` 控制面板将参数组合为"慢/正常/快"三档预设，如需微调单个参数，请直接编辑 `%APPDATA%/ZenScroll/config.json`（需修改 `PRESETS` 源码中的常数值）。下方括号内为各参数合理取值范围。

### 摩擦力 Friction `[0.80 — 0.99]`

基准摩擦系数，低速时速度为零附近的有效摩擦力。**值越大，总体滚动越滑、越持久**；**值越小，滚动越"涩"、停得越快**。Smartwheel 机制开启后，动态摩擦力将在 `Friction` ~ `Smart MAX` 之间插值。

### Smart MAX 高速摩擦 `[0.950 — 1.000]`

速度达到 `Max Velocity` 时的摩擦系数。**值越大（越接近 1.0），高速滚动越接近无摩擦惯性滑行**——模拟棘轮脱开后的自由旋转。0.985 时快速甩滚轮可以飞很远，0.992 以上几乎不减速。

**速度-摩擦力插值规则（三次幂曲线）：**

```
speed_ratio = |当前速度| / max_velocity
friction     = Friction + (SmartMAX - Friction) × speed_ratio³
```

慢速 → 摩擦力 ≈ `Friction`（阻尼大，棘轮感）  
快速 → 摩擦力 ≈ `Smart MAX`（阻尼小，自由滑行）

### 回弹力 Bounce Tension `[0.50 — 1.00]`

滚动超出边界（顶部/底部）后回弹的阻尼系数。**值越大，回弹越硬、回弹次数越少**（1.0 是临界阻尼，一次归位）；**值越小，回弹越软、弹性感越强**（低于 0.80 时会有明显的橡皮筋拉伸感）。

### 加速度 Scroll Accel `[0.10 — 3.00]`

滚轮每格刻度值转化为速度的倍率。**值越大，每滚一格产生的初速度越大**，同等滚动手势下滚动距离更远。Readest 设为 1.0（线性），Chrome 设为 1.5（偏激进）。

### 最大速度 Max Velocity `[30 — 400]`

单次滚动的速度上限（像素/tick）。**值越大，快速甩滚轮时能飞得更远**；**值越小，快速滚动时距离被截断**。200+ 适合长页面内容流，120 左右适合精细阅读应用。

### 最小速度 Min Velocity `[0.05 — 2.00]`

速度低于该阈值时直接归零终止滚动。**值越大，滚动尾巴收得越早、越干脆**；**值越小，最后那一点滑行越细碎、越绵长**。注意：值过小（< 0.10）可能导致滚动结束阶段产生肉眼可见的微抖动。

## 推荐配置

| 预设 | 摩擦力 | Smart MAX | 回弹力 | 加速度 | 最大速度 | 最小速度 | 手感描述 |
|------|--------|-----------|--------|--------|---------|---------|---------|
| **慢** | 0.92 | 0.97 | 0.90 | 0.8 | 80 | 0.30 | 每格滚动精准，段落清晰，适合阅读/代码 |
| **正常** | 0.94 | 0.985 | 0.85 | 1.5 | 200 | 0.30 | 慢速有阻尼感，快速能飞，日常浏览首选 |
| **快** | 0.95 | 0.992 | 0.80 | 2.5 | 350 | 0.50 | 起速快、高速持久，适合长文档快速定位 |

> **慢**：低速精确控制，整体偏紧较稳，适合需要逐行阅读的场景。
>
> **正常**：慢速恰到好处的段落感，高速足够滑行距离，适应大多数场景。
>
> **快**：起速极快、高速持续久，适合频繁大范围跳转的长文档。

## 技术细节

- 钩子回调运行于安装线程的消息泵中；注入运行于独立线程，8ms 间隔
- 原始 `WM_MOUSEWHEEL` 事件通过 `LRESULT(1)` 被拦截丢弃，避免应用原生滚动
- 使用 `SendInput` 模拟硬件级滚动输入，兼容 Chrome 的 Raw Input API（`PostMessageW` 无法工作）
- 通过 `INJECTING` 原子标志防止 `SendInput` 导致的钩子重入死循环
- 进程识别使用 `GetForegroundWindow` 而非 `WindowFromPoint`，避免 UIPI 权限隔离导致的窗口不可见
- 托盘图标内嵌于二进制（`include_bytes!`），运行时通过 `LookupIconIdFromDirectoryEx` + `CreateIconFromResourceEx` 加载
- 配置持久化到 `%APPDATA%/ZenScroll/config.json`，使用 serde_json 序列化
- 托盘图标切换时读取现有配置文件再修改，不会丢失 `custom_profiles` 和 `debug` 设置
- `zen-scroll-ui` 通过 `FindWindowW("ZenScrollTray")` 定位守护进程窗口，发送 `WM_APP` 信号触发配置重载
- 全局速度预设定义在 `zen-scroll-core/src/physics.rs` 中的 `PRESETS` 常量数组
- `AppProfile` 不再携带物理参数，仅作为应用匹配规则；物理参数完全由 `speed_preset` 索引的全局预设决定

## 路线图

- [ ] macOS 支持（`CGEventTap`）
- [ ] Linux 支持（`libinput` / XInput）
- [x] 守护进程与控制面板间 IPC 通信（`WM_APP`）
- [ ] 更多应用预设
- [ ] 安装包（WiX / NSIS）

## License

MIT
