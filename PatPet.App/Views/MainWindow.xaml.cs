using System.ComponentModel;
using System.Runtime;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Threading;
using Microsoft.Win32;
using PatPet.Interop;
using PatPet.Models;
using PatPet.Services;
using PatPet.ViewModels;

namespace PatPet.Views;

/// <summary>
/// Dua mode tampilan, keduanya hanya di monitor utama:
/// Bar: bar hitam tetap di tengah atas layar; klik membuka tray yang menyambung di bawahnya.
/// Ikon: kucing yang bisa diseret; klik membuka tray di dekatnya.
/// </summary>
public partial class MainWindow : Window
{
    private const double BarWindowWidth = PanelWindow.PanelWidth + 20;
    private const double DragThreshold = 4;

    private readonly SettingsService _settings;
    private readonly DispatcherTimer _trimTimer;
    private readonly DispatcherTimer _fullscreenTimer;
    private MainViewModel? _vm;
    private PanelWindow? _panel;
    private bool _hiddenForFullscreen;
    private Win32.ForegroundWatcher? _foregroundWatcher;

    /// <summary>Kapan tray terakhir ditutup karena pindah aplikasi (agar klik ikon tray tidak langsung membukanya lagi).</summary>
    public DateTime LastAutoClosedAt { get; private set; } = DateTime.MinValue;

    // seret ikon
    private bool _pressed, _dragging;
    private Point _pressScreen, _pressWindow;

    public MainWindow(SettingsService settings)
    {
        _settings = settings;
        InitializeComponent();

        _trimTimer = new DispatcherTimer(DispatcherPriority.ApplicationIdle) { Interval = TimeSpan.FromSeconds(8) };
        _trimTimer.Tick += (_, _) => TrimMemory();
        _fullscreenTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(2) };
        _fullscreenTimer.Tick += (_, _) => CheckFullscreen();

        Left = -10000;
        Top = 0;

        SourceInitialized += (_, _) => Win32.MakeToolWindow(new WindowInteropHelper(this).Handle);
        Loaded += (_, _) =>
        {
            ApplyMode();
            _fullscreenTimer.Start();
            _trimTimer.Interval = TimeSpan.FromSeconds(20); // sekali setelah startup (cuaca, lagu, catatan sudah termuat)
            _trimTimer.Start();
        };
        SizeChanged += (_, _) => { if (_vm?.IsBarMode == true) PlaceBar(); };
        DataContextChanged += OnDataContextChanged;

        BarHost.MouseLeftButtonUp += (_, e) =>
        {
            BarHost.CloseHover(immediate: true);
            // saat alarm berbunyi, klik bar = matikan alarm
            if (_vm?.Timer.IsAlarmRinging == true) _vm.Timer.StopAlarmCommand.Execute(null);
            else _vm?.ToggleExpandedCommand.Execute(null);
            e.Handled = true;
        };
        IconHost.MouseLeftButtonDown += OnIconDown;
        IconHost.MouseMove += OnIconMove;
        IconHost.MouseLeftButtonUp += OnIconUp;
        IconHost.LostMouseCapture += (_, _) => { _pressed = false; _dragging = false; };

