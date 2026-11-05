using PatPet.Services;
using PatPet.ViewModels;

namespace PatPet.Tests;

/// <summary>Kucing di bar/ikon ikut berubah untuk keadaan yang berlangsung lama.</summary>
public class PetViewModelTests
{
    private static (PetViewModel vm, PetBrain brain, FakeClock clock) Create()
    {
        var clock = new FakeClock(new DateTime(2026, 11, 5, 14, 0, 0));
        var brain = new PetBrain(clock);
        return (new PetViewModel(brain), brain, clock);
    }

    [Fact]
    public void Idle_by_default()
    {
        var (vm, _, _) = Create();
        Assert.Equal("idle", vm.RestingAnimation);
    }

    [Fact]
    public void Sleeps_after_five_minutes_idle()
    {
        var (vm, brain, _) = Create();
        brain.TimeTick(TimeSpan.FromMinutes(5));
        Assert.Equal("sleep", vm.RestingAnimation);
    }

    [Fact]
    public void Wears_headphones_while_music_plays_and_returns_to_idle()
    {
        var (vm, brain, clock) = Create();
        brain.MusicChanged(true);
        Assert.Equal("music", vm.RestingAnimation);

        clock.Advance(TimeSpan.FromSeconds(2));
        brain.MusicChanged(false);
        Assert.Equal("idle", vm.RestingAnimation);
    }

    [Fact]
    public void Reads_while_timer_runs()
    {
        var (vm, brain, _) = Create();
        brain.FocusChanged(true);
        Assert.Equal("focus", vm.RestingAnimation);
    }

    [Fact]
    public void Short_reactions_do_not_change_the_resting_cat()
    {
        var (vm, brain, _) = Create();
        brain.NoteAdded(); // senang sesaat
        Assert.Equal("happy", vm.Animation);
        Assert.Equal("idle", vm.RestingAnimation);
    }
}
