using System.Media;

namespace PatPet.Services;

/// <summary>
/// Alarm timer: pola "beep-beep-beep" yang dibuat di memori (tanpa file audio) dan diputar berulang
/// sampai dimatikan atau batas waktu tercapai. Buffer suara dibuang saat alarm berhenti.
/// </summary>
public sealed class AlarmService : IDisposable
{
    private const int SampleRate = 22050;

    private SoundPlayer? _player;
    private MemoryStream? _wav;
    private CancellationTokenSource? _autoStop;

    public bool IsRinging { get; private set; }

    public event Action<bool>? RingingChanged;

    public void Start(TimeSpan maxDuration)
    {
        Stop();
        try
        {
            _wav = BuildWav();
            _player = new SoundPlayer(_wav);
            _player.Load();
            _player.PlayLooping();
        }
        catch
        {
            SystemSounds.Exclamation.Play(); // perangkat audio bermasalah: setidaknya bunyi sistem
        }

        IsRinging = true;
        RingingChanged?.Invoke(true);

        var cts = _autoStop = new CancellationTokenSource();
        _ = AutoStopAsync(maxDuration, cts.Token);
    }

    private async Task AutoStopAsync(TimeSpan after, CancellationToken ct)
    {
        try
        {
            await Task.Delay(after, ct);
            Stop();
        }
        catch (OperationCanceledException) { }
    }

    public void Stop()
    {
        _autoStop?.Cancel();
        _autoStop?.Dispose();
        _autoStop = null;

        if (_player is not null)
        {
            try { _player.Stop(); } catch { }
            _player.Dispose();
            _player = null;
        }
        _wav?.Dispose();
        _wav = null;

        if (!IsRinging) return;
        IsRinging = false;
        RingingChanged?.Invoke(false);
    }

    /// <summary>WAV PCM 16-bit mono: 3 nada 880 Hz (140 ms, jeda 90 ms) lalu hening 650 ms ≈ 1,3 detik per pola.</summary>
    internal static MemoryStream BuildWav()
    {
        var samples = new List<short>(SampleRate * 2);
        for (var i = 0; i < 3; i++)
        {
            AddTone(samples, 880, 140);
            AddSilence(samples, 90);
        }
        AddSilence(samples, 650);

        var dataBytes = samples.Count * 2;
        var ms = new MemoryStream(44 + dataBytes);
        using (var w = new BinaryWriter(ms, System.Text.Encoding.ASCII, leaveOpen: true))
        {
            w.Write("RIFF"u8.ToArray());
            w.Write(36 + dataBytes);
            w.Write("WAVE"u8.ToArray());
            w.Write("fmt "u8.ToArray());
            w.Write(16);                 // ukuran chunk fmt
            w.Write((short)1);           // PCM
            w.Write((short)1);           // mono
            w.Write(SampleRate);
            w.Write(SampleRate * 2);     // byte rate
            w.Write((short)2);           // block align
            w.Write((short)16);          // bit per sample
            w.Write("data"u8.ToArray());
            w.Write(dataBytes);
            foreach (var s in samples) w.Write(s);
        }
        ms.Position = 0;
        return ms;
    }

    private static void AddTone(List<short> buf, double freq, int ms)
    {
        var n = SampleRate * ms / 1000;
        var fade = SampleRate * 6 / 1000; // fade 6 ms agar tidak "klik"
        for (var i = 0; i < n; i++)
        {
            var env = Math.Min(1.0, Math.Min(i, n - 1 - i) / (double)fade);
            // nada dasar + sedikit harmonik kedua agar lebih "alarm"
            var v = Math.Sin(2 * Math.PI * freq * i / SampleRate) * 0.8 + Math.Sin(4 * Math.PI * freq * i / SampleRate) * 0.2;
            buf.Add((short)(v * env * 0.45 * short.MaxValue));
        }
    }

    private static void AddSilence(List<short> buf, int ms) => buf.AddRange(new short[SampleRate * ms / 1000]);

    public void Dispose() => Stop();
}
