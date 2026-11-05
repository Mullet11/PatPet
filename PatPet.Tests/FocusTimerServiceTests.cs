using PatPet.Services;

namespace PatPet.Tests;

public class FocusTimerServiceTests
{
    private static (FocusTimerService svc, FakeClock clock, List<TimerChange> log) Create()
    {
        var clock = new FakeClock(new DateTime(2026, 11, 5, 9, 0, 0));
        var svc = new FocusTimerService(clock);
        var log = new List<TimerChange>();
        svc.Changed += log.Add;
        return (svc, clock, log);
    }

    [Fact]
    public void Starts_with_any_duration()
    {
        var (svc, _, log) = Create();
        svc.Start(new TimeSpan(1, 23, 45));
        Assert.Equal(TimerState.Running, svc.State);
        Assert.Equal(new TimeSpan(1, 23, 45), svc.Remaining);
        Assert.Single(log);
    }

    [Fact]
    public void Zero_duration_does_not_start()
    {
        var (svc, _, _) = Create();
        svc.Start(TimeSpan.Zero);
        Assert.Equal(TimerState.Idle, svc.State);
    }

    [Fact]
    public void Duration_is_capped_below_24_hours()
    {
        var (svc, _, _) = Create();
        svc.Start(TimeSpan.FromHours(30));
        Assert.Equal(FocusTimerService.MaxDuration, svc.Remaining);
    }

    [Fact]
    public void Completes_once_and_returns_to_idle()
    {
        var (svc, clock, log) = Create();
        svc.Start(TimeSpan.FromSeconds(90));
        clock.Advance(TimeSpan.FromSeconds(89));
        svc.Tick();
        Assert.Equal(TimerState.Running, svc.State);

        clock.Advance(TimeSpan.FromSeconds(1));
        svc.Tick();
        svc.Tick();
        Assert.Equal(TimerState.Idle, svc.State);
        Assert.Single(log, c => c.Completed);
    }

    [Fact]
    public void Pause_freezes_and_resume_continues()
    {
        var (svc, clock, _) = Create();
        svc.Start(TimeSpan.FromMinutes(10));
        clock.Advance(TimeSpan.FromMinutes(3));
        svc.Pause();
        clock.Advance(TimeSpan.FromHours(2));
        svc.Tick();
        Assert.Equal(TimerState.Paused, svc.State);
        Assert.Equal(TimeSpan.FromMinutes(7), svc.Remaining);

        svc.Resume();
        clock.Advance(TimeSpan.FromMinutes(7));
        svc.Tick();
        Assert.Equal(TimerState.Idle, svc.State);
    }

    [Fact]
    public void AddTime_while_running_and_never_below_one_second()
    {
        var (svc, clock, _) = Create();
        svc.Start(TimeSpan.FromMinutes(5));
        clock.Advance(TimeSpan.FromMinutes(1));
        svc.AddTime(TimeSpan.FromMinutes(10));
        Assert.Equal(TimeSpan.FromMinutes(14), svc.Remaining);

        svc.AddTime(TimeSpan.FromHours(-5));
        Assert.Equal(TimeSpan.FromSeconds(1), svc.Remaining);
    }

    [Fact]
    public void Stop_is_not_a_completion()
    {
        var (svc, _, log) = Create();
        svc.Start(TimeSpan.FromMinutes(1));
        svc.Stop();
        Assert.Equal(TimerState.Idle, svc.State);
        Assert.False(log[^1].Completed);
    }

    [Theory]
    [InlineData(0, 25, 0, "25:00")]
    [InlineData(1, 5, 9, "1:05:09")]
    [InlineData(0, 0, 7, "00:07")]
    public void Format_hides_hours_when_zero(int h, int m, int s, string expected) =>
        Assert.Equal(expected, FocusTimerService.Format(new TimeSpan(h, m, s)));
}
