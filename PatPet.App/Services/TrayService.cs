using System.Windows;
using System.Windows.Controls;
using System.Windows.Media.Imaging;
using CommunityToolkit.Mvvm.Input;
using H.NotifyIcon;
using PatPet.ViewModels;
using PatPet.Views;

namespace PatPet.Services;

public interface INotificationService
{
    void Notify(string title, string message);
}

public sealed class TrayService : INotificationService, IDisposable
{
    private TaskbarIcon? _icon;
    private MainWindow? _window;
    private MainViewModel? _vm;
    private MenuItem? _panelItem, _hideItem, _topmostItem, _timerItem, _modeItem;

    public void Initialize(MainWindow window, MainViewModel vm)
    {
        _window = window;
        _vm = vm;

        _panelItem = new MenuItem();
        _panelItem.Click += (_, _) => TogglePanel();

        _hideItem = new MenuItem();
        _hideItem.Click += (_, _) =>
        {
            if (window.IsVisible) window.HideByUser();
            else window.Show();
        };

        _topmostItem = new MenuItem { Header = "Selalu di atas", IsCheckable = true };
        _topmostItem.Click += (_, _) => vm.AlwaysOnTop = _topmostItem.IsChecked;

        _timerItem = new MenuItem();
        _timerItem.Click += (_, _) => vm.Timer.StartPauseCommand.Execute(null);

        _modeItem = new MenuItem();
        _modeItem.Click += (_, _) => vm.Mode = vm.IsBarMode ? Models.DisplayMode.Icon : Models.DisplayMode.Bar;

        var exit = new MenuItem { Header = "Keluar" };
        exit.Click += async (_, _) => await ((App)Application.Current).ExitAppAsync();

        var menu = new ContextMenu();
        menu.Items.Add(_panelItem);
        menu.Items.Add(_hideItem);
        menu.Items.Add(_modeItem);
        menu.Items.Add(_topmostItem);
        menu.Items.Add(_timerItem);
        menu.Items.Add(new Separator());
        menu.Items.Add(exit);
        menu.Opened += (_, _) => RefreshMenu();
        RefreshMenu();

        _icon = new TaskbarIcon
        {
            ToolTipText = "PatPet",
            IconSource = new BitmapImage(new Uri("pack://application:,,,/PatPet;component/Assets/app.ico")),
            ContextMenu = menu,
            NoLeftClickDelay = true,
            LeftClickCommand = new RelayCommand(TogglePanel),
        };
        _icon.ForceCreate(false);
    }

    private void RefreshMenu()
    {
        if (_vm is null || _window is null) return;
        _panelItem!.Header = _vm.IsExpanded ? "Tutup tray" : "Buka tray";
        _hideItem!.Header = _window.IsVisible ? "Sembunyikan PatPet" : "Tampilkan PatPet";
        _topmostItem!.IsChecked = _vm.AlwaysOnTop;
        _timerItem!.Header = _vm.Timer.IsActive
            ? (_vm.Timer.IsPaused ? "Lanjutkan timer" : "Jeda timer")
            : $"Mulai timer ({_vm.Timer.DurationText})";
        _modeItem!.Header = _vm.IsBarMode ? "Ganti ke mode ikon" : "Ganti ke mode bar atas";
    }

    private void TogglePanel()
    {
        if (_window is null || _vm is null) return;
        if (!_window.IsVisible) _window.Show();
        // klik ikon tray memindahkan fokus ke taskbar sehingga tray PatPet baru saja tertutup;
        // jangan langsung dibuka lagi, klik ini maksudnya menutup
        if (!_vm.IsExpanded && DateTime.Now - _window.LastAutoClosedAt < TimeSpan.FromMilliseconds(400)) return;
        _vm.IsExpanded = !_vm.IsExpanded;
    }

    public void Notify(string title, string message)
    {
        try { _icon?.ShowNotification(title, message); }
        catch { /* notifikasi bersifat opsional */ }
    }

    public void Dispose()
    {
        _icon?.Dispose();
        _icon = null;
    }
}
