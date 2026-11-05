using PatPet.Interop;

namespace PatPet.Services;

/// <summary>Baterai (GetSystemPowerStatus) dan suhu (thermal zone ACPI) tanpa hak admin.</summary>
public sealed class SystemStatusService : IDisposable
{
    private Win32.ThermalQuery? _thermal;
    private bool _thermalTried;
    private int _thermalBusy;
    private double? _lastTemperature;

    internal Win32.PowerStatus ReadPower() => Win32.GetPowerStatus();

    /// <summary>Dibaca di thread pool karena PDH bisa lambat sesaat.</summary>
    public async Task<double?> ReadTemperatureAsync()
    {
        if (Interlocked.Exchange(ref _thermalBusy, 1) == 1) return _lastTemperature; // pembacaan sebelumnya belum selesai
        try
        {
            _lastTemperature = await Task.Run(() =>
            {
                if (!_thermalTried)
                {
                    _thermalTried = true;
                    _thermal = Win32.ThermalQuery.TryOpen();
                }
                return _thermal?.ReadCelsius();
            });
            return _lastTemperature;
        }
        catch
        {
            return null;
        }
        finally
        {
            Interlocked.Exchange(ref _thermalBusy, 0);
        }
    }

    public void Dispose()
    {
        _thermal?.Dispose();
        _thermal = null;
    }
}
