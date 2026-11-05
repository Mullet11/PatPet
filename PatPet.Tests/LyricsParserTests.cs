using System.Text.Json;
using PatPet.Services;

namespace PatPet.Tests;

/// <summary>Uji parser format LRC memakai teks contoh buatan sendiri.</summary>
public class LyricsParserTests
{
    [Fact]
    public void Parses_and_sorts_timed_lines()
    {
        var lines = LyricsService.ParseLrc("[00:12.50] baris kedua\n[00:03.00] baris pertama\n[01:02.345]baris ketiga\ntanpa waktu");
        Assert.Equal(3, lines.Count);
        Assert.Equal("baris pertama", lines[0].Text);
        Assert.Equal(TimeSpan.FromSeconds(3), lines[0].Time);
        Assert.Equal(TimeSpan.FromSeconds(12.5), lines[1].Time);
        Assert.Equal(TimeSpan.FromSeconds(62.345), lines[2].Time);
    }

    [Fact]
    public void Line_with_several_time_tags_repeats()
    {
        var lines = LyricsService.ParseLrc("[00:10.00][00:40.00] reff contoh");
        Assert.Equal(2, lines.Count);
        Assert.All(lines, l => Assert.Equal("reff contoh", l.Text));
    }

    [Fact]
    public void Index_follows_playback_position()
    {
        var lines = LyricsService.ParseLrc("[00:05.00] a\n[00:10.00] b\n[00:20.00] c");
        Assert.Equal(-1, LyricsService.IndexAt(lines, TimeSpan.FromSeconds(1)));
        Assert.Equal(0, LyricsService.IndexAt(lines, TimeSpan.FromSeconds(5)));
        Assert.Equal(1, LyricsService.IndexAt(lines, TimeSpan.FromSeconds(19.9)));
        Assert.Equal(2, LyricsService.IndexAt(lines, TimeSpan.FromMinutes(3)));
    }

    [Fact]
    public void Synced_lyrics_preferred_over_plain()
    {
        using var doc = JsonDocument.Parse("""{"syncedLyrics":"[00:01.00] satu","plainLyrics":"satu"}""");
        var r = LyricsService.Parse(doc.RootElement)!;
        Assert.True(r.Synced);
        Assert.Single(r.Lines);
    }

    [Fact]
    public void Plain_and_instrumental_are_handled()
    {
        using var plain = JsonDocument.Parse("""{"syncedLyrics":null,"plainLyrics":"satu\ndua"}""");
        var p = LyricsService.Parse(plain.RootElement)!;
        Assert.False(p.Synced);
        Assert.Equal(2, p.Lines.Count);

        using var ins = JsonDocument.Parse("""{"instrumental":true}""");
        Assert.True(LyricsService.Parse(ins.RootElement)!.Instrumental);

        using var empty = JsonDocument.Parse("""{"plainLyrics":""}""");
        Assert.Null(LyricsService.Parse(empty.RootElement));
    }

    [Fact]
    public void Very_long_lines_are_trimmed()
    {
        var lines = LyricsService.ParseLrc("[00:01.00] " + new string('x', 1000));
        Assert.Equal(200, lines[0].Text.Length);
    }
}
