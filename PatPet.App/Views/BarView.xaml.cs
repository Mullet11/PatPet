using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using PatPet.ViewModels;

namespace PatPet.Views;

/// <summary>
/// Bar hitam di tengah atas layar. Saat diam hanya kucing dan jam; saat hover atau tray terbuka
/// bar melebar dan menampilkan status lain. Animasi hanya berjalan sesaat.
/// </summary>
public partial class BarView : UserControl
{
    private const double CompactWidth = 112;
    private const double CompactAlarmWidth = 214; // muat chip "Waktu habis"
    /// <summary>Tinggi baris utama bar (tanpa detail hover).</summary>
    public const double RowHeight = 34;
    public const double ToastWidth = 300;
    private const double ToastFillMax = 30;

    private const double HoverWidth = 380;
    private const double DetailsHeight = 38;

    private readonly DispatcherTimer _hoverDelay;
    private MainViewModel? _vm;
    private bool _expanded;
    private bool _toastShowing;
    private bool _hoverOpen;
    private int _toastId;
    private int _extrasId;

    /// <summary>Lebar bar saat diam: kucing + jam, ditambah chip alarm bila sedang berbunyi.</summary>
    private double CollapsedWidth => _vm?.Timer.IsAlarmRinging == true ? CompactAlarmWidth : CompactWidth;

