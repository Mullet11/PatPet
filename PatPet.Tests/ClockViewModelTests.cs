using PatPet.Services;
using PatPet.ViewModels;

namespace PatPet.Tests;

public class ClockViewModelTests
{
    [Theory]
    [InlineData(4, "Selamat pagi")]
    [InlineData(9, "Selamat pagi")]
    [InlineData(10, "Selamat siang")]
    [InlineData(15, "Selamat sore")]
    [InlineData(18, "Selamat malam")]
    [InlineData(2, "Selamat malam")]
    public void Greeting_by_hour(int hour, string expected) =>
        Assert.Equal(expected, ClockViewModel.GetGreeting(hour));

    [Theory]
    [InlineData(7, "WIB")]
    [InlineData(8, "WITA")]
    [InlineData(9, "WIT")]
    [InlineData(0, "UTC+0")]
    [InlineData(-5, "UTC-5")]
    public void Zone_label(int hours, string expected) =>
        Assert.Equal(expected, ClockViewModel.GetZoneLabel(TimeSpan.FromHours(hours)));

    [Fact]
    public void Time_has_no_seconds_and_date_is_indonesian()
    {
        using var weather = new WeatherService();
        var settings = new SettingsService(Path.Combine(Path.GetTempPath(), $"patpet-{Guid.NewGuid():N}.json"));
        var vm = new ClockViewModel(weather, settings);
        vm.Update(new DateTime(2026, 11, 5, 8, 30, 15));
        Assert.Equal("08:30", vm.TimeText);
        Assert.Equal("Kamis, 5 November 2026", vm.DateText);
    }

    [Theory]
    [InlineData(0, true, "Cerah", "☀")]
    [InlineData(0, false, "Cerah", "☾")]
    [InlineData(63, true, "Hujan sedang", "☂")]
    [InlineData(95, true, "Badai petir", "⛈")]
    public void Weather_codes_are_described(int code, bool day, string text, string glyph)
    {
        var (t, g) = WeatherService.Describe(code, day);
        Assert.Equal(text, t);
        Assert.Equal(glyph, g);
    }

    [Theory]
    [InlineData(-3.32, 114.59, true)]
    [InlineData(91, 0, false)]
    [InlineData(0, 181, false)]
    [InlineData(double.NaN, 0, false)]
    public void Coordinates_are_validated(double lat, double lon, bool ok) =>
        Assert.Equal(ok, WeatherService.IsValid(lat, lon));
}
