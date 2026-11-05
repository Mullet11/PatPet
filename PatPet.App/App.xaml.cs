using System.Windows;
using System.Windows.Threading;
using PatPet.Services;
using PatPet.ViewModels;
using PatPet.Views;

namespace PatPet;

public partial class App : Application
{
    private const string MutexName = @"Local\PatPet_SingleInstance";
    private const long MaxLogBytes = 512 * 1024;

    private Mutex? _mutex;
    private bool _exiting;

    private SettingsService? _settings;
    private ClockService? _clock;
    private SpotifyMediaService? _spotify;
    private WeatherService? _weather;
    private LyricsService? _lyrics;
    private SystemStatusService? _system;
    private AlarmService? _alarm;
    private TrayService? _tray;
    private MainViewModel? _vm;

    protected override async void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _mutex = new Mutex(true, MutexName, out var createdNew);
        if (!createdNew)
        {
            MessageBox.Show("PatPet sudah jalan. Lihat bar di atas layar atau ikon PatPet di system tray.", "PatPet",
                MessageBoxButton.OK, MessageBoxImage.Information);
            _mutex.Dispose();
            _mutex = null;
            Shutdown();
            return;
        }

        // Render software: widget kecil dengan animasi 4–8 fps tidak butuh GPU, dan tanpa perangkat
        // Direct3D memori turun ±60 MB → ±28 MB (terukur di Task Manager).
        System.Windows.Media.RenderOptions.ProcessRenderMode = System.Windows.Interop.RenderMode.SoftwareOnly;

        // Animasi (transisi halaman, bar melebar, dll.) mengikuti refresh rate monitor, mis. 144 Hz,
        // bukan batas bawaan WPF ±60 fps. Animasi hanya berjalan sesaat, jadi CPU saat diam tetap rendah.
        System.Windows.Media.Animation.Timeline.DesiredFrameRateProperty.OverrideMetadata(
            typeof(System.Windows.Media.Animation.Timeline),
            new FrameworkPropertyMetadata((int?)Interop.Win32.GetRefreshRate()));

        Directory.CreateDirectory(AppPaths.DataDir);
        DispatcherUnhandledException += OnUnhandledException;
        TaskScheduler.UnobservedTaskException += (_, args) => { Log(args.Exception); args.SetObserved(); };

        // ---- komposisi manual (tanpa container DI) ----
        _settings = new SettingsService();
        _settings.Load();

        var clockSource = new SystemClock();
        _clock = new ClockService();
        _spotify = new SpotifyMediaService();
        _weather = new WeatherService();
        _lyrics = new LyricsService();
        _system = new SystemStatusService();
        _alarm = new AlarmService();
        _tray = new TrayService();
        var notes = NoteRepository.ForFile(AppPaths.NotesDb);
        var timer = new FocusTimerService(clockSource);
        var brain = new PetBrain(clockSource);

        try { notes.BackupDaily(AppPaths.BackupDir); }
        catch (Exception ex) { Log(ex); }

        var clockVm = new ClockViewModel(_weather, _settings);
        _vm = new MainViewModel(
            clockVm,
            new NowPlayingViewModel(_spotify, _lyrics, _settings),
            new NotesViewModel(notes, _tray),
            new TimerViewModel(timer, _tray, _settings, _alarm),
            new PetViewModel(brain),
            new SettingsViewModel(_settings, brain, _weather, clockVm),
            new SystemStatusViewModel(_system),
            _settings,
            _clock,
            new ActivityService(),
            timer,
            brain);

        var window = new MainWindow(_settings) { DataContext = _vm };
        window.Show();
        _tray.Initialize(window, _vm);

        try { await _vm.InitializeAsync(); }
        catch (Exception ex) { Log(ex); }
    }

    public async Task ExitAppAsync()
    {
        if (_exiting) return;
        _exiting = true;
        if (_vm is not null)
        {
            try { await _vm.ShutdownAsync(); }
            catch (Exception ex) { Log(ex); }
        }
        Shutdown();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _tray?.Dispose();
        _spotify?.Dispose();
        _clock?.Dispose();
        _weather?.Dispose();
        _lyrics?.Dispose();
        _system?.Dispose();
        _alarm?.Dispose();
        if (_mutex is not null)
        {
            try { _mutex.ReleaseMutex(); } catch { }
            _mutex.Dispose();
        }
        base.OnExit(e);
    }

    private void OnUnhandledException(object sender, DispatcherUnhandledExceptionEventArgs e)
    {
        Log(e.Exception);
        e.Handled = true; // jangan sampai widget hilang karena error kecil
    }

    internal static void Log(Exception ex)
    {
        try
        {
            var path = AppPaths.ErrorLog;
            var fi = new FileInfo(path);
            if (fi.Exists && fi.Length > MaxLogBytes)
                File.Move(path, path + ".old", overwrite: true); // log tidak tumbuh tanpa batas
            File.AppendAllText(path, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] {ex.GetType().Name}: {ex.Message}{Environment.NewLine}{ex.StackTrace}{Environment.NewLine}{Environment.NewLine}");
        }
        catch { }
    }
}
