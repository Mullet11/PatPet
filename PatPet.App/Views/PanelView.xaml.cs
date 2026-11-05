using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;
using PatPet.ViewModels;
using Page = PatPet.ViewModels.Page;

namespace PatPet.Views;

public partial class PanelView : UserControl
{
    private static readonly Duration TransitionDuration = TimeSpan.FromMilliseconds(260);
    private const double SlideDistance = 36;

    private MainViewModel? _vm;
    private Page _shownPage;

    public PanelView()
    {
        InitializeComponent();
        Loaded += (_, _) => Attach();
        Unloaded += (_, _) => Detach(); // lepas langganan agar tray yang ditutup bisa di-GC
        DataContextChanged += (_, _) => { Detach(); if (IsLoaded) Attach(); };
    }

    /// <summary>Mode bar: panel menyambung tepat di bawah bar (sudut atas rata, tanpa garis atas).</summary>
    public void SetAttached(bool attached)
    {
        Card.CornerRadius = attached ? new CornerRadius(0, 0, 18, 18) : new CornerRadius(18);
        Card.BorderThickness = attached ? new Thickness(0) : new Thickness(1);
        Card.Padding = attached ? new Thickness(12, 2, 12, 12) : new Thickness(12, 8, 12, 12);
    }

    private void Attach()
    {
        if (_vm is not null) return;
        _vm = DataContext as MainViewModel;
        if (_vm is null) return;
        _shownPage = _vm.CurrentPage;
        _vm.PropertyChanged += OnVmPropertyChanged;
    }

    private void Detach()
    {
        if (_vm is not null) _vm.PropertyChanged -= OnVmPropertyChanged;
        _vm = null;
    }

    private void OnVmPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(MainViewModel.CurrentPage) || _vm is null) return;
        var next = _vm.CurrentPage;
        if (next == _shownPage) return;
        var forward = next > _shownPage;
        _shownPage = next;
        AnimateTransition(forward);
    }

    /// <summary>
    /// Halaman baru masuk dengan geser + fade dari arah tab yang dituju, dan tinggi tray
    /// berpindah halus dari tinggi lama ke tinggi baru (tidak melompat).
    /// Kecepatan frame mengikuti refresh rate monitor (lihat App.OnStartup).
    /// </summary>
    private void AnimateTransition(bool forward)
    {
        // kunci tinggi lama sebelum layout halaman baru terjadi
        var fromHeight = PageClip.ActualHeight;
        PageClip.BeginAnimation(HeightProperty, null);
        PageClip.Height = fromHeight;

        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        PageShift.BeginAnimation(TranslateTransform.XProperty,
            new DoubleAnimation(forward ? SlideDistance : -SlideDistance, 0, TransitionDuration) { EasingFunction = ease });
        PageHost.BeginAnimation(OpacityProperty,
            new DoubleAnimation(0, 1, TransitionDuration) { EasingFunction = ease });

        // ukur tinggi halaman baru setelah ia dibuat, lalu animasikan tinggi
        Dispatcher.InvokeAsync(() =>
        {
            PageHost.Measure(new Size(PageClip.ActualWidth > 0 ? PageClip.ActualWidth : double.PositiveInfinity, double.PositiveInfinity));
            var toHeight = PageHost.DesiredSize.Height;
            if (Math.Abs(toHeight - fromHeight) < 1)
            {
                ReleaseHeight();
                return;
            }
            var grow = new DoubleAnimation(fromHeight, toHeight, TransitionDuration) { EasingFunction = ease };
            grow.Completed += (_, _) => ReleaseHeight();
            PageClip.BeginAnimation(HeightProperty, grow);
        }, DispatcherPriority.Loaded);
    }

    private void ReleaseHeight()
    {
        PageClip.BeginAnimation(HeightProperty, null);
        PageClip.Height = double.NaN; // kembali mengikuti isi (mis. catatan bertambah)
    }
}
