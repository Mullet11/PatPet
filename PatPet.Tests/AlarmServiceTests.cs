using System.Text;
using PatPet.Services;

namespace PatPet.Tests;

public class AlarmServiceTests
{
    [Fact]
    public void Generated_wav_has_valid_pcm_header_and_length()
    {
        using var wav = AlarmService.BuildWav();
        var bytes = wav.ToArray();

        Assert.Equal("RIFF", Encoding.ASCII.GetString(bytes, 0, 4));
        Assert.Equal("WAVE", Encoding.ASCII.GetString(bytes, 8, 4));
        Assert.Equal(1, BitConverter.ToInt16(bytes, 20));       // PCM
        Assert.Equal(1, BitConverter.ToInt16(bytes, 22));       // mono
        Assert.Equal(22050, BitConverter.ToInt32(bytes, 24));   // sample rate
        Assert.Equal(16, BitConverter.ToInt16(bytes, 34));      // 16-bit

        var dataLen = BitConverter.ToInt32(bytes, 40);
        Assert.Equal(bytes.Length - 44, dataLen);
        var seconds = dataLen / 2.0 / 22050;
        Assert.InRange(seconds, 1.2, 1.5); // 3 nada + jeda ≈ 1,34 dtk per pola
        Assert.True(bytes.Length < 100 * 1024); // buffer kecil
    }
}
