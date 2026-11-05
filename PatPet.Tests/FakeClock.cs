using PatPet.Services;

namespace PatPet.Tests;

public sealed class FakeClock : IClock
{
    public FakeClock(DateTime start) => Now = start;

    public DateTime Now { get; set; }

    public void Advance(TimeSpan by) => Now += by;
}
