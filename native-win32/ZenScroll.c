#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>
#include <windowsx.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>

#define WM_TRAY_ICON (WM_APP + 1)
#define CMD_STATUS 1000
#define CMD_TOGGLE 1001
#define CMD_SHOW_UI 1002
#define CMD_QUIT 1003
#define ID_BTN_TOGGLE 2001
#define ID_BTN_SLOW 2002
#define ID_BTN_NORMAL 2003
#define ID_BTN_FAST 2004
#define ID_TXT_STATUS 2005
#define WHEEL_DELTA_ZEN 120
#define TICK_MS 8

typedef struct ScrollConfig {
    double friction;
    double min_velocity;
    double max_velocity;
    double scroll_accel;
    double smartwheel_friction_max;
} ScrollConfig;

static const ScrollConfig PRESETS[3] = {
    {0.92, 0.3, 80.0, 0.8, 0.97},
    {0.94, 0.3, 200.0, 1.5, 0.985},
    {0.95, 0.5, 350.0, 2.5, 0.992},
};

static HHOOK g_hook = NULL;
static HWND g_tray_hwnd = NULL;
static HWND g_panel_hwnd = NULL;
static HANDLE g_worker = NULL;
static volatile LONG g_running = 1;
static volatile LONG g_enabled = 1;
static volatile LONG g_injecting = 0;
static CRITICAL_SECTION g_lock;
static double g_velocity = 0.0;
static bool g_active = false;
static ULONGLONG g_last_tick = 0;
static ULONGLONG g_last_scroll_time = 0;
static HWND g_scroll_hwnd = NULL;
static POINT g_scroll_pt = {0, 0};
static int g_speed_preset = 1;
static UINT g_original_scroll_lines = 3;

static void lower_working_set(void) {
    SetProcessWorkingSetSize(GetCurrentProcess(), (SIZE_T)-1, (SIZE_T)-1);
}

static void config_path(wchar_t *out, DWORD cap) {
    wchar_t base[MAX_PATH * 2] = L"";
    DWORD n = GetEnvironmentVariableW(L"APPDATA", base, (DWORD)(sizeof(base) / sizeof(base[0])));
    if (n == 0 || n >= sizeof(base) / sizeof(base[0])) {
        GetCurrentDirectoryW((DWORD)(sizeof(base) / sizeof(base[0])), base);
    }
    swprintf(out, cap, L"%ls\\ZenScroll\\config.json", base);
}

static char *read_config_file(DWORD *len_out) {
    wchar_t path[MAX_PATH * 2];
    config_path(path, (DWORD)(sizeof(path) / sizeof(path[0])));
    HANDLE f = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (f == INVALID_HANDLE_VALUE) return NULL;
    DWORD size = GetFileSize(f, NULL);
    if (size == INVALID_FILE_SIZE || size > 65536) { CloseHandle(f); return NULL; }
    char *buf = (char *)HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, size + 1);
    if (!buf) { CloseHandle(f); return NULL; }
    DWORD read = 0;
    if (!ReadFile(f, buf, size, &read, NULL)) { HeapFree(GetProcessHeap(), 0, buf); CloseHandle(f); return NULL; }
    CloseHandle(f);
    buf[read] = 0;
    if (len_out) *len_out = read;
    return buf;
}

static const char *skip_ws(const char *p) {
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') p++;
    return p;
}

