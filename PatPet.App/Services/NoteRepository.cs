using System.Globalization;
using Microsoft.Data.Sqlite;
using PatPet.Models;

namespace PatPet.Services;

/// <summary>Catatan di SQLite. Koneksi dibuka per operasi; semua query berparameter.</summary>
public sealed class NoteRepository
{
    public const int MaxContentLength = 4000;
    private const string DeadlineFormat = "yyyy-MM-dd'T'HH:mm:ss"; // urut secara leksikal

    private readonly string _connectionString;

    public NoteRepository(string connectionString)
    {
        _connectionString = connectionString;
        EnsureSchema();
    }

    public static NoteRepository ForFile(string path) =>
        new(new SqliteConnectionStringBuilder { DataSource = path, Mode = SqliteOpenMode.ReadWriteCreate }.ToString());

    private SqliteConnection Open()
    {
        var c = new SqliteConnection(_connectionString);
        c.Open();
        return c;
    }

    private void EnsureSchema()
    {
        using var c = Open();
        using (var cmd = c.CreateCommand())
        {
            cmd.CommandText = """
                CREATE TABLE IF NOT EXISTS notes (
                  id         INTEGER PRIMARY KEY AUTOINCREMENT,
                  content    TEXT    NOT NULL,
                  is_pinned  INTEGER NOT NULL DEFAULT 0,
                  created_at TEXT    NOT NULL,
                  updated_at TEXT    NOT NULL
                );
                """;
            cmd.ExecuteNonQuery();
        }

        // migrasi v1.0 → v1.1: kolom deadline & is_done
        var columns = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        using (var info = c.CreateCommand())
        {
            info.CommandText = "PRAGMA table_info(notes)";
            using var r = info.ExecuteReader();
            while (r.Read()) columns.Add(r.GetString(1));
        }

        using var tx = c.BeginTransaction();
        using (var alter = c.CreateCommand())
        {
            alter.Transaction = tx;
            if (!columns.Contains("deadline"))
            {
                alter.CommandText = "ALTER TABLE notes ADD COLUMN deadline TEXT NULL";
                alter.ExecuteNonQuery();
            }
            if (!columns.Contains("is_done"))
            {
                alter.CommandText = "ALTER TABLE notes ADD COLUMN is_done INTEGER NOT NULL DEFAULT 0";
                alter.ExecuteNonQuery();
            }
            alter.CommandText = """
                DROP INDEX IF EXISTS idx_notes_pinned;
                CREATE INDEX IF NOT EXISTS idx_notes_order ON notes(is_done, is_pinned DESC, deadline, updated_at DESC);
                """;
            alter.ExecuteNonQuery();
        }
        tx.Commit();
    }

    public async Task<IReadOnlyList<Note>> GetAllAsync(string? search = null)
    {
        await using var c = Open();
        await using var cmd = c.CreateCommand();
        cmd.CommandText = """
            SELECT id, content, is_pinned, created_at, updated_at, deadline, is_done FROM notes
            WHERE @q = '' OR content LIKE '%' || @q || '%' ESCAPE '\'
            ORDER BY is_done ASC, is_pinned DESC,
                     CASE WHEN deadline IS NULL THEN 1 ELSE 0 END, deadline ASC,
                     updated_at DESC
            """;
        cmd.Parameters.AddWithValue("@q", EscapeLike(search?.Trim() ?? ""));
        return await ReadNotesAsync(cmd);
    }

    /// <summary>Catatan belum selesai yang deadline-nya sudah lewat atau tepat <paramref name="now"/>.</summary>
    public async Task<IReadOnlyList<Note>> GetDueAsync(DateTime now)
    {
        await using var c = Open();
        await using var cmd = c.CreateCommand();
        cmd.CommandText = """
            SELECT id, content, is_pinned, created_at, updated_at, deadline, is_done FROM notes
            WHERE is_done = 0 AND deadline IS NOT NULL AND deadline <= @now
            ORDER BY deadline ASC
            """;
        cmd.Parameters.AddWithValue("@now", FormatDeadline(now));
        return await ReadNotesAsync(cmd);
    }

