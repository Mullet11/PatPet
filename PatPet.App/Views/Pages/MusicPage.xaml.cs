using System.ComponentModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Threading;
using PatPet.ViewModels;

namespace PatPet.Views.Pages;

public partial class MusicPage : UserControl
{
    private readonly DispatcherTimer _lyricTimer;
    private NowPlayingViewModel? _np;

    public MusicPage()
    {
        InitializeComponent();
        // Slider menandai klik sebagai "handled" (IsMoveToPointEnabled), jadi daftarkan dengan handledEventsToo.
        Seek.AddHandler(PreviewMouseLeftButtonDownEvent, new MouseButtonEventHandler(OnSeekDown), handledEventsToo: true);
        Seek.AddHandler(PreviewMouseLeftButtonUpEvent, new MouseButtonEventHandler(OnSeekUp), handledEventsToo: true);
        Seek.AddHandler(LostMouseCaptureEvent, new MouseEventHandler(OnSeekLostCapture), handledEventsToo: true);

        // sorot baris lirik ±4×/detik, hanya selama halaman ini terlihat
        _lyricTimer = new DispatcherTimer(DispatcherPriority.Background) { Interval = TimeSpan.FromMilliseconds(250) };
        _lyricTimer.Tick += (_, _) => _np?.UpdateLyricPosition();

        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _np = (DataContext as MainViewModel)?.NowPlaying;
        if (_np is null) return;
        _np.PropertyChanged += OnNowPlayingChanged;
        _np.LyricsActive = true;
        _lyricTimer.Start();
        Dispatcher.InvokeAsync(ScrollToCurrent, DispatcherPriority.Loaded);
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        // lepas semua langganan agar halaman bisa di-GC
        _lyricTimer.Stop();
        if (_np is null) return;
        _np.PropertyChanged -= OnNowPlayingChanged;
        _np.LyricsActive = false;
        _np = null;
    }

    private void OnNowPlayingChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(NowPlayingViewModel.CurrentLyricIndex))
            Dispatcher.InvokeAsync(ScrollToCurrent, DispatcherPriority.Loaded);
    }

    /// <summary>Gulir halus agar baris yang sedang dinyanyikan berada di tengah kotak lirik.</summary>
    private void ScrollToCurrent()
    {
        if (_np is null) return;
        var idx = _np.CurrentLyricIndex;
        if (idx < 0)
        {
            LyricsScroll.ScrollToVerticalOffset(0);
            return;
        }
        if (LyricsList.ItemContainerGenerator.ContainerFromIndex(idx) is not FrameworkElement item) return;
        try
        {
            var y = item.TransformToAncestor(LyricsScroll).Transform(new Point(0, 0)).Y;
            var target = LyricsScroll.VerticalOffset + y - (LyricsScroll.ViewportHeight - item.ActualHeight) / 2;
            LyricsScroll.ScrollToVerticalOffset(Math.Max(0, target));
        }
        catch (InvalidOperationException)
        {
            // item belum berada di pohon visual
        }
    }

    private void OnSeekDown(object sender, MouseButtonEventArgs e) => _np?.BeginSeek();

    private async void OnSeekUp(object sender, MouseButtonEventArgs e)
    {
        if (_np is { } np) await np.EndSeekAsync();
    }

    private async void OnSeekLostCapture(object sender, MouseEventArgs e)
    {
        if (_np is { IsSeeking: true } np && Mouse.LeftButton == MouseButtonState.Released)
            await np.EndSeekAsync();
    }
}