    public BarView()
    {
        InitializeComponent();

        // hover: bar sedikit melebar & memanjang menampilkan cuaca + lagu
        _hoverDelay = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(350) };
        _hoverDelay.Tick += (_, _) => { _hoverDelay.Stop(); OpenHover(); };
        Bar.MouseEnter += (_, _) => { if (!_expanded) _hoverDelay.Start(); };
        Bar.MouseLeave += (_, _) => CloseHover();
        Loaded += (_, _) => Attach();
        Unloaded += (_, _) => Detach(); // lepas event agar tidak bocor
        DataContextChanged += (_, _) => { Detach(); if (IsLoaded) Attach(); };
    }

    private void Attach()
    {
        if (_vm is not null) return;
        _vm = DataContext as MainViewModel;
        if (_vm is null) return;
        _vm.System.PowerSourceChanged += OnPowerSourceChanged;
        _vm.Timer.PropertyChanged += OnTimerPropertyChanged;
        Bar.Width = CollapsedWidth;
    }

    private void OnTimerPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        // alarm mulai/berhenti: sesuaikan lebar bar kecil
        if (e.PropertyName == nameof(TimerViewModel.IsAlarmRinging) && !_expanded && !_hoverOpen && !_toastShowing)
            AnimateWidth(CollapsedWidth, 200);
    }

    /// <summary>Tampilkan atau sembunyikan status lain (timer, musik, baterai, suhu).</summary>
    private async void ShowExtras(bool show, bool immediate = false)
    {
        var id = ++_extrasId;
        if (show)
        {
            Extras.Visibility = Visibility.Visible;
            Extras.BeginAnimation(OpacityProperty, new DoubleAnimation(1, TimeSpan.FromMilliseconds(180))
            {
                BeginTime = TimeSpan.FromMilliseconds(immediate ? 0 : 70),
            });
            return;
        }
        if (immediate)
        {
            Extras.BeginAnimation(OpacityProperty, null);
            Extras.Opacity = 0;
            Extras.Visibility = Visibility.Collapsed;
            return;
        }
        Extras.BeginAnimation(OpacityProperty, new DoubleAnimation(0, TimeSpan.FromMilliseconds(120)));
        await Task.Delay(200); // tunggu bar selesai mengecil agar status tidak terlihat terpotong
        if (id == _extrasId) Extras.Visibility = Visibility.Collapsed;
    }

    private void OpenHover()
    {
        if (_hoverOpen || _expanded || _toastShowing || !Bar.IsMouseOver || _vm is null) return;
        _hoverOpen = true;
        _vm.IsHoverOpen = true;

        AnimateWidth(HoverWidth, 220);
        ShowExtras(true);
        Details.BeginAnimation(HeightProperty, new DoubleAnimation(DetailsHeight, TimeSpan.FromMilliseconds(220))
        {
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
        });
        Details.BeginAnimation(OpacityProperty, new DoubleAnimation(1, TimeSpan.FromMilliseconds(200))
        {
            BeginTime = TimeSpan.FromMilliseconds(80),
        });
    }

    /// <param name="immediate">true = langsung (tanpa animasi), mis. saat tray dibuka.</param>
    public void CloseHover(bool immediate = false)
    {
        _hoverDelay.Stop();
        if (_vm is not null) _vm.IsHoverOpen = false;
        if (!_hoverOpen) return;
        _hoverOpen = false;

        if (immediate)
        {
            Details.BeginAnimation(HeightProperty, null);
            Details.BeginAnimation(OpacityProperty, null);
            Details.Height = 0;
            Details.Opacity = 0;
        }
        else
        {
            Details.BeginAnimation(OpacityProperty, new DoubleAnimation(0, TimeSpan.FromMilliseconds(100)));
            Details.BeginAnimation(HeightProperty, new DoubleAnimation(0, TimeSpan.FromMilliseconds(180))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseIn },
            });
        }
        if (!_expanded && !_toastShowing)
        {
            AnimateWidth(CollapsedWidth, 200);
            ShowExtras(false, immediate);
        }
    }

    private void Detach()
    {
        CloseHover();
        if (_vm is not null)
        {
            _vm.System.PowerSourceChanged -= OnPowerSourceChanged;
            _vm.Timer.PropertyChanged -= OnTimerPropertyChanged;
        }
        _vm = null;
    }

    /// <summary>Saat panel terbuka, bar melebar menyatu dengan panel di bawahnya.</summary>
    public void SetExpanded(bool expanded, double panelWidth)
    {
        _expanded = expanded;
        if (expanded) CloseHover(immediate: true);
        Bar.CornerRadius = expanded ? new CornerRadius(0) : new CornerRadius(0, 0, 16, 16);
        AnimateWidth(expanded ? panelWidth : CollapsedWidth, 180);
        ShowExtras(expanded, immediate: expanded); // tray terbuka: semua status terlihat di bar
    }

    private void AnimateWidth(double to, int ms) =>
        Bar.BeginAnimation(WidthProperty, new DoubleAnimation(to, TimeSpan.FromMilliseconds(ms))
        {
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
        });

    private async void OnPowerSourceChanged(bool plugged)
    {
        var id = ++_toastId;
        var pct = _vm?.System.BatteryPercent ?? 0;
        _toastShowing = true;
        CloseHover(immediate: true);

        ToastText.Text = plugged ? "Mengisi daya" : "Charger dicabut";
        ToastIcon.Text = plugged ? "" : "";
        ToastIcon.Foreground = (Brush)FindResource(plugged ? "Accent2Brush" : "WarnBrush");
        ToastFill.Background = (Brush)FindResource(plugged ? "Accent2Brush" : (pct <= 20 ? "DangerBrush" : "TextBrush"));

        Toast.Visibility = Visibility.Visible;
        if (!_expanded) AnimateWidth(ToastWidth, 220);
        Normal.BeginAnimation(OpacityProperty, new DoubleAnimation(0, TimeSpan.FromMilliseconds(150)));
        Toast.BeginAnimation(OpacityProperty, new DoubleAnimation(1, TimeSpan.FromMilliseconds(200)));

        var full = Math.Clamp(pct, 0, 100) / 100.0 * ToastFillMax;
        if (plugged)
        {
            // baterai "terisi" dari kosong ke persentase saat ini, petir berdenyut
            ToastFill.BeginAnimation(WidthProperty, new DoubleAnimation(0, full, TimeSpan.FromMilliseconds(900))
            {
                EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut },
            });
            var pulse = new DoubleAnimation(1, 1.35, TimeSpan.FromMilliseconds(260)) { AutoReverse = true, RepeatBehavior = new RepeatBehavior(3) };
            ToastIconScale.BeginAnimation(ScaleTransform.ScaleXProperty, pulse);
            ToastIconScale.BeginAnimation(ScaleTransform.ScaleYProperty, pulse);
        }
        else
        {
            // isi baterai berkedip sekali dua kali
            ToastFill.BeginAnimation(WidthProperty, null);
            ToastFill.Width = full;
            ToastFill.BeginAnimation(OpacityProperty, new DoubleAnimation(1, 0.2, TimeSpan.FromMilliseconds(220))
            {
                AutoReverse = true,
                RepeatBehavior = new RepeatBehavior(3),
            });
        }

        await Task.Delay(2600);
        if (id != _toastId) return; // ada notifikasi baru

        Toast.BeginAnimation(OpacityProperty, new DoubleAnimation(0, TimeSpan.FromMilliseconds(200)));
        Normal.BeginAnimation(OpacityProperty, new DoubleAnimation(1, TimeSpan.FromMilliseconds(250)));
        if (!_expanded) AnimateWidth(CollapsedWidth, 220);
        await Task.Delay(220);
        if (id != _toastId) return;

        Toast.Visibility = Visibility.Collapsed;
        _toastShowing = false;
        // hentikan semua animasi agar tidak ada yang menahan render
        ToastFill.BeginAnimation(WidthProperty, null);
        ToastFill.BeginAnimation(OpacityProperty, null);
        ToastIconScale.BeginAnimation(ScaleTransform.ScaleXProperty, null);
        ToastIconScale.BeginAnimation(ScaleTransform.ScaleYProperty, null);
    }

    /// <summary>Untuk pengujian manual tampilan animasi.</summary>
    internal void PreviewToast(bool plugged) => OnPowerSourceChanged(plugged);
}