        SystemEvents.DisplaySettingsChanged += OnDisplaySettingsChanged;
        Closed += (_, _) =>
        {
            SystemEvents.DisplaySettingsChanged -= OnDisplaySettingsChanged;
            _trimTimer.Stop();
            _fullscreenTimer.Stop();
            ClosePanel();
        };
    }

    // ---------- ViewModel ----------

    private void OnDataContextChanged(object sender, DependencyPropertyChangedEventArgs e)
    {
        if (_vm is not null)
        {
            _vm.PropertyChanged -= OnVmPropertyChanged;
            _vm.Settings.PropertyChanged -= OnSettingsPropertyChanged;
        }
        _vm = DataContext as MainViewModel;
        if (_vm is null) return;
        _vm.PropertyChanged += OnVmPropertyChanged;
        _vm.Settings.PropertyChanged += OnSettingsPropertyChanged;
    }

    private void OnSettingsPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(SettingsViewModel.PetScale) && _vm?.IsBarMode == false)
        {
            IconSprite.Scale = _settings.Current.PetScale;
            Dispatcher.InvokeAsync(() => { ClampIcon(); PlacePanel(); }, DispatcherPriority.Loaded);
        }
    }

    private void OnVmPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(MainViewModel.IsExpanded):
                if (_vm!.IsExpanded) OpenPanel(); else ClosePanel();
                break;
            case nameof(MainViewModel.Mode):
                if (_vm!.IsExpanded) _vm.IsExpanded = false;
                ApplyMode();
                break;
            case nameof(MainViewModel.HideInFullscreen):
                CheckFullscreen();
                break;
        }
    }

    // ---------- mode ----------

    private void ApplyMode()
    {
        var bar = _vm?.IsBarMode ?? true;
        BarHost.Visibility = bar ? Visibility.Visible : Visibility.Collapsed;
        IconHost.Visibility = bar ? Visibility.Collapsed : Visibility.Visible;
        Root.Width = bar ? BarWindowWidth : double.NaN; // bar: jendela lebar tetap, bar beranimasi di dalamnya
        IconSprite.Scale = _settings.Current.PetScale;
        UpdateLayout();
        if (bar) PlaceBar(); else PlaceIcon();
    }

    /// <summary>Tengah atas area kerja monitor utama, tetap (tidak bisa digeser).</summary>
    private void PlaceBar()
    {
        var wa = SystemParameters.WorkArea;
        var left = wa.Left + (wa.Width - ActualWidth) / 2;
        if (Math.Abs(Left - left) > 0.5) Left = left;
        if (Math.Abs(Top - wa.Top) > 0.5) Top = wa.Top;
        PlacePanel();
    }

    private void PlaceIcon()
    {
        var wa = SystemParameters.WorkArea;
        Left = _settings.Current.IconLeft ?? wa.Right - 140;
        Top = _settings.Current.IconTop ?? wa.Top + 80;
        ClampIcon();
    }

    /// <summary>Ikon selalu di dalam area kerja monitor utama (tidak bisa dibawa ke monitor kedua).</summary>
    private void ClampIcon()
    {
        var (left, top) = ClampToPrimary(Left, Top);
        if (Math.Abs(left - Left) > 0.5) Left = left;
        if (Math.Abs(top - Top) > 0.5) Top = top;
    }

    private (double Left, double Top) ClampToPrimary(double left, double top)
    {
        var work = SystemParameters.WorkArea; // monitor utama
        return (Math.Max(work.Left, Math.Min(left, work.Right - ActualWidth)),
                Math.Max(work.Top, Math.Min(top, work.Bottom - ActualHeight)));
    }

    private void OnDisplaySettingsChanged(object? sender, EventArgs e) =>
        Dispatcher.InvokeAsync(() => { if (_vm?.IsBarMode == false) ClampIcon(); else PlaceBar(); PlacePanel(); });

    // ---------- seret ikon ke mana saja ----------

    private void OnIconDown(object sender, MouseButtonEventArgs e)
    {
        _pressed = true;
        _dragging = false;
        _pressScreen = ScreenDip(e);
        _pressWindow = new Point(Left, Top);
        IconHost.CaptureMouse();
        e.Handled = true;
    }

    private void OnIconMove(object sender, MouseEventArgs e)
    {
        if (!_pressed) return;
        var p = ScreenDip(e);
        var dx = p.X - _pressScreen.X;
        var dy = p.Y - _pressScreen.Y;
        if (!_dragging && Math.Abs(dx) < DragThreshold && Math.Abs(dy) < DragThreshold) return;
        if (!_dragging && _vm is { IsExpanded: true }) _vm.IsExpanded = false;
        _dragging = true;
        var (left, top) = ClampToPrimary(_pressWindow.X + dx, _pressWindow.Y + dy);
        Left = left;
        Top = top;
    }

    private void OnIconUp(object sender, MouseButtonEventArgs e)
    {
        var wasDragging = _dragging;
        _pressed = false;
        _dragging = false;
        IconHost.ReleaseMouseCapture();

        if (wasDragging)
        {
            ClampIcon();
            _settings.Current.IconLeft = Math.Round(Left);
            _settings.Current.IconTop = Math.Round(Top);
            _settings.Save();
        }
        else if (_vm?.Timer.IsAlarmRinging == true)
        {
            _vm.Timer.StopAlarmCommand.Execute(null);
        }
        else
        {
            _vm?.ToggleExpandedCommand.Execute(null);
        }
        e.Handled = true;
    }

    private Point ScreenDip(MouseEventArgs e)
    {
        var physical = PointToScreen(e.GetPosition(this));
        return PresentationSource.FromVisual(this)?.CompositionTarget is { } ct
            ? ct.TransformFromDevice.Transform(physical)
            : physical;
    }

    // ---------- panel (tray) ----------

    private void OpenPanel()
    {
        if (_vm is null) return;
        _trimTimer.Stop();
        ToolTipService.SetIsEnabled(IconHost, false);
        if (IconHost.ToolTip is ToolTip { IsOpen: true } tip) tip.IsOpen = false;

        if (_panel is null)
        {
            _panel = new PanelWindow(_vm.IsBarMode) { Owner = this, DataContext = _vm };
            _panel.SizeChanged += (_, _) => PlacePanel();
            _panel.CloseRequested += () => { if (_vm is not null) _vm.IsExpanded = false; };
            _panel.Show();
        }
        // tutup saat pindah ke aplikasi lain: Alt+Tab, klik jendela lain, desktop, atau taskbar
        _foregroundWatcher ??= new Win32.ForegroundWatcher(OnOtherAppActivated);
        if (_vm.IsBarMode) BarHost.SetExpanded(true, PanelWindow.PanelWidth);
        PlacePanel();
        _panel.Activate();
    }

    private void ClosePanel()
    {
        ToolTipService.SetIsEnabled(IconHost, true);
        if (_vm?.IsBarMode != false) BarHost.SetExpanded(false, PanelWindow.PanelWidth);
        _foregroundWatcher?.Dispose();
        _foregroundWatcher = null;
        if (_panel is null) return;

        var p = _panel;
        _panel = null;
        p.Close(); // jendela, halaman, dan binding dilepas → bisa di-GC
        _trimTimer.Stop();
        _trimTimer.Start();
    }

    private void PlacePanel()
    {
        if (_panel is null || _panel.ActualWidth <= 0) return;
        double w = _panel.ActualWidth, h = _panel.ActualHeight;

        if (_vm?.IsBarMode != false)
        {
            // tepat di bawah bar, tengahnya sama
            _panel.Left = Left + (ActualWidth - w) / 2;
            _panel.Top = Top + BarView.RowHeight;
            return;
        }

        // mode ikon: di bawah kucing bila muat, kalau tidak di atasnya; selalu di monitor utama
        var work = SystemParameters.WorkArea;
        var top = Top + ActualHeight + 6;
        if (top + h > work.Bottom) top = Top - h - 6;
        top = Math.Max(work.Top, Math.Min(top, work.Bottom - h));
        var left = Left + ActualWidth / 2 - w / 2;
        left = Math.Max(work.Left, Math.Min(left, work.Right - w));
        _panel.Left = left;
        _panel.Top = top;
    }

    private void OnOtherAppActivated()
    {
        if (_vm is not { IsExpanded: true } || !_settings.Current.AutoCollapse) return;
        LastAutoClosedAt = DateTime.Now;
        _vm.IsExpanded = false;
    }

    private void TrimMemory()
    {
        _trimTimer.Stop();
        _trimTimer.Interval = TimeSpan.FromSeconds(8);
        if (_vm is { IsExpanded: true }) return;
        // sekali setelah panel ditutup (bukan berkala)
        GCSettings.LargeObjectHeapCompactionMode = GCLargeObjectHeapCompactionMode.CompactOnce;
        GC.Collect(2, GCCollectionMode.Forced, blocking: true, compacting: true);
        GC.WaitForPendingFinalizers();
        GC.Collect(2, GCCollectionMode.Forced, blocking: true, compacting: true);
        Win32.TrimWorkingSet(); // halaman yang tidak dipakai widget saat diam dikembalikan ke Windows
    }

    // ---------- aplikasi layar penuh ----------

    private void CheckFullscreen()
    {
        if (_vm is null) return;
        var shouldHide = _vm.HideInFullscreen && Win32.IsForegroundFullscreen();
        if (shouldHide && IsVisible)
        {
            _vm.IsExpanded = false;
            _hiddenForFullscreen = true;
            Hide();
        }
        else if (!shouldHide && _hiddenForFullscreen)
        {
            _hiddenForFullscreen = false;
            Show();
        }
    }

    /// <summary>Dipanggil tray "Sembunyikan": jangan dimunculkan lagi otomatis.</summary>
    public void HideByUser()
    {
        _hiddenForFullscreen = false;
        if (_vm is not null) _vm.IsExpanded = false;
        Hide();
    }
}
