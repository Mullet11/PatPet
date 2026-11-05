namespace PatPet.Services;

public enum TimerState { Idle, Running, Paused }

public sealed record TimerChange(TimerState State, bool Completed);

/// <summary>
/// Satu timer hitung mundur dengan durasi bebas. Sisa waktu dihitung dari target DateTime
/// (tidak drift, tetap benar setelah sleep), dan <see cref="Tick"/> dipanggil oleh jam aplikasi.
/// </summary>
public sealed class FocusTimerService
{
    public static readonly TimeSpan MaxDuration = TimeSpan.FromHours(24) - TimeSpan.FromSeconds(1);

    private readonly IClock _clock;
    private DateTime _target;
    private TimeSpan _pausedRemaining;

    public FocusTimerService(IClock clock) => _clock = clock;

    public TimerState State { get; private set; } = TimerState.Idle;
    public bool IsActive => State != TimerState.Idle;
    /// <summary>Total durasi putaran saat ini (bertambah/berkurang bila diubah saat berjalan).</summary>
    public TimeSpan Duration { get; private set; }

    public event Action<TimerChange>? Changed;

    public TimeSpan Remaining
    {
        get
        {
            if (State == TimerState.Idle) return TimeSpan.Zero;
            if (State == TimerState.Paused) return _pausedRemaining;
            var r = _target - _clock.Now;
            return r < TimeSpan.Zero ? TimeSpan.Zero : r;
        }
    }

    public double Progress =>
        Duration <= TimeSpan.Zero ? 0 : Math.Clamp(1 - Remaining.TotalSeconds / Duration.TotalSeconds, 0, 1);

    public void Start(TimeSpan duration)
    {
        if (State != TimerState.Idle || duration <= TimeSpan.Zero) return;
        if (duration > MaxDuration) duration = MaxDuration;
        Duration = duration;
        _target = _clock.Now + duration;
        State = TimerState.Running;
        Changed?.Invoke(new TimerChange(State, false));
    }

    public void Pause()
    {
        if (State != TimerState.Running) return;
        _pausedRemaining = Remaining;
        State = TimerState.Paused;
        Changed?.Invoke(new TimerChange(State, false));
    }

    public void Resume()
    {
        if (State != TimerState.Paused) return;
        _target = _clock.Now + _pausedRemaining;
        State = TimerState.Running;
        Changed?.Invoke(new TimerChange(State, false));
    }

    public void Stop()
    {
        if (State == TimerState.Idle) return;
        State = TimerState.Idle;
        Changed?.Invoke(new TimerChange(State, false));
    }

    /// <summary>Tambah/kurangi sisa waktu (minimal tersisa 1 detik).</summary>
    public void AddTime(TimeSpan delta)
    {
        if (State == TimerState.Idle) return;
        var current = Remaining;
        var remaining = current + delta;
        if (remaining < TimeSpan.FromSeconds(1)) remaining = TimeSpan.FromSeconds(1);
        if (remaining > MaxDuration) remaining = MaxDuration;

        if (State == TimerState.Paused) _pausedRemaining = remaining;
        else _target = _clock.Now + remaining;

        Duration += remaining - current;
        if (Duration < remaining) Duration = remaining;
    }

    public void Tick()
    {
        if (State == TimerState.Running && _clock.Now >= _target)
        {
            State = TimerState.Idle;
            Changed?.Invoke(new TimerChange(State, true));
        }
    }

    /// <summary>"25:00" atau "1:05:09".</summary>
    public static string Format(TimeSpan t)
    {
        t = TimeSpan.FromSeconds(Math.Ceiling(Math.Max(0, t.TotalSeconds)));
        return t.TotalHours >= 1
            ? $"{(int)t.TotalHours}:{t.Minutes:00}:{t.Seconds:00}"
            : $"{t.Minutes:00}:{t.Seconds:00}";
    }
}
