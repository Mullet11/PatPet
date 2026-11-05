using PatPet.Interop;

namespace PatPet.Services;

/// <summary>Deteksi idle pengguna lewat GetLastInputInfo.</summary>
public sealed class ActivityService
{
    public TimeSpan GetIdleTime() => Win32.GetIdleTime();
}