static void write_config(void) {
    wchar_t path[MAX_PATH * 2];
    config_path(path, (DWORD)(sizeof(path) / sizeof(path[0])));
    wchar_t dir[MAX_PATH * 2];
    wcscpy(dir, path);
    wchar_t *slash = wcsrchr(dir, L'\\');
    if (slash) { *slash = 0; CreateDirectoryW(dir, NULL); }
    char json[256];
    snprintf(json, sizeof(json), "{\n  \"enabled\": %s,\n  \"speed_preset\": %d,\n  \"custom_profiles\": [],\n  \"debug\": false\n}\n", g_enabled ? "true" : "false", g_speed_preset);
    HANDLE f = CreateFileW(path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (f == INVALID_HANDLE_VALUE) return;
    DWORD written = 0;
    WriteFile(f, json, (DWORD)strlen(json), &written, NULL);
    CloseHandle(f);
}

static void reload_config(void) {
    DWORD len = 0;
    char *buf = read_config_file(&len);
    if (!buf) { g_enabled = 1; g_speed_preset = 1; write_config(); return; }
    char *p = strstr(buf, "\"enabled\"");
    if (p && (p = strchr(p, ':'))) {
        p = (char *)skip_ws(p + 1);
        g_enabled = (strncmp(p, "false", 5) == 0) ? 0 : 1;
    }
    p = strstr(buf, "\"speed_preset\"");
    if (p && (p = strchr(p, ':'))) {
        int v = atoi(p + 1);
        if (v < 0) v = 0;
        if (v > 2) v = 2;
        g_speed_preset = v;
    }
    HeapFree(GetProcessHeap(), 0, buf);
}

static void update_ui(void);
static void update_tray_tip(void);

static void set_enabled(bool enabled) {
    g_enabled = enabled ? 1 : 0;
    write_config();
    update_ui();
    update_tray_tip();
    lower_working_set();
}

static void set_preset(int preset) {
    if (preset < 0) preset = 0;
    if (preset > 2) preset = 2;
    g_speed_preset = preset;
    write_config();
    update_ui();
    update_tray_tip();
    lower_working_set();
}

static double adaptive_scroll_factor(double interval_ms) {
    const double FAST_MS = 30.0, SLOW_MS = 300.0, MIN_FACTOR = 0.3, MAX_FACTOR = 3.0;
    if (interval_ms >= SLOW_MS) return MIN_FACTOR;
    if (interval_ms <= FAST_MS) return MAX_FACTOR;
    double t = (interval_ms - FAST_MS) / (SLOW_MS - FAST_MS);
    return MAX_FACTOR + (MIN_FACTOR - MAX_FACTOR) * t;
}

static double smartwheel_friction(const ScrollConfig *cfg, double velocity) {
    double v = velocity < 0 ? -velocity : velocity;
    double ratio = v / cfg->max_velocity;
    if (ratio < 0.0) ratio = 0.0;
    if (ratio > 1.0) ratio = 1.0;
    double weight = ratio * ratio * ratio;
    return cfg->friction + (cfg->smartwheel_friction_max - cfg->friction) * weight;
}

static void feed_wheel(int raw_delta, HWND target, POINT pt) {
    ULONGLONG now = GetTickCount64();
    const ScrollConfig *cfg = &PRESETS[g_speed_preset];
    double factor = g_active ? adaptive_scroll_factor((double)(now - g_last_scroll_time)) : 0.5;
    g_last_scroll_time = now;
    g_scroll_hwnd = target;
    g_scroll_pt = pt;
    g_velocity += (double)raw_delta * cfg->scroll_accel * factor;
    if (g_velocity > cfg->max_velocity) g_velocity = cfg->max_velocity;
    if (g_velocity < -cfg->max_velocity) g_velocity = -cfg->max_velocity;
    g_active = true;
    g_last_tick = now;
}

static void tick_injector(void) {
    if (!g_active) return;
    ULONGLONG now = GetTickCount64();
    const ScrollConfig *cfg = &PRESETS[g_speed_preset];
    double dt_ratio = (double)(now - g_last_tick) / (double)TICK_MS;
    if (dt_ratio <= 0.0) dt_ratio = 1.0;
    if (dt_ratio > 8.0) dt_ratio = 8.0;
    g_last_tick = now;
    int send = (int)(g_velocity * dt_ratio);
    g_velocity *= smartwheel_friction(cfg, g_velocity);
    double av = g_velocity < 0 ? -g_velocity : g_velocity;
    if (av < cfg->min_velocity) { g_velocity = 0.0; g_active = false; return; }
    if (send != 0) {
        int delta = send;
        if (delta > WHEEL_DELTA_ZEN * 4) delta = WHEEL_DELTA_ZEN * 4;
        if (delta < -WHEEL_DELTA_ZEN * 4) delta = -WHEEL_DELTA_ZEN * 4;
        INPUT input;
        ZeroMemory(&input, sizeof(input));
        input.type = INPUT_MOUSE;
        input.mi.mouseData = (DWORD)delta;
        input.mi.dwFlags = MOUSEEVENTF_WHEEL;
        if (g_scroll_hwnd && IsWindow(g_scroll_hwnd)) {
            WPARAM wp = ((WPARAM)((WORD)delta)) << 16;
            LPARAM lp = ((LPARAM)((WORD)g_scroll_pt.x)) | (((LPARAM)((WORD)g_scroll_pt.y)) << 16);
            PostMessageW(g_scroll_hwnd, WM_MOUSEWHEEL, wp, lp);
        } else {
            InterlockedExchange(&g_injecting, 1);
            SendInput(1, &input, sizeof(INPUT));
            InterlockedExchange(&g_injecting, 0);
        }
    }
}

static DWORD WINAPI worker_thread(LPVOID unused) {
    (void)unused;
    while (InterlockedCompareExchange(&g_running, 1, 1)) {
        EnterCriticalSection(&g_lock);
        tick_injector();
        LeaveCriticalSection(&g_lock);
        Sleep(TICK_MS);
    }
    return 0;
}

static LRESULT CALLBACK mouse_proc(int nCode, WPARAM wParam, LPARAM lParam) {
    if (InterlockedCompareExchange(&g_injecting, 1, 1)) return CallNextHookEx(NULL, nCode, wParam, lParam);
    if (nCode >= 0 && wParam == WM_MOUSEWHEEL) {
        MSLLHOOKSTRUCT *ms = (MSLLHOOKSTRUCT *)lParam;
        if (ms && (ms->flags & 0x00000003)) return CallNextHookEx(NULL, nCode, wParam, lParam);
        if (g_enabled) {
            int raw_delta = (SHORT)HIWORD(ms->mouseData);
            POINT pt = ms->pt;
            HWND target = WindowFromPoint(pt);
            EnterCriticalSection(&g_lock);
            feed_wheel(raw_delta, target, pt);
            LeaveCriticalSection(&g_lock);
            return 1;
        }
    }
    return CallNextHookEx(NULL, nCode, wParam, lParam);
}

static void update_tray_tip(void) {
    if (!g_tray_hwnd) return;
    NOTIFYICONDATAW nid;
    ZeroMemory(&nid, sizeof(nid));
    nid.cbSize = sizeof(nid);
    nid.hWnd = g_tray_hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_TIP;
    swprintf(nid.szTip, sizeof(nid.szTip) / sizeof(nid.szTip[0]), L"ZenScroll - %ls", g_enabled ? L"运行中" : L"已停止");
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

static void add_tray_icon(void) {
    NOTIFYICONDATAW nid;
    ZeroMemory(&nid, sizeof(nid));
    nid.cbSize = sizeof(nid);
    nid.hWnd = g_tray_hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_ICON;
    nid.hIcon = LoadIconW(GetModuleHandleW(NULL), MAKEINTRESOURCEW(1));
    if (!nid.hIcon) nid.hIcon = LoadIconW(NULL, IDI_APPLICATION);
    wcscpy(nid.szTip, L"ZenScroll - 运行中");
    Shell_NotifyIconW(NIM_ADD, &nid);
}

static void remove_tray_icon(void) {
    NOTIFYICONDATAW nid;
    ZeroMemory(&nid, sizeof(nid));
    nid.cbSize = sizeof(nid);
    nid.hWnd = g_tray_hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

static void show_panel(void) {
    if (g_panel_hwnd) {
        ShowWindow(g_panel_hwnd, SW_SHOW);
        SetForegroundWindow(g_panel_hwnd);
    }
}

static void show_menu(void) {
    HMENU menu = CreatePopupMenu();
    if (!menu) return;
    AppendMenuW(menu, MF_STRING | MF_GRAYED | MF_BYCOMMAND, CMD_STATUS, g_enabled ? L" 运行中" : L" 已停止");
    AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, NULL);
    AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_TOGGLE, g_enabled ? L"禁用" : L"启用");
    AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, NULL);
    AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_SHOW_UI, L"控制面板");
    AppendMenuW(menu, MF_SEPARATOR | MF_BYCOMMAND, 0, NULL);
    AppendMenuW(menu, MF_STRING | MF_BYCOMMAND, CMD_QUIT, L"退出");
    POINT pt;
    GetCursorPos(&pt);
    SetForegroundWindow(g_tray_hwnd);
    TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_RIGHTBUTTON, pt.x, pt.y, 0, g_tray_hwnd, NULL);
    DestroyMenu(menu);
}

