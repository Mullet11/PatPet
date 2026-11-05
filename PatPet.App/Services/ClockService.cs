using System.Windows.Threading;

namespace PatPet.Services;

/// <summary>Tick 1 detik yang disejajarkan ke awal detik berikutnya (tanpa drift).</summary>
public sealed class ClockService : IDisposable
{
    private readonly DispatcherTimer _timer;

    public event Action<DateTime>? Tick;

    public ClockService()
    {
        _timer = new DispatcherTimer(DispatcherPriority.Normal);
        _timer.Tick += OnTimer;
    }

    public void Start() => ScheduleNext();

    public void Stop() => _timer.Stop();

    private void ScheduleNext()
    {
        var ms = 1000 - DateTime.Now.Millisecond;
        _timer.Interval = TimeSpan.FromMilliseconds(Math.Max(ms, 20) + 5);
        _timer.Start();
    }

    private void OnTimer(object? sender, EventArgs e)
    {
        _timer.Stop();
        Tick?.Invoke(DateTime.Now);
        ScheduleNext();
    }

    public void Dispose() => _timer.Stop();
}