    private static async Task<IReadOnlyList<Note>> ReadNotesAsync(SqliteCommand cmd)
    {
        var list = new List<Note>();
        await using var r = await cmd.ExecuteReaderAsync();
        while (await r.ReadAsync())
        {
            list.Add(new Note(
                r.GetInt64(0),
                r.GetString(1),
                r.GetInt64(2) != 0,
                ParseStamp(r.GetString(3)),
                ParseStamp(r.GetString(4)),
                r.IsDBNull(5) ? null : ParseDeadline(r.GetString(5)),
                r.GetInt64(6) != 0));
        }
        return list;
    }

    public async Task<Note> AddAsync(string content, DateTime? deadline = null)
    {
        content = Clip(content);
        var now = DateTime.Now;
        await using var c = Open();
        await using var cmd = c.CreateCommand();
        cmd.CommandText = """
            INSERT INTO notes (content, is_pinned, created_at, updated_at, deadline, is_done)
            VALUES (@c, 0, @t, @t, @d, 0);
            SELECT last_insert_rowid();
            """;
        cmd.Parameters.AddWithValue("@c", content);
        cmd.Parameters.AddWithValue("@t", FormatStamp(now));
        cmd.Parameters.AddWithValue("@d", deadline is { } d ? FormatDeadline(d) : DBNull.Value);
        var id = (long)(await cmd.ExecuteScalarAsync() ?? 0L);
        return new Note(id, content, false, now, now, deadline);
    }

    public Task UpdateAsync(long id, string content) =>
        ExecAsync("UPDATE notes SET content = @v, updated_at = @t WHERE id = @id", id, Clip(content), touch: true);

    public Task SetDeadlineAsync(long id, DateTime? deadline) =>
        ExecAsync("UPDATE notes SET deadline = @v, updated_at = @t WHERE id = @id", id,
            deadline is { } d ? FormatDeadline(d) : DBNull.Value, touch: true);

    public Task SetDoneAsync(long id, bool done) =>
        ExecAsync("UPDATE notes SET is_done = @v WHERE id = @id", id, done ? 1 : 0);

    public Task DeleteAsync(long id) => ExecAsync("DELETE FROM notes WHERE id = @id", id);

    public Task TogglePinAsync(long id) => ExecAsync("UPDATE notes SET is_pinned = 1 - is_pinned WHERE id = @id", id);

    private async Task ExecAsync(string sql, long id, object? value = null, bool touch = false)
    {
        await using var c = Open();
        await using var cmd = c.CreateCommand();
        cmd.CommandText = sql;
        cmd.Parameters.AddWithValue("@id", id);
        if (value is not null) cmd.Parameters.AddWithValue("@v", value);
        if (touch) cmd.Parameters.AddWithValue("@t", FormatStamp(DateTime.Now));
        await cmd.ExecuteNonQueryAsync();
    }

    /// <summary>Backup harian notes.db, menyimpan <paramref name="keep"/> file terakhir.</summary>
    public void BackupDaily(string backupDir, int keep = 7)
    {
        Directory.CreateDirectory(backupDir);
        var target = Path.Combine(backupDir, $"notes-{DateTime.Now:yyyyMMdd}.db");
        if (!File.Exists(target))
        {
            using var src = Open();
            using var dst = new SqliteConnection(new SqliteConnectionStringBuilder { DataSource = target, Pooling = false }.ToString());
            dst.Open();
            src.BackupDatabase(dst);
        }

        foreach (var old in Directory.GetFiles(backupDir, "notes-*.db").OrderByDescending(f => f).Skip(keep))
        {
            try { File.Delete(old); } catch { /* abaikan */ }
        }
    }

    internal static string EscapeLike(string s) =>
        s.Replace("\\", "\\\\").Replace("%", "\\%").Replace("_", "\\_");

    private static string Clip(string s) => s.Length > MaxContentLength ? s[..MaxContentLength] : s;

    private static string FormatStamp(DateTime t) => t.ToString("o", CultureInfo.InvariantCulture);
    private static DateTime ParseStamp(string s) => DateTime.Parse(s, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind);
    private static string FormatDeadline(DateTime t) => t.ToString(DeadlineFormat, CultureInfo.InvariantCulture);
    private static DateTime? ParseDeadline(string s) =>
        DateTime.TryParseExact(s, DeadlineFormat, CultureInfo.InvariantCulture, DateTimeStyles.None, out var d) ? d : null;
}