static RECT g_toggle_rect = {28, 190, 144, 228};
static RECT g_slow_rect = {166, 190, 244, 228};
static RECT g_normal_rect = {256, 190, 334, 228};
static RECT g_fast_rect = {346, 190, 424, 228};

static void update_ui(void) {
    if (g_panel_hwnd) InvalidateRect(g_panel_hwnd, NULL, TRUE);
}

static void fill_rect(HDC hdc, RECT rc, COLORREF color) {
    HBRUSH b = CreateSolidBrush(color);
    FillRect(hdc, &rc, b);
    DeleteObject(b);
}

static void draw_round_box(HDC hdc, RECT rc, COLORREF fill, COLORREF stroke, int radius) {
    HBRUSH b = CreateSolidBrush(fill);
    HPEN p = CreatePen(PS_SOLID, 1, stroke);
    HGDIOBJ old_b = SelectObject(hdc, b);
    HGDIOBJ old_p = SelectObject(hdc, p);
    RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    DeleteObject(b);
    DeleteObject(p);
}

static void draw_text(HDC hdc, const wchar_t *text, RECT rc, COLORREF color, int size, int weight, UINT format) {
    HFONT font = CreateFontW(size, 0, 0, 0, weight, FALSE, FALSE, FALSE, DEFAULT_CHARSET,
                             OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
                             DEFAULT_PITCH | FF_SWISS, L"Segoe UI");
    HGDIOBJ old = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, color);
    DrawTextW(hdc, text, -1, &rc, format);
    SelectObject(hdc, old);
    DeleteObject(font);
}

