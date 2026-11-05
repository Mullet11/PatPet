using System.Globalization;
using System.Media;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Services;

namespace PatPet.ViewModels;

/// <summary>Satu timer dengan durasi bebas (jam:menit:detik), bisa diubah sebelum dan saat berjalan.</summary>
public partial class TimerViewModel : ObservableObject
{
    private readonly FocusTimerService _svc;
    private readonly INotificationService _notify;
    private readonly SettingsService _settings;
    private readonly DispatcherTimer _saveDebounce;
    private readonly AlarmService _alarm;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStart), nameof(DurationText))]
    private int _hours;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStart), nameof(DurationText))]
    private int _minutes;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanStart), nameof(DurationText))]
    private int _seconds;

    [ObservableProperty] private bool _isActive;
    [ObservableProperty] private bool _isPaused;
    [ObservableProperty] private string _remainingText = "";
    [ObservableProperty] private double _progress;
    [ObservableProperty] private string _startPauseGlyph = "";
    [ObservableProperty] private string _statusText = "Atur durasi lalu mulai";
    [ObservableProperty] private bool _isAlarmRinging;
    [ObservableProperty] private bool _alarmEnabled;

    public TimerViewModel(FocusTimerService svc, INotificationService notify, SettingsService settings, AlarmService alarm)
    {
        _svc = svc;
        _alarm = alarm;
        _alarmEnabled = settings.Current.Timer.AlarmEnabled;
        _alarm.RingingChanged += r => IsAlarmRinging = r;
        _notify = notify;
        _settings = settings;
        _saveDebounce = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(800) };
        _saveDebounce.Tick += (_, _) => SaveDuration();

        var d = TimeSpan.FromSeconds(settings.Current.Timer.DurationSeconds);
        _hours = (int)d.TotalHours;
        _minutes = d.Minutes;
        _seconds = d.Seconds;

        _svc.Changed += OnChanged;
        Refresh();
    }

    public TimeSpan Duration => new(Hours, Minutes, Seconds);
    public bool CanStart => Duration > TimeSpan.Zero;
    public string DurationText => FocusTimerService.Format(Duration);

    // nilai yang diketik manual dibatasi ke rentang yang valid
    partial void OnHoursChanged(int value)
    {
        if (value is < 0 or > 23) { Hours = Math.Clamp(value, 0, 23); return; }
        DurationEdited();
    }

    partial void OnMinutesChanged(int value)
    {
        if (value is < 0 or > 59) { Minutes = Math.Clamp(value, 0, 59); return; }
        DurationEdited();
    }

    partial void OnSecondsChanged(int value)
    {
        if (value is < 0 or > 59) { Seconds = Math.Clamp(value, 0, 59); return; }
        DurationEdited();
    }

    private void DurationEdited()
    {
        _saveDebounce.Stop();
        _saveDebounce.Start();
        if (!IsActive) Refresh();
    }

    private void SaveDuration()
    {
        _saveDebounce.Stop();
        if (Duration <= TimeSpan.Zero) return;
        _settings.Current.Timer.DurationSeconds = (int)Duration.TotalSeconds;
        _settings.Save();
    }

    /// <summary>Set durasi dari total detik, dibatasi 0 – 23:59:59.</summary>
    public void SetDuration(TimeSpan d)
    {
        var secs = (int)Math.Clamp(d.TotalSeconds, 0, FocusTimerService.MaxDuration.TotalSeconds);
        Hours = secs / 3600;
        Minutes = secs / 60 % 60;
        Seconds = secs % 60;
    }

    /// <summary>Naik/turun satu segmen. Parameter: "h+1", "m-5", "s+10", dst. Angka berputar (59 → 00).</summary>
    [RelayCommand]
    private void Step(string? spec)
    {
        if (IsActive || string.IsNullOrEmpty(spec) || spec.Length < 3) return;
        if (!int.TryParse(spec[1..], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var delta)) return;
        switch (spec[0])
        {
            case 'h': Hours = Wrap(Hours + delta, 24); break;
            case 'm': Minutes = Wrap(Minutes + delta, 60); break;
            case 's': Seconds = Wrap(Seconds + delta, 60); break;
        }
    }

    private static int Wrap(int v, int mod) => ((v % mod) + mod) % mod;

    /// <summary>Tambah menit ke durasi (sebelum mulai) atau ke sisa waktu (saat berjalan).</summary>
    [RelayCommand]
    private void AddMinutes(string? minutes)
    {
        if (!int.TryParse(minutes, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var m)) return;
        if (IsActive)
        {
            _svc.AddTime(TimeSpan.FromMinutes(m));
            Refresh();
        }
        else
        {
            SetDuration(Duration + TimeSpan.FromMinutes(m));
        }
    }

    [RelayCommand]
    private void Clear()
    {
        if (!IsActive) SetDuration(TimeSpan.Zero);
    }

    [RelayCommand]
    private void StartPause()
    {
        switch (_svc.State)
        {
            case TimerState.Idle:
                if (!CanStart) return;
                _alarm.Stop();
                SaveDuration();
                _svc.Start(Duration);
                break;
            case TimerState.Running:
                _svc.Pause();
                break;
            case TimerState.Paused:
                _svc.Resume();
                break;
        }
        Refresh();
    }

    [RelayCommand]
    private void Stop()
    {
        _alarm.Stop();
        _svc.Stop();
        Refresh();
    }

    public void Refresh()
    {
        IsActive = _svc.IsActive;
        IsPaused = _svc.State == TimerState.Paused;
        RemainingText = IsActive ? FocusTimerService.Format(_svc.Remaining) : DurationText;
        Progress = IsActive ? _svc.Progress : 0;
        StartPauseGlyph = _svc.State == TimerState.Running ? "" : "";
        StatusText = _svc.State switch
        {
            TimerState.Running => "Selesai pukul " + DateTime.Now.Add(_svc.Remaining).ToString("HH:mm", CultureInfo.InvariantCulture),
            TimerState.Paused => "Dijeda",
            _ => "Atur durasi lalu mulai",
        };
    }

    private void OnChanged(TimerChange c)
    {
        Refresh();
        if (!c.Completed) return;

        if (AlarmEnabled) _alarm.Start(TimeSpan.FromSeconds(_settings.Current.Timer.AlarmSeconds));
        else SystemSounds.Asterisk.Play();
        _notify.Notify("Waktu habis!", $"Timer {FocusTimerService.Format(_svc.Duration)} selesai."
                                       + (AlarmEnabled ? " Klik bar atau kucing untuk matikan alarm." : ""));
    }

    partial void OnAlarmEnabledChanged(bool value)
    {
        _settings.Current.Timer.AlarmEnabled = value;
        _settings.Save();
        if (!value) _alarm.Stop();
    }

    [RelayCommand]
    private void StopAlarm() => _alarm.Stop();

    /// <summary>Coba dengar bunyi alarm selama 3 detik.</summary>
    [RelayCommand]
    private void TestAlarm()
    {
        if (_alarm.IsRinging) _alarm.Stop();
        else _alarm.Start(TimeSpan.FromSeconds(3));
    }
}
