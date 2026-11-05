using CommunityToolkit.Mvvm.ComponentModel;
using PatPet.Services;

namespace PatPet.ViewModels;

public enum BatteryLevel { Normal, Low, Charging }

/// <summary>Baterai & suhu untuk bar atas. Baterai dibaca tiap detik (murah), suhu tiap 5 detik.</summary>
public partial class SystemStatusViewModel : ObservableObject
{
    private readonly SystemStatusService _svc;
    private bool? _lastPlugged;
    private DateTime _nextTemp = DateTime.MinValue;

    [ObservableProperty] private bool _hasBattery;
    [ObservableProperty] private int _batteryPercent;
    [ObservableProperty] private bool _isPlugged;
    [ObservableProperty] private bool _isCharging;
    [ObservableProperty] private BatteryLevel _level;
    [ObservableProperty] private string _batteryText = "";
    [ObservableProperty] private bool _hasTemperature;
    [ObservableProperty] private string _temperatureText = "";
    [ObservableProperty] private bool _isHot;

    public SystemStatusViewModel(SystemStatusService svc) => _svc = svc;

    /// <summary>Charger dicolok (true) atau dicabut (false); untuk animasi di bar.</summary>
    public event Action<bool>? PowerSourceChanged;

    public void Update(DateTime now)
    {
        var p = _svc.ReadPower();
        HasBattery = p.HasBattery;
        BatteryPercent = p.Percent;
        IsPlugged = p.Plugged;
        IsCharging = p.Charging || (p.Plugged && p.Percent < 100);
        Level = IsPlugged ? BatteryLevel.Charging : p.Percent <= 20 ? BatteryLevel.Low : BatteryLevel.Normal;
        BatteryText = p.HasBattery ? $"{p.Percent}%" : "AC";

        if (_lastPlugged is { } last && last != p.Plugged && p.HasBattery) PowerSourceChanged?.Invoke(p.Plugged);
        _lastPlugged = p.Plugged;

        if (now >= _nextTemp)
        {
            _nextTemp = now.AddSeconds(5);
            _ = UpdateTemperatureAsync();
        }
    }

    private async Task UpdateTemperatureAsync()
    {
        var c = await _svc.ReadTemperatureAsync();
        HasTemperature = c is not null;
        if (c is { } v)
        {
            TemperatureText = $"{Math.Round(v):0}°C";
            IsHot = v >= 80;
        }
    }
}
