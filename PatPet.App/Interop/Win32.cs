using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

namespace PatPet.Interop;

internal static class Win32
{
    public const int GWL_EXSTYLE = -20;
    public const int WS_EX_TOOLWINDOW = 0x00000080;
    public const int WS_EX_APPWINDOW = 0x00040000;

    // ---------- gaya jendela ----------

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr64(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr64(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW", SetLastError = true)]
    private static extern int GetWindowLong32(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW", SetLastError = true)]
    private static extern int SetWindowLong32(IntPtr hWnd, int nIndex, int dwNewLong);

    private static long GetExStyle(IntPtr hwnd) =>
        IntPtr.Size == 8 ? GetWindowLongPtr64(hwnd, GWL_EXSTYLE).ToInt64() : GetWindowLong32(hwnd, GWL_EXSTYLE);

    private static void SetExStyle(IntPtr hwnd, long style)
    {
        if (IntPtr.Size == 8) SetWindowLongPtr64(hwnd, GWL_EXSTYLE, new IntPtr(style));
        else SetWindowLong32(hwnd, GWL_EXSTYLE, (int)style);
    }

    /// <summary>Sembunyikan dari Alt+Tab.</summary>
    public static void MakeToolWindow(IntPtr hwnd) =>
        SetExStyle(hwnd, (GetExStyle(hwnd) | WS_EX_TOOLWINDOW) & ~(long)WS_EX_APPWINDOW);

    // ---------- idle ----------

    [StructLayout(LayoutKind.Sequential)]
    private struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);

    public static TimeSpan GetIdleTime()
    {
        var info = new LASTINPUTINFO { cbSize = (uint)Marshal.SizeOf<LASTINPUTINFO>() };
        if (!GetLastInputInfo(ref info)) return TimeSpan.Zero;
        return TimeSpan.FromMilliseconds(unchecked((uint)Environment.TickCount - info.dwTime));
    }

    // ---------- monitor ----------

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct MONITORINFO
    {
        public int cbSize;
        public RECT rcMonitor;
        public RECT rcWork;
        public uint dwFlags;
    }

    private const uint MONITOR_DEFAULTTONEAREST = 2;

    [DllImport("user32.dll")]
    private static extern IntPtr MonitorFromPoint(POINT pt, uint flags);

    [DllImport("user32.dll")]
    private static extern IntPtr MonitorFromWindow(IntPtr hwnd, uint flags);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO lpmi);

    /// <summary>Area kerja (tanpa taskbar) monitor terdekat dari titik layar fisik.</summary>
    public static RECT? GetWorkAreaAt(int x, int y)
    {
        var mon = MonitorFromPoint(new POINT { X = x, Y = y }, MONITOR_DEFAULTTONEAREST);
        if (mon == IntPtr.Zero) return null;
        var mi = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
        return GetMonitorInfo(mon, ref mi) ? mi.rcWork : null;
    }

    // ---------- deteksi aplikasi layar penuh ----------

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetClassName(IntPtr hWnd, StringBuilder name, int max);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool IsZoomed(IntPtr hWnd);

    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr hwnd, int attr, out RECT value, int size);

    [DllImport("shell32.dll")]
    private static extern int SHQueryUserNotificationState(out int state);

    private const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
    private const int QUNS_RUNNING_D3D_FULL_SCREEN = 3;
    private const int QUNS_PRESENTATION_MODE = 4;

