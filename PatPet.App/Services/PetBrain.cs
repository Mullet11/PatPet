using PatPet.Models;

namespace PatPet.Services;

/// <summary>
/// State machine pet. Hanya menerima sinyal dan mengeluarkan <see cref="CurrentState"/>.
/// Prioritas: happy &gt; focus &gt; music &gt; yawn &gt; sleep &gt; idle.
/// </summary>
public sealed class PetBrain
{
    private readonly IClock _clock;
    private bool _isPlaying;
    private bool _isFocus;
    private TimeSpan _idle;
    private PetState? _oneShot;
    private DateTime _oneShotStarted;
    private DateTime _lastChange = DateTime.MinValue;
    private DateTime _lastClick = DateTime.MinValue;
    private DateTime _lastYawn = DateTime.MinValue;

    public PetBrain(IClock clock) => _clock = clock;

    public TimeSpan IdleSleepThreshold { get; set; } = TimeSpan.FromMinutes(5);
    public TimeSpan MinimumHold { get; init; } = TimeSpan.FromSeconds(1.5);
    public TimeSpan ClickCooldown { get; init; } = TimeSpan.FromSeconds(2);
    public TimeSpan YawnInterval { get; init; } = TimeSpan.FromMinutes(10);
    /// <summary>Batas aman one-shot bila event selesai animasi tidak datang (mis. jendela tersembunyi).</summary>
    public TimeSpan OneShotTimeout { get; init; } = TimeSpan.FromSeconds(4);

    public PetState CurrentState { get; private set; } = PetState.Idle;

    /// <summary>Dinaikkan saat state berganti, atau saat one-shot yang sama dipicu ulang.</summary>
    public event Action<PetState>? StateChanged;

    public static bool IsNight(DateTime t) => t.Hour >= 23 || t.Hour < 4;

    public void MusicChanged(bool isPlaying)
    {
        _isPlaying = isPlaying;
        Update();
    }

    /// <summary>Timer fokus berjalan/berhenti; <paramref name="completed"/> = selesai sampai nol.</summary>
    public void FocusChanged(bool active, bool completed = false)
    {
        _isFocus = active;
        if (completed) TriggerHappy();
        Update();
    }

    public void NoteAdded()
    {
        TriggerHappy();
        Update();
    }

    /// <returns>false bila masih dalam cooldown klik.</returns>
    public bool PetClicked()
    {
        var now = _clock.Now;
        if (now - _lastClick < ClickCooldown) return false;
        _lastClick = now;
        TriggerHappy();
        Update();
        return true;
    }

    public void UserIdle(TimeSpan idle)
    {
        _idle = idle;
        Update();
    }

    public void TimeTick(TimeSpan idle)
    {
        _idle = idle;
        var now = _clock.Now;

        if (_oneShot is not null && now - _oneShotStarted >= OneShotTimeout)
            _oneShot = null;

        if (_oneShot is null && IsNight(now) && !_isFocus && !_isPlaying
            && _idle < IdleSleepThreshold && now - _lastYawn >= YawnInterval)
        {
            _oneShot = PetState.Yawn;
            _oneShotStarted = now;
            _lastYawn = now;
        }

        Update();
    }

    public void OneShotCompleted()
    {
        if (_oneShot is null) return;
        _oneShot = null;
        Update(force: true);
    }

    public PetState Evaluate()
    {
        if (_oneShot == PetState.Happy) return PetState.Happy;
        if (_isFocus) return PetState.Focus;
        if (_isPlaying) return PetState.Music;
        if (_oneShot == PetState.Yawn) return PetState.Yawn;
        if (_idle >= IdleSleepThreshold) return PetState.Sleep;
        return PetState.Idle;
    }

    private void TriggerHappy()
    {
        _oneShot = PetState.Happy;
        _oneShotStarted = _clock.Now;
        if (CurrentState == PetState.Happy) StateChanged?.Invoke(PetState.Happy); // mainkan ulang
    }

    private void Update(bool force = false)
    {
        var next = Evaluate();
        if (_oneShot == PetState.Yawn && next != PetState.Yawn) _oneShot = null; // yawn kalah prioritas
        if (next == CurrentState) return;

        var now = _clock.Now;
        var bypassHold = force
                         || next == PetState.Happy
                         || CurrentState == PetState.Happy
                         || _lastChange == DateTime.MinValue
                         || now - _lastChange >= MinimumHold;
        if (!bypassHold) return; // anti-kedip; dievaluasi ulang pada tick berikutnya

        CurrentState = next;
        _lastChange = now;
        StateChanged?.Invoke(next);
    }
}
