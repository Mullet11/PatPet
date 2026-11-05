using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Services;

namespace PatPet.ViewModels;

public partial class SettingsViewModel : ObservableObject
{
    private readonly SettingsService _settings;
    private readonly PetBrain _brain;
    private readonly WeatherService _weather;
    private readonly ClockViewModel _clock;
    private readonly bool _loading;
    private CancellationTokenSource? _searchCts;

    [ObservableProperty] private int _idleSleepMinutes;
    [ObservableProperty] private int _petScale;
    [ObservableProperty] private bool _autoCollapse;
    [ObservableProperty] private bool _useDeviceLocation;
    [ObservableProperty] private string _cityQuery = "";
    [ObservableProperty] private string _locationStatus = "";
    [ObservableProperty] private bool _isSearching;

    public SettingsViewModel(SettingsService settings, PetBrain brain, WeatherService weather, ClockViewModel clock)
    {
        _settings = settings;
        _brain = brain;
        _weather = weather;
        _clock = clock;

        _loading = true;
        var s = settings.Current;
        IdleSleepMinutes = s.IdleSleepMinutes;
        PetScale = s.PetScale;
        AutoCollapse = s.AutoCollapse;
        UseDeviceLocation = s.Location.UseDeviceLocation;
        _loading = false;
        Commit();
        UpdateLocationStatus();
    }

    public ObservableCollection<GeoPlace> SearchResults { get; } = new();

    partial void OnIdleSleepMinutesChanged(int value) => Commit();
    partial void OnPetScaleChanged(int value) => Commit();
    partial void OnAutoCollapseChanged(bool value) => Commit();

    partial void OnUseDeviceLocationChanged(bool value)
    {
        if (_loading) return;
        _settings.Current.Location.UseDeviceLocation = value;
        _settings.Save();
        UpdateLocationStatus();
        _ = _clock.RefreshAsync(forceLocation: true);
    }

    [RelayCommand]
    private void ChangeIdle(string? delta) =>
        IdleSleepMinutes = Math.Clamp(IdleSleepMinutes + (int.TryParse(delta, out var d) ? d : 0), 1, 120);

    [RelayCommand]
    private void SetPetScale(string? scale) =>
        PetScale = Math.Clamp(int.TryParse(scale, out var v) ? v : 2, 1, 3);

    private void Commit()
    {
        if (_loading) return;
        var s = _settings.Current;
        s.IdleSleepMinutes = Math.Clamp(IdleSleepMinutes, 1, 120);
        s.PetScale = Math.Clamp(PetScale, 1, 3);
        s.AutoCollapse = AutoCollapse;
        _brain.IdleSleepThreshold = TimeSpan.FromMinutes(s.IdleSleepMinutes);
        _settings.Save();
    }

    [RelayCommand]
    private async Task SearchCity()
    {
        _searchCts?.Cancel();
        _searchCts = new CancellationTokenSource(TimeSpan.FromSeconds(15));
        var ct = _searchCts.Token;

        SearchResults.Clear();
        if (CityQuery.Trim().Length < 2)
        {
            LocationStatus = "Ketik paling sedikit 2 huruf.";
            return;
        }

        IsSearching = true;
        LocationStatus = "Mencari…";
        try
        {
            var results = await _weather.SearchCityAsync(CityQuery, ct);
            if (ct.IsCancellationRequested) return;
            foreach (var r in results) SearchResults.Add(r);
            LocationStatus = results.Count == 0 ? "Kota tidak ditemukan atau internet terputus." : "Pilih kotanya:";
        }
        finally
        {
            IsSearching = false;
        }
    }

    [RelayCommand]
    private async Task SelectPlace(GeoPlace? place)
    {
        if (place is null) return;
        var loc = _settings.Current.Location;
        loc.Name = place.Name;
        loc.Latitude = place.Latitude;
        loc.Longitude = place.Longitude;
        loc.ResolvedAt = DateTime.Now;
        loc.UseDeviceLocation = false;
        _settings.Save();

        SearchResults.Clear();
        CityQuery = "";
        if (UseDeviceLocation) UseDeviceLocation = false; // memicu refresh lewat handler
        else await _clock.RefreshAsync(forceLocation: false);
        UpdateLocationStatus();
    }

    private void UpdateLocationStatus()
    {
        var loc = _settings.Current.Location;
        LocationStatus = UseDeviceLocation
            ? "Lokasi diambil dari Windows (Pengaturan, Privasi, Lokasi)."
            : $"Kota: {loc.Name ?? "belum dipilih"}";
    }
}
