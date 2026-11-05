using System.Text.Json;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using PatPet.Models;

namespace PatPet.Views.Controls;

/// <summary>
/// Menggambar sprite sheet pixel art (strip horizontal) sesuai pet_manifest.json.
/// Timer berhenti saat kontrol tidak terlihat → CPU ≈ 0%.
/// </summary>
public sealed class SpriteAnimator : FrameworkElement
{
    private static PetManifest? _manifest;
    private static readonly Dictionary<string, BitmapSource[]> FrameCache = new();

    public static readonly DependencyProperty AnimationProperty = DependencyProperty.Register(
        nameof(Animation), typeof(string), typeof(SpriteAnimator),
        new FrameworkPropertyMetadata("idle", OnAnimationInputChanged));

    public static readonly DependencyProperty PlayOnceProperty = DependencyProperty.Register(
        nameof(PlayOnce), typeof(bool), typeof(SpriteAnimator),
        new FrameworkPropertyMetadata(false, OnAnimationInputChanged));

    public static readonly DependencyProperty RestartTokenProperty = DependencyProperty.Register(
        nameof(RestartToken), typeof(int), typeof(SpriteAnimator),
        new FrameworkPropertyMetadata(0, OnAnimationInputChanged));

    public static readonly DependencyProperty ScaleProperty = DependencyProperty.Register(
        nameof(Scale), typeof(double), typeof(SpriteAnimator),
        new FrameworkPropertyMetadata(3.0, FrameworkPropertyMetadataOptions.AffectsMeasure | FrameworkPropertyMetadataOptions.AffectsRender));

    public static readonly DependencyProperty CompletedCommandProperty = DependencyProperty.Register(
        nameof(CompletedCommand), typeof(ICommand), typeof(SpriteAnimator));

    private readonly DispatcherTimer _timer;
    private BitmapSource[] _frames = Array.Empty<BitmapSource>();
    private int _index;
    private bool _loop = true;
    private bool _done;

    public SpriteAnimator()
    {
        _timer = new DispatcherTimer(DispatcherPriority.Render);
        _timer.Tick += OnTick;
        RenderOptions.SetBitmapScalingMode(this, BitmapScalingMode.NearestNeighbor);
        Loaded += (_, _) => LoadAnimation();
        Unloaded += (_, _) => _timer.Stop();
        IsVisibleChanged += (_, _) => UpdateTimer();
    }

    public string Animation { get => (string)GetValue(AnimationProperty); set => SetValue(AnimationProperty, value); }
    public bool PlayOnce { get => (bool)GetValue(PlayOnceProperty); set => SetValue(PlayOnceProperty, value); }
    public int RestartToken { get => (int)GetValue(RestartTokenProperty); set => SetValue(RestartTokenProperty, value); }
    public double Scale { get => (double)GetValue(ScaleProperty); set => SetValue(ScaleProperty, value); }
    public ICommand? CompletedCommand { get => (ICommand?)GetValue(CompletedCommandProperty); set => SetValue(CompletedCommandProperty, value); }

    public event EventHandler? Completed;

    private static PetManifest Manifest => _manifest ??= LoadManifest();

    private static PetManifest LoadManifest()
    {
        try
        {
            using var s = typeof(SpriteAnimator).Assembly.GetManifestResourceStream("pet_manifest.json");
            if (s is null) return new PetManifest();
            return JsonSerializer.Deserialize<PetManifest>(s, new JsonSerializerOptions { PropertyNameCaseInsensitive = true })
                   ?? new PetManifest();
        }
        catch
        {
            return new PetManifest();
        }
    }

    private static BitmapSource[] GetFrames(string name, PetAnimation anim)
    {
        if (FrameCache.TryGetValue(name, out var cached)) return cached;

        var sheet = new BitmapImage();
        sheet.BeginInit();
        sheet.UriSource = new Uri($"pack://application:,,,/PatPet;component/Assets/Pet/{anim.File}", UriKind.Absolute);
        sheet.CacheOption = BitmapCacheOption.OnLoad;
        sheet.EndInit();
        sheet.Freeze();

        int w = Manifest.FrameWidth, h = Manifest.FrameHeight;
        var count = Math.Min(anim.Frames, sheet.PixelWidth / w);
        var frames = new BitmapSource[Math.Max(count, 0)];
        for (var i = 0; i < frames.Length; i++)
        {
            var crop = new CroppedBitmap(sheet, new Int32Rect(i * w, 0, w, h));
            crop.Freeze();
            frames[i] = crop;
        }
        FrameCache[name] = frames;
        return frames;
    }

    private static void OnAnimationInputChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is SpriteAnimator s && s.IsLoaded) s.LoadAnimation();
    }

    private void LoadAnimation()
    {
        try
        {
            var name = string.IsNullOrEmpty(Animation) ? "idle" : Animation;
            if (!Manifest.Animations.TryGetValue(name, out var anim))
            {
                if (!Manifest.Animations.TryGetValue("idle", out anim)) return;
                name = "idle";
            }

            _frames = GetFrames(name, anim);
            _index = 0;
            _done = false;
            _loop = anim.Loop && !PlayOnce;
            _timer.Interval = TimeSpan.FromMilliseconds(1000.0 / Math.Max(1, anim.Fps));
        }
        catch
        {
            _frames = Array.Empty<BitmapSource>();
        }
        InvalidateVisual();
        UpdateTimer();
    }

    private void UpdateTimer()
    {
        if (IsVisible && IsLoaded && !_done && _frames.Length > 1) _timer.Start();
        else _timer.Stop();
    }

    private void OnTick(object? sender, EventArgs e)
    {
        _index++;
        if (_index >= _frames.Length)
        {
            if (_loop)
            {
                _index = 0;
            }
            else
            {
                _index = _frames.Length - 1;
                _done = true;
                _timer.Stop();
                InvalidateVisual();
                Completed?.Invoke(this, EventArgs.Empty);
                if (CompletedCommand?.CanExecute(null) == true) CompletedCommand.Execute(null);
                return;
            }
        }
        InvalidateVisual();
    }

    protected override Size MeasureOverride(Size availableSize) =>
        new(Manifest.FrameWidth * Scale, Manifest.FrameHeight * Scale);

    protected override void OnRender(DrawingContext dc)
    {
        var rect = new Rect(0, 0, Manifest.FrameWidth * Scale, Manifest.FrameHeight * Scale);
        dc.DrawRectangle(Brushes.Transparent, null, rect); // area klik penuh
        if (_frames.Length > 0) dc.DrawImage(_frames[Math.Clamp(_index, 0, _frames.Length - 1)], rect);
    }
}