    /// <summary>
    /// true bila aplikasi lain benar-benar layar penuh (video/game/presentasi/F11).
    /// Jendela yang hanya di-maximize TIDAK dihitung — penting saat taskbar auto-hide,
    /// karena jendela maximize (mis. browser, Claude desktop) lalu menutupi seluruh monitor.
    /// </summary>
    public static bool IsForegroundFullscreen()
    {
        // game D3D eksklusif / mode presentasi: langsung dari Windows
        if (SHQueryUserNotificationState(out var quns) == 0 && quns is QUNS_RUNNING_D3D_FULL_SCREEN or QUNS_PRESENTATION_MODE)
            return true;

        var fg = GetForegroundWindow();
        if (fg == IntPtr.Zero || IsZoomed(fg)) return false;

        GetWindowThreadProcessId(fg, out var pid);
        if (pid == (uint)Environment.ProcessId) return false;

        var cls = new StringBuilder(64);
        GetClassName(fg, cls, cls.Capacity);
        var c = cls.ToString();
        if (c is "Progman" or "WorkerW" or "Shell_TrayWnd" or "Shell_SecondaryTrayWnd") return false; // desktop & taskbar

        // batas yang terlihat (tanpa bayangan DWM); fallback ke rect jendela
        if (DwmGetWindowAttribute(fg, DWMWA_EXTENDED_FRAME_BOUNDS, out var r, Marshal.SizeOf<RECT>()) != 0
            && !GetWindowRect(fg, out r)) return false;
        var mi = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
        if (!GetMonitorInfo(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), ref mi)) return false;
        // PatPet hanya tampil di monitor utama, jadi layar penuh di monitor kedua tidak menyembunyikannya
        if ((mi.dwFlags & MONITORINFOF_PRIMARY) == 0) return false;
        var m = mi.rcMonitor;
        return r.Left <= m.Left && r.Top <= m.Top && r.Right >= m.Right && r.Bottom >= m.Bottom;
    }

    private const uint MONITORINFOF_PRIMARY = 1;

    // ---------- jendela aktif berpindah (Alt+Tab, klik aplikasi lain, desktop, taskbar) ----------

    private delegate void WinEventProc(IntPtr hook, uint evt, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);

    [DllImport("user32.dll")]
    private static extern IntPtr SetWinEventHook(uint eventMin, uint eventMax, IntPtr hmod, WinEventProc proc, uint pid, uint tid, uint flags);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnhookWinEvent(IntPtr hook);

    private const uint EVENT_SYSTEM_FOREGROUND = 0x0003;
    private const uint WINEVENT_OUTOFCONTEXT = 0x0000;
    private const uint WINEVENT_SKIPOWNPROCESS = 0x0002;

    /// <summary>
    /// Memberi tahu saat jendela aktif berpindah ke aplikasi lain. Event datang di thread UI
    /// (thread yang memasang hook). Hanya aktif selama tray terbuka; Dispose melepas hook.
    /// </summary>
    public sealed class ForegroundWatcher : IDisposable
    {
        private readonly WinEventProc _proc; // disimpan agar delegate tidak di-GC selama hook terpasang
        private IntPtr _hook;

        public ForegroundWatcher(Action onOtherAppActivated)
        {
            _proc = (_, _, hwnd, _, _, _, _) =>
            {
                if (hwnd == IntPtr.Zero) return;
                GetWindowThreadProcessId(hwnd, out var pid);
                if (pid != (uint)Environment.ProcessId) onOtherAppActivated();
            };
            _hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, IntPtr.Zero, _proc, 0, 0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        }

        public void Dispose()
        {
            if (_hook == IntPtr.Zero) return;
            UnhookWinEvent(_hook);
            _hook = IntPtr.Zero;
        }
    }

    // ---------- refresh rate monitor ----------

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public short dmSpecVersion;
        public short dmDriverVersion;
        public short dmSize;
        public short dmDriverExtra;
        public int dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public int dmDisplayOrientation;
        public int dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public short dmLogPixels;
        public int dmBitsPerPel;
        public int dmPelsWidth;
        public int dmPelsHeight;
        public int dmDisplayFlags;
        public int dmDisplayFrequency;
        public int dmICMMethod;
        public int dmICMIntent;
        public int dmMediaType;
        public int dmDitherType;
        public int dmReserved1;
        public int dmReserved2;
        public int dmPanningWidth;
        public int dmPanningHeight;
    }

    private const int ENUM_CURRENT_SETTINGS = -1;

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumDisplaySettings(string? deviceName, int modeNum, ref DEVMODE devMode);

    /// <summary>Refresh rate monitor utama (Hz); 60 bila tidak terbaca.</summary>
    public static int GetRefreshRate()
    {
        try
        {
            var dm = new DEVMODE { dmDeviceName = "", dmFormName = "", dmSize = (short)Marshal.SizeOf<DEVMODE>() };
            if (EnumDisplaySettings(null, ENUM_CURRENT_SETTINGS, ref dm) && dm.dmDisplayFrequency is >= 30 and <= 500)
                return dm.dmDisplayFrequency;
        }
        catch { }
        return 60;
    }

    // ---------- memori ----------

    [DllImport("kernel32.dll")]
    private static extern IntPtr GetCurrentProcess();

    [DllImport("psapi.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EmptyWorkingSet(IntPtr process);

    /// <summary>Lepas halaman memori yang sedang tidak dipakai dari working set (dipanggil saat widget diam).</summary>
    public static void TrimWorkingSet()
    {
        try { EmptyWorkingSet(GetCurrentProcess()); } catch { }
    }

    // ---------- baterai ----------

    [StructLayout(LayoutKind.Sequential)]
    private struct SYSTEM_POWER_STATUS
    {
        public byte ACLineStatus;
        public byte BatteryFlag;
        public byte BatteryLifePercent;
        public byte SystemStatusFlag;
        public int BatteryLifeTime;
        public int BatteryFullLifeTime;
    }

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetSystemPowerStatus(out SYSTEM_POWER_STATUS status);

    public readonly record struct PowerStatus(bool HasBattery, int Percent, bool Plugged, bool Charging);

    public static PowerStatus GetPowerStatus()
    {
        if (!GetSystemPowerStatus(out var s)) return new PowerStatus(false, 0, true, false);
        var hasBattery = s.BatteryFlag != 128 && s.BatteryFlag != 255 && s.BatteryLifePercent <= 100;
        return new PowerStatus(
            hasBattery,
            hasBattery ? s.BatteryLifePercent : 0,
            s.ACLineStatus == 1,
            hasBattery && (s.BatteryFlag & 8) != 0);
    }

    // ---------- suhu (Performance Counter "Thermal Zone Information", tanpa hak admin) ----------

    [StructLayout(LayoutKind.Explicit)]
    private struct PDH_FMT_COUNTERVALUE
    {
        [FieldOffset(0)] public uint CStatus;
        [FieldOffset(8)] public double doubleValue;
        [FieldOffset(8)] public long largeValue;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct PDH_FMT_COUNTERVALUE_ITEM
    {
        public IntPtr szName;
        public PDH_FMT_COUNTERVALUE FmtValue;
    }

    private const uint PDH_FMT_DOUBLE = 0x00000200;
    private const uint PDH_MORE_DATA = 0x800007D2;

    [DllImport("pdh.dll", CharSet = CharSet.Unicode)]
    private static extern uint PdhOpenQuery(string? dataSource, IntPtr userData, out IntPtr query);

    [DllImport("pdh.dll", CharSet = CharSet.Unicode)]
    private static extern uint PdhAddEnglishCounter(IntPtr query, string path, IntPtr userData, out IntPtr counter);

    [DllImport("pdh.dll")]
    private static extern uint PdhCollectQueryData(IntPtr query);

    [DllImport("pdh.dll", CharSet = CharSet.Unicode)]
    private static extern uint PdhGetFormattedCounterArray(IntPtr counter, uint format, ref uint bufferSize, out uint itemCount, IntPtr buffer);

    [DllImport("pdh.dll")]
    private static extern uint PdhCloseQuery(IntPtr query);

    public sealed class ThermalQuery : IDisposable
    {
        private IntPtr _query;
        private readonly IntPtr _counter;

        private ThermalQuery(IntPtr query, IntPtr counter)
        {
            _query = query;
            _counter = counter;
        }

        public static ThermalQuery? TryOpen()
        {
            if (PdhOpenQuery(null, IntPtr.Zero, out var q) != 0) return null;
            if (PdhAddEnglishCounter(q, @"\Thermal Zone Information(*)\Temperature", IntPtr.Zero, out var c) != 0)
            {
                PdhCloseQuery(q);
                return null;
            }
            return new ThermalQuery(q, c);
        }

        /// <summary>Suhu tertinggi dari semua thermal zone, dalam °C; null bila tidak tersedia.</summary>
        public double? ReadCelsius()
        {
            if (_query == IntPtr.Zero || PdhCollectQueryData(_query) != 0) return null;

            uint size = 0;
            var rc = PdhGetFormattedCounterArray(_counter, PDH_FMT_DOUBLE, ref size, out _, IntPtr.Zero);
            if (rc != PDH_MORE_DATA || size == 0 || size > 64 * 1024) return null;

            var buffer = Marshal.AllocHGlobal((int)size);
            try
            {
                if (PdhGetFormattedCounterArray(_counter, PDH_FMT_DOUBLE, ref size, out var count, buffer) != 0) return null;
                var itemSize = Marshal.SizeOf<PDH_FMT_COUNTERVALUE_ITEM>();
                double? max = null;
                for (var i = 0; i < count; i++)
                {
                    var item = Marshal.PtrToStructure<PDH_FMT_COUNTERVALUE_ITEM>(buffer + i * itemSize);
                    if (item.FmtValue.CStatus != 0) continue;
                    var celsius = item.FmtValue.doubleValue - 273.15; // counter dalam Kelvin
                    if (celsius is > 0 and < 125 && (max is null || celsius > max)) max = celsius;
                }
                return max;
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
        }

        public void Dispose()
        {
            if (_query == IntPtr.Zero) return;
            PdhCloseQuery(_query);
            _query = IntPtr.Zero;
        }
    }
}
