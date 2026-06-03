# ZenScroll

**系统级鼠标滚轮平滑滚动优化器**

通过 `WH_MOUSE_LL` 全局钩子拦截鼠标滚轮事件，注入自定义物理引擎驱动的平滑滚动，取代 Chrome / Readest / Firefox 等应用的原生滚动。

![License](https://img.shields.io/badge/license-MIT-blue)
![Rust](https://img.shields.io/badge/rust-2024-orange)

---

## 架构

```
ZenScroll/
├── zen-scroll-core/          # 物理引擎 + 插件系统（库）
├── zen-scroll-daemon/        # 系统守护进程
│   ├── hook.rs               #   WH_MOUSE_LL 钩子安装与回调
│   ├── detect.rs             #   前台进程识别（GetForegroundWindow）
│   ├── smoother.rs           #   物理注入（SendInput 模拟硬件滚动）
│   ├── profile.rs            #   内置预设 + 自定义配置合并
│   ├── config.rs             #   配置持久化（%APPDATA%/ZenScroll/config.json）
│   ├── log.rs                #   调试日志（时间戳 + 运行时开关）
│   ├── tray.rs               #   系统托盘图标
│   └── main.rs               #   入口 + 消息泵
└── gpui-demo/                # GPU 加速参数调节面板
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
4. **`profile.rs`** — 内置 Chrome / Readest / Firefox 三套预设 + 从配置文件加载自定义覆盖
5. **`config.rs`** — JSON 配置读写，位置 `%APPDATA%/ZenScroll/config.json`
6. **`log.rs`** — 调试日志模块，运行时通过 `debug` 配置开关，输出带时间戳
7. **`tray.rs`** — 系统托盘图标，左键开关、右键菜单（状态/Enable/Quit）

原理：拦截 → 吃掉原始事件(`LRESULT(1)`) → 计算平滑曲线 → 连续 `SendInput` 微量注入

### gpui-demo

基于 [gpui](https://github.com/zed-industries/gpui) 的 GPU 加速参数调节面板：

- 三档预设切换（Chrome / Readest / Firefox）
- 7 项物理参数实时调节（滑块 + 滚轮 + 按钮微调）
- 启用/禁用开关
- 底部状态栏显示当前配置

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
cargo run -r -p gpui-demo
```

两个进程独立运行，互不依赖。控制面板可独立用作物理参数调谐器。

---

## 内置预设

| 应用 | 摩擦力 (Friction) | Smart MIN | Smart MAX | 回弹力 (Bounce) | 加速度 (Accel) | 最大速度 (MaxV) | 最小速度 (MinV) | 匹配进程 |
|------|------------------|-----------|-----------|-----------------|----------------|-----------------|-----------------|----------|
| Chrome | 0.94 | 0.93 | 0.998 | 0.85 | 1.5 | 200 | 0.30 | chrome.exe, msedge.exe, brave.exe, opera.exe |
| Readest | 0.96 | 0.95 | 0.999 | 0.90 | 1.0 | 120 | 0.20 | readest.exe |
| Firefox | 0.93 | 0.92 | 0.998 | 0.85 | 1.3 | 180 | 0.40 | firefox.exe |

## 自定义配置

在 `%APPDATA%/ZenScroll/config.json` 中添加 `custom_profiles` 覆盖内置预设参数：

```json
{
  "enabled": true,
  "debug": false,
  "selected_profile": "Chrome",
  "custom_profiles": [
    {
      "name": "Chrome",
      "friction": 0.95,
      "bounce_tension": 0.88,
      "scroll_accel": 2.0,
      "max_velocity": 250.0,
      "min_velocity": 0.25,
      "deceleration_rate": 0.998,
      "max_bounce_distance": 150.0,
      "smartwheel_friction_min": 0.94,
      "smartwheel_friction_max": 0.998
    }
  ]
}
```

- `enabled` — 守护进程启动时是否启用平滑滚动
- `debug` — 设为 `true` 输出详细调试日志（含时间戳），默认关闭
- `selected_profile` — 当前激活的预设名称
- `custom_profiles` — 覆盖内置预设参数，`name` 必须匹配 Chrome / Readest / Firefox

## 参数详解

每个参数直接影响滚动手感，调试面板中可调节范围为下方括号值。

### 摩擦力 Friction `[0.80 — 0.99]`

基准摩擦系数，仅在速度为零时生效（当 Smartwheel 机制激活时）。**值越大，总体滚动越滑、越持久**（如 0.96 的 Readest 预设手感绵长）；**值越小，滚动越"涩"、停得越快**。Smartwheel 开启后，动态摩擦力将在 `Smart MIN` ~ `Smart MAX` 之间插值，此参数不再直接参与每帧衰减。

### Smart MIN 低速摩擦 `[0.80 — 0.99]`

速度接近 0 时的摩擦系数。**值越小，低速时停得越快，段落感越强**——模拟棘轮啮合时的阻力。设到 0.90 以下时，每次滚动的最后几格有明显"卡顿"感，适合喜欢精确控制每格滚动的用户。

### Smart MAX 高速摩擦 `[0.950 — 1.000]`

速度达到 `Max Velocity` 时的摩擦系数。**值越大（越接近 1.0），高速滚动越接近无摩擦惯性滑行**——模拟棘轮脱开后的自由旋转。0.998 时快速甩滚轮可以飞很远，0.999 以上几乎不减速。

**速度-摩擦力插值规则：**

```
speed_ratio = |当前速度| / max_velocity
friction    = SmartMIN + (SmartMAX - SmartMIN) × speed_ratio
```

慢速 → 摩擦力 ≈ Smart MIN（阻尼大，棘轮感）
快速 → 摩擦力 ≈ Smart MAX（阻尼小，自由滑行）

### 回弹力 Bounce Tension `[0.50 — 1.00]`

滚动超出边界（顶部/底部）后回弹的阻尼系数。**值越大，回弹越硬、回弹次数越少**（1.0 是临界阻尼，一次归位）；**值越小，回弹越软、弹性感越强**（低于 0.80 时会有明显的橡皮筋拉伸感）。

### 加速度 Scroll Accel `[0.10 — 3.00]`

滚轮每格刻度值转化为速度的倍率。**值越大，每滚一格产生的初速度越大**，同等滚动手势下滚动距离更远。Readest 设为 1.0（线性），Chrome 设为 1.5（偏激进）。

### 最大速度 Max Velocity `[30 — 400]`

单次滚动的速度上限（像素/tick）。**值越大，快速甩滚轮时能飞得更远**；**值越小，快速滚动时距离被截断**。200+ 适合长页面内容流，120 左右适合精细阅读应用。

### 最小速度 Min Velocity `[0.05 — 2.00]`

速度低于该阈值时直接归零终止滚动。**值越大，滚动尾巴收得越早、越干脆**；**值越小，最后那一点滑行越细碎、越绵长**。注意：值过小（< 0.10）可能导致滚动结束阶段产生肉眼可见的微抖动。

## 推荐配置

| 风格 | 摩擦力 | Smart MIN | Smart MAX | 回弹力 | 加速度 | 最大速度 | 最小速度 | 手感描述 |
|------|--------|-----------|-----------|--------|--------|---------|---------|---------|
| **慢速细腻** | 0.96 | 0.95 | 0.999 | 0.90 | 1.0 | 120 | 0.20 | 每格滚动精准，段落清晰，适合阅读/代码 |
| **均衡** (Chrome 默认) | 0.94 | 0.93 | 0.998 | 0.85 | 1.5 | 200 | 0.30 | 慢速有阻尼感，快速能飞，日常浏览 |
| **快速激进** | 0.92 | 0.90 | 0.998 | 0.80 | 2.0 | 300 | 0.50 | 起速快、高速持续久，适合长文档快速定位 |

> **慢速细腻**：低速时 Smart MIN=0.95 提供充足阻尼，每格滚动手感明确；高速时 Smart MAX=0.999 近乎无摩擦滑行，适合边读边滚的阅读场景。
>
> **均衡**：Chrome 内置预设，低速摩擦 0.93 有恰到好处的段落感，高速 0.998 提供足够滑行距离，适应大多数场景。
>
> **快速激进**：Smart MIN=0.90 段落感最弱，起速极快；加速度 2.0 让每格滚轮产生更大速度，适合需要频繁大范围跳转的长文档。

## 技术细节

- 钩子回调运行于安装线程的消息泵中；注入运行于独立线程，8ms 间隔
- 原始 `WM_MOUSEWHEEL` 事件通过 `LRESULT(1)` 被拦截丢弃，避免应用原生滚动
- 使用 `SendInput` 模拟硬件级滚动输入，兼容 Chrome 的 Raw Input API（`PostMessageW` 无法工作）
- 通过 `INJECTING` 原子标志防止 `SendInput` 导致的钩子重入死循环
- 进程识别使用 `GetForegroundWindow` 而非 `WindowFromPoint`，避免 UIPI 权限隔离导致的窗口不可见
- 托盘图标内嵌于二进制（`include_bytes!`），运行时通过 `LookupIconIdFromDirectoryEx` + `CreateIconFromResourceEx` 加载
- 配置持久化到 `%APPDATA%/ZenScroll/config.json`，使用 serde_json 序列化
- 托盘图标切换时读取现有配置文件再修改，不会丢失 `custom_profiles` 和 `debug` 设置

## 路线图

- [ ] macOS 支持（`CGEventTap`）
- [ ] Linux 支持（`libinput` / XInput）
- [ ] 守护进程与控制面板间 IPC 通信
- [ ] 更多应用预设
- [ ] 安装包（WiX / NSIS）

## License

MIT