static void draw_button(HDC hdc, RECT rc, const wchar_t *text, bool active, bool primary) {
    COLORREF fill = primary ? (active ? RGB(37, 99, 235) : RGB(31, 41, 55))
                            : (active ? RGB(30, 64, 175) : RGB(17, 24, 39));
    COLORREF stroke = active ? RGB(96, 165, 250) : RGB(55, 65, 81);
    COLORREF text_color = primary ? RGB(255, 255, 255) : (active ? RGB(219, 234, 254) : RGB(209, 213, 219));
    draw_round_box(hdc, rc, fill, stroke, 14);
    draw_text(hdc, text, rc, text_color, 18, FW_SEMIBOLD, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
}

static bool point_in_rect(RECT rc, int x, int y) {
    return x >= rc.left && x <= rc.right && y >= rc.top && y <= rc.bottom;
}

static void paint_panel(HWND hwnd) {
    PAINTSTRUCT ps;
    HDC hdc = BeginPaint(hwnd, &ps);
    RECT client;
    GetClientRect(hwnd, &client);
    fill_rect(hdc, client, RGB(11, 18, 32));

    RECT title = {28, 20, 430, 50};
    draw_text(hdc, L"ZenScroll", title, RGB(243, 244, 246), 28, FW_BOLD, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
    RECT sub = {30, 50, 430, 74};
    draw_text(hdc, L"像 macOS 一样平滑滚动", sub, RGB(148, 163, 184), 16, FW_NORMAL, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    RECT card = {28, 96, 424, 170};
    draw_round_box(hdc, card, RGB(15, 23, 42), RGB(30, 41, 59), 18);

    RECT dot = {48, 124, 66, 142};
    draw_round_box(hdc, dot, g_enabled ? RGB(34, 197, 94) : RGB(239, 68, 68), g_enabled ? RGB(74, 222, 128) : RGB(248, 113, 113), 20);
    RECT state = {82, 112, 390, 142};
    draw_text(hdc, g_enabled ? L"运行中" : L"已停止", state, g_enabled ? RGB(134, 239, 172) : RGB(252, 165, 165), 22, FW_BOLD, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    const wchar_t *preset = g_speed_preset == 0 ? L"慢速" : (g_speed_preset == 1 ? L"正常" : L"快速");
    wchar_t line[160];
    swprintf(line, sizeof(line) / sizeof(line[0]), L"当前速度：%ls    生效范围：全局滚动窗口", preset);
    RECT desc = {82, 142, 400, 164};
    draw_text(hdc, line, desc, RGB(203, 213, 225), 15, FW_NORMAL, DT_LEFT | DT_VCENTER | DT_SINGLELINE);

    draw_button(hdc, g_toggle_rect, g_enabled ? L"禁用" : L"启用", true, true);
    draw_button(hdc, g_slow_rect, L"慢速", g_speed_preset == 0, false);
    draw_button(hdc, g_normal_rect, L"正常", g_speed_preset == 1, false);
    draw_button(hdc, g_fast_rect, L"快速", g_speed_preset == 2, false);

    RECT hint = {28, 254, 424, 282};
    draw_text(hdc, L"关闭窗口最小化到托盘；右键托盘可退出。", hint, RGB(100, 116, 139), 14, FW_NORMAL, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
    EndPaint(hwnd, &ps);
}

static LRESULT CALLBACK panel_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
        case WM_PAINT:
            paint_panel(hwnd);
            return 0;
        case WM_LBUTTONDOWN: {
            int x = GET_X_LPARAM(lp);
            int y = GET_Y_LPARAM(lp);
            if (point_in_rect(g_toggle_rect, x, y)) set_enabled(!g_enabled);
            else if (point_in_rect(g_slow_rect, x, y)) set_preset(0);
            else if (point_in_rect(g_normal_rect, x, y)) set_preset(1);
            else if (point_in_rect(g_fast_rect, x, y)) set_preset(2);
            return 0;
        }
        case WM_CLOSE:
            ShowWindow(hwnd, SW_HIDE);
            lower_working_set();
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

static LRESULT CALLBACK tray_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    if (msg == WM_TRAY_ICON) {
        if ((UINT)lp == WM_LBUTTONUP) show_panel();
        else if ((UINT)lp == WM_RBUTTONUP) show_menu();
        return 0;
    }
    if (msg == WM_COMMAND) {
        switch (LOWORD(wp)) {
            case CMD_TOGGLE: set_enabled(!g_enabled); break;
            case CMD_SHOW_UI: show_panel(); break;
            case CMD_QUIT: DestroyWindow(hwnd); break;
            default: break;
        }
        return 0;
    }
    if (msg == WM_APP) {
        reload_config();
        update_ui();
        update_tray_tip();
        lower_working_set();
        return 0;
    }
    if (msg == WM_DESTROY) {
        remove_tray_icon();
        if (g_panel_hwnd) DestroyWindow(g_panel_hwnd);
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(hwnd, msg, wp, lp);
}

static bool create_windows(HINSTANCE hInstance) {
    WNDCLASSW wc;
    ZeroMemory(&wc, sizeof(wc));
    wc.lpfnWndProc = tray_proc;
    wc.hInstance = hInstance;
    wc.lpszClassName = L"ZenScrollTray";
    wc.hIcon = LoadIconW(hInstance, MAKEINTRESOURCEW(1));
    if (!RegisterClassW(&wc)) return false;
    g_tray_hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, wc.lpszClassName, L"ZenScroll", 0, 0, 0, 0, 0, NULL, NULL, hInstance, NULL);
    if (!g_tray_hwnd) return false;

    WNDCLASSW pc;
    ZeroMemory(&pc, sizeof(pc));
    pc.lpfnWndProc = panel_proc;
    pc.hInstance = hInstance;
    pc.lpszClassName = L"ZenScrollPanel";
    pc.hIcon = LoadIconW(hInstance, MAKEINTRESOURCEW(1));
    pc.hCursor = LoadCursorW(NULL, IDC_ARROW);
    pc.hbrBackground = CreateSolidBrush(RGB(11, 18, 32));
    if (!RegisterClassW(&pc)) return false;
    g_panel_hwnd = CreateWindowExW(0, pc.lpszClassName, L"ZenScroll", WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX, CW_USEDEFAULT, CW_USEDEFAULT, 470, 330, NULL, NULL, hInstance, NULL);
    return g_panel_hwnd != NULL;
}

int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE hPrev, PWSTR cmd, int show) {
    (void)hPrev; (void)cmd; (void)show;
    HeapSetInformation(NULL, HeapEnableTerminationOnCorruption, NULL, 0);
    InitializeCriticalSection(&g_lock);
    reload_config();
    SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &g_original_scroll_lines, 0);
    if (g_original_scroll_lines == 0) {
        UINT one = 1;
        SystemParametersInfoW(SPI_SETWHEELSCROLLLINES, one, &one, SPIF_UPDATEINIFILE);
    }

    if (!create_windows(hInstance)) return 1;
    g_hook = SetWindowsHookExW(WH_MOUSE_LL, mouse_proc, hInstance, 0);
    if (!g_hook) return 2;
    add_tray_icon();
    update_ui();
    update_tray_tip();
    g_worker = CreateThread(NULL, 64 * 1024, worker_thread, NULL, STACK_SIZE_PARAM_IS_A_RESERVATION, NULL);
    ShowWindow(g_panel_hwnd, SW_SHOW);
    UpdateWindow(g_panel_hwnd);
    lower_working_set();

    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    InterlockedExchange(&g_running, 0);
    if (g_worker) { WaitForSingleObject(g_worker, 1000); CloseHandle(g_worker); }
    if (g_hook) UnhookWindowsHookEx(g_hook);
    if (g_original_scroll_lines == 0) {
        UINT zero = 0;
        SystemParametersInfoW(SPI_SETWHEELSCROLLLINES, 0, &zero, SPIF_UPDATEINIFILE);
    }
    DeleteCriticalSection(&g_lock);
    return 0;
}
