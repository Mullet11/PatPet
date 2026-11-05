using PatPet.Models;
using PatPet.Services;
using PatPet.Views.Controls;

namespace PatPet.Tests;

public class DeadlineTests
{
    private static readonly DateTime Now = new(2026, 11, 5, 14, 0, 0); // Kamis

    [Fact]
    public void Format_relative_days()
    {
        Assert.Equal("Hari ini 23:59", DeadlineFormat.Format(Now.Date.AddHours(23).AddMinutes(59), Now));
        Assert.Equal("Besok 09:00", DeadlineFormat.Format(Now.Date.AddDays(1).AddHours(9), Now));
        Assert.Equal("Kemarin 10:00", DeadlineFormat.Format(Now.Date.AddDays(-1).AddHours(10), Now));
        Assert.Equal("Sab, 7 Nov 08:00", DeadlineFormat.Format(new DateTime(2026, 11, 7, 8, 0, 0), Now));
    }

    [Fact]
    public void State_of_deadline()
    {
        Assert.Equal(DeadlineState.None, DeadlineFormat.StateOf(null, false, Now));
        Assert.Equal(DeadlineState.Overdue, DeadlineFormat.StateOf(Now.AddMinutes(-1), false, Now));
        Assert.Equal(DeadlineState.Done, DeadlineFormat.StateOf(Now.AddMinutes(-1), true, Now));
        Assert.Equal(DeadlineState.Today, DeadlineFormat.StateOf(Now.AddHours(2), false, Now));
        Assert.Equal(DeadlineState.Soon, DeadlineFormat.StateOf(Now.AddDays(1), false, Now));
        Assert.Equal(DeadlineState.Later, DeadlineFormat.StateOf(Now.AddDays(5), false, Now));
    }

    [Theory]
    [InlineData("21:30", true)]
    [InlineData("9:05", true)]
    [InlineData("09.05", true)]
    [InlineData("24:00", false)]
    [InlineData("abc", false)]
    [InlineData("", false)]
    public void Time_input_is_validated(string input, bool ok) =>
        Assert.Equal(ok, DeadlinePicker.TryParseTime(input, out _));

    [Fact]
    public void Settings_are_sanitized()
    {
        var s = new AppSettings
        {
            PetScale = 99,
            IdleSleepMinutes = -4,
            Timer = new TimerSettings { DurationSeconds = -5 },
            Mode = (DisplayMode)42,
            Location = new LocationSettings { Latitude = 500, Longitude = 10, Name = new string('x', 200) },
        };
        SettingsService.Sanitize(s);
        Assert.Equal(3, s.PetScale);
        Assert.Equal(1, s.IdleSleepMinutes);
        Assert.Equal(1, s.Timer.DurationSeconds);
        Assert.Equal(DisplayMode.Bar, s.Mode);
        Assert.Null(s.Location.Latitude);
        Assert.Equal(80, s.Location.Name!.Length);
    }
}
