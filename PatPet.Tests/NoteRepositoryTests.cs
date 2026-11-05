using Microsoft.Data.Sqlite;
using PatPet.Services;

namespace PatPet.Tests;

public sealed class NoteRepositoryTests : IDisposable
{
    private readonly SqliteConnection _keepAlive;
    private readonly NoteRepository _repo;

    public NoteRepositoryTests()
    {
        // DB in-memory bersama; koneksi keep-alive menjaga isinya selama tes
        var cs = $"Data Source=notes-{Guid.NewGuid():N};Mode=Memory;Cache=Shared";
        _keepAlive = new SqliteConnection(cs);
        _keepAlive.Open();
        _repo = new NoteRepository(cs);
    }

    public void Dispose() => _keepAlive.Dispose();

    [Fact]
    public async Task Add_and_get()
    {
        var n = await _repo.AddAsync("Beli susu kucing");
        var all = await _repo.GetAllAsync();
        var single = Assert.Single(all);
        Assert.Equal(n.Id, single.Id);
        Assert.Equal("Beli susu kucing", single.Content);
        Assert.False(single.IsPinned);
    }

    [Fact]
    public async Task Update_changes_content()
    {
        var n = await _repo.AddAsync("draft");
        await _repo.UpdateAsync(n.Id, "final");
        Assert.Equal("final", (await _repo.GetAllAsync()).Single().Content);
    }

    [Fact]
    public async Task Delete_removes_note()
    {
        var n = await _repo.AddAsync("hapus aku");
        await _repo.DeleteAsync(n.Id);
        Assert.Empty(await _repo.GetAllAsync());
    }

    [Fact]
    public async Task Pinned_notes_come_first()
    {
        var a = await _repo.AddAsync("a");
        await Task.Delay(5);
        await _repo.AddAsync("b");
        await _repo.TogglePinAsync(a.Id);

        var all = await _repo.GetAllAsync();
        Assert.Equal("a", all[0].Content);
        Assert.True(all[0].IsPinned);

        await _repo.TogglePinAsync(a.Id);
        Assert.False((await _repo.GetAllAsync()).Single(x => x.Id == a.Id).IsPinned);
    }

    [Fact]
    public async Task Deadline_is_saved_and_sorted_earliest_first()
    {
        var later = await _repo.AddAsync("later", new DateTime(2026, 12, 1, 9, 0, 0));
        await _repo.AddAsync("none");
        var soon = await _repo.AddAsync("soon", new DateTime(2026, 11, 6, 23, 59, 0));

        var all = await _repo.GetAllAsync();
        Assert.Equal(new[] { "soon", "later", "none" }, all.Select(n => n.Content));
        Assert.Equal(new DateTime(2026, 11, 6, 23, 59, 0), all[0].Deadline);

        await _repo.SetDeadlineAsync(later.Id, null);
        Assert.Null((await _repo.GetAllAsync()).Single(n => n.Id == later.Id).Deadline);
        Assert.NotNull((await _repo.GetAllAsync()).Single(n => n.Id == soon.Id).Deadline);
    }

    [Fact]
    public async Task Done_notes_go_last_and_are_not_due()
    {
        var a = await _repo.AddAsync("a", new DateTime(2026, 1, 1, 8, 0, 0));
        await _repo.AddAsync("b", new DateTime(2026, 1, 2, 8, 0, 0));
        var now = new DateTime(2026, 11, 5, 12, 0, 0);

        Assert.Equal(2, (await _repo.GetDueAsync(now)).Count);

        await _repo.SetDoneAsync(a.Id, true);
        var all = await _repo.GetAllAsync();
        Assert.Equal("a", all[^1].Content);
        Assert.True(all[^1].IsDone);
        Assert.Single(await _repo.GetDueAsync(now));
    }

    [Fact]
    public async Task Search_wildcards_are_treated_literally()
    {
        await _repo.AddAsync("diskon 50% hari ini");
        await _repo.AddAsync("biasa saja");
        Assert.Single(await _repo.GetAllAsync("%"));
        Assert.Empty(await _repo.GetAllAsync("_"));
    }

    [Fact]
    public async Task Content_is_capped()
    {
        await _repo.AddAsync(new string('a', NoteRepository.MaxContentLength + 500));
        Assert.Equal(NoteRepository.MaxContentLength, (await _repo.GetAllAsync()).Single().Content.Length);
    }

    [Fact]
    public async Task Search_filters_by_content()
    {
        await _repo.AddAsync("Tugas kalkulus");
        await _repo.AddAsync("Belanja mingguan");
        await _repo.AddAsync("Revisi tugas akhir");

        var hits = await _repo.GetAllAsync("tugas");
        Assert.Equal(2, hits.Count);
        Assert.All(hits, h => Assert.Contains("tugas", h.Content, StringComparison.OrdinalIgnoreCase));
    }
}
