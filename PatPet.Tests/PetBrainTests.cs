using PatPet.Models;
using PatPet.Services;

namespace PatPet.Tests;

public class PetBrainTests
{
    private static readonly DateTime Afternoon = new(2026, 11, 5, 14, 0, 0);
    private static readonly DateTime LateNight = new(2026, 11, 5, 23, 30, 0);

    private static (PetBrain brain, FakeClock clock) Create(DateTime? start = null)
    {
        var clock = new FakeClock(start ?? Afternoon);
        return (new PetBrain(clock), clock);
    }

    [Fact]
    public void Default_is_idle()
    {
        var (brain, _) = Create();
        Assert.Equal(PetState.Idle, brain.Evaluate());
        Assert.Equal(PetState.Idle, brain.CurrentState);
    }

    [Fact]
    public void Music_playing_enters_music()
    {
        var (brain, _) = Create();
        brain.MusicChanged(true);
        Assert.Equal(PetState.Music, brain.CurrentState);
    }

    [Fact]
    public void Focus_beats_music()
    {
        var (brain, _) = Create();
        brain.MusicChanged(true);
        brain.FocusChanged(true);
        Assert.Equal(PetState.Focus, brain.Evaluate());
    }

    [Fact]
    public void Happy_beats_everything_and_ends_on_completion()
    {
        var (brain, clock) = Create();
        brain.FocusChanged(true);
        clock.Advance(TimeSpan.FromSeconds(3));
        Assert.True(brain.PetClicked());
        Assert.Equal(PetState.Happy, brain.CurrentState);

        brain.OneShotCompleted();
        Assert.Equal(PetState.Focus, brain.CurrentState);
    }

    [Fact]
    public void Sleep_when_idle_long_without_music()
    {
        var (brain, _) = Create();
        brain.TimeTick(TimeSpan.FromMinutes(6));
        Assert.Equal(PetState.Sleep, brain.CurrentState);
    }

    [Fact]
    public void Music_prevents_sleep()
    {
        var (brain, clock) = Create();
        brain.MusicChanged(true);
        clock.Advance(TimeSpan.FromSeconds(2));
        brain.TimeTick(TimeSpan.FromMinutes(30));
        Assert.Equal(PetState.Music, brain.CurrentState);
    }

    [Fact]
    public void Input_wakes_pet()
    {
        var (brain, clock) = Create();
        brain.TimeTick(TimeSpan.FromMinutes(6));
        clock.Advance(TimeSpan.FromSeconds(2));
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Idle, brain.CurrentState);
    }

    [Fact]
    public void Anti_flicker_holds_state_for_minimum_time()
    {
        var (brain, clock) = Create();
        brain.MusicChanged(true);
        Assert.Equal(PetState.Music, brain.CurrentState);

        clock.Advance(TimeSpan.FromSeconds(0.5));
        brain.MusicChanged(false);
        Assert.Equal(PetState.Music, brain.CurrentState); // ditahan

        clock.Advance(TimeSpan.FromSeconds(1.5));
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Idle, brain.CurrentState);
    }

    [Fact]
    public void Click_cooldown_is_enforced()
    {
        var (brain, clock) = Create();
        Assert.True(brain.PetClicked());
        clock.Advance(TimeSpan.FromSeconds(1));
        Assert.False(brain.PetClicked());
        clock.Advance(TimeSpan.FromSeconds(1.1));
        Assert.True(brain.PetClicked());
    }

    [Fact]
    public void Focus_completed_triggers_happy()
    {
        var (brain, _) = Create();
        brain.FocusChanged(false, completed: true);
        Assert.Equal(PetState.Happy, brain.CurrentState);
    }

    [Fact]
    public void Note_added_triggers_happy()
    {
        var (brain, _) = Create();
        brain.NoteAdded();
        Assert.Equal(PetState.Happy, brain.CurrentState);
    }

    [Fact]
    public void Happy_times_out_if_animation_never_completes()
    {
        var (brain, clock) = Create();
        brain.NoteAdded();
        clock.Advance(TimeSpan.FromSeconds(5));
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Idle, brain.CurrentState);
    }

    [Fact]
    public void Yawns_at_night_about_every_ten_minutes()
    {
        var (brain, clock) = Create(LateNight);
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Yawn, brain.CurrentState);

        brain.OneShotCompleted();
        Assert.Equal(PetState.Idle, brain.CurrentState);

        clock.Advance(TimeSpan.FromMinutes(5));
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Idle, brain.CurrentState);

        clock.Advance(TimeSpan.FromMinutes(5));
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Yawn, brain.CurrentState);
    }

    [Fact]
    public void No_yawn_during_the_day()
    {
        var (brain, _) = Create();
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Idle, brain.CurrentState);
    }

    [Fact]
    public void Music_beats_yawn()
    {
        var (brain, _) = Create(LateNight);
        brain.MusicChanged(true);
        brain.TimeTick(TimeSpan.Zero);
        Assert.Equal(PetState.Music, brain.CurrentState);
    }
}
