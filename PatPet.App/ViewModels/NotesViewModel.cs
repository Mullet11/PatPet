using System.Collections.ObjectModel;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Services;

namespace PatPet.ViewModels;

public partial class NotesViewModel : ObservableObject
{
    private readonly NoteRepository _repo;
    private readonly INotificationService _notify;
    private readonly DispatcherTimer _searchDebounce;
    private readonly DispatcherTimer _undoTimer;
    private readonly HashSet<(long Id, DateTime Deadline)> _notified = new();
    private bool _firstDeadlineCheck = true;

    [ObservableProperty] private string _searchText = "";
    [ObservableProperty] private string _newNoteText = "";
    [ObservableProperty] private DateTime? _newDeadline;
    [ObservableProperty] private bool _isEmpty = true;
    [ObservableProperty] private NoteItemViewModel? _pendingDelete;
    [ObservableProperty] private int _overdueCount;

    public NotesViewModel(NoteRepository repo, INotificationService notify)
    {
        _repo = repo;
        _notify = notify;
        _searchDebounce = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(250) };
        _searchDebounce.Tick += async (_, _) => { _searchDebounce.Stop(); await LoadAsync(); };
        _undoTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
        _undoTimer.Tick += async (_, _) => await CommitPendingDeleteAsync();
    }

    public ObservableCollection<NoteItemViewModel> Notes { get; } = new();

    public event Action? NoteAdded;

    partial void OnSearchTextChanged(string value)
    {
        _searchDebounce.Stop();
        _searchDebounce.Start();
    }

    public async Task LoadAsync()
    {
        foreach (var n in Notes) await n.FlushAsync();
        var list = await _repo.GetAllAsync(SearchText);
        Notes.Clear();
        var now = DateTime.Now;
        foreach (var note in list)
        {
            if (PendingDelete?.Id == note.Id) continue;
            Notes.Add(new NoteItemViewModel(_repo, note, now));
        }
        IsEmpty = Notes.Count == 0;
        OverdueCount = Notes.Count(n => n.State == DeadlineState.Overdue);
    }

    [RelayCommand]
    private async Task Add()
    {
        var text = NewNoteText.Trim();
        if (text.Length == 0) return;
        await _repo.AddAsync(text, NewDeadline);
        NewNoteText = "";
        NewDeadline = null;
        SearchText = "";
        _searchDebounce.Stop();
        await LoadAsync();
        NoteAdded?.Invoke();
    }

    [RelayCommand]
    private async Task TogglePin(NoteItemViewModel? item)
    {
        if (item is null) return;
        await item.FlushAsync();
        await _repo.TogglePinAsync(item.Id);
        await LoadAsync();
    }

    [RelayCommand]
    private async Task ToggleDone(NoteItemViewModel? item)
    {
        if (item is null) return;
        await item.FlushAsync();
        await _repo.SetDoneAsync(item.Id, !item.IsDone);
        await LoadAsync();
    }

    [RelayCommand]
    private async Task Delete(NoteItemViewModel? item)
    {
        if (item is null) return;
        await CommitPendingDeleteAsync();
        await item.FlushAsync();
        PendingDelete = item;
        Notes.Remove(item);
        IsEmpty = Notes.Count == 0;
        _undoTimer.Start();
    }

    [RelayCommand]
    private async Task Undo()
    {
        if (PendingDelete is null) return;
        _undoTimer.Stop();
        PendingDelete = null;
        await LoadAsync();
    }

    private async Task CommitPendingDeleteAsync()
    {
        _undoTimer.Stop();
        if (PendingDelete is { } p)
        {
            PendingDelete = null;
            await _repo.DeleteAsync(p.Id);
        }
    }

    /// <summary>Dipanggil tiap menit: perbarui label deadline dan kirim notifikasi saat jatuh tempo.</summary>
    public async Task CheckDeadlinesAsync(DateTime now)
    {
        foreach (var n in Notes) n.RefreshDeadline(now);
        OverdueCount = Notes.Count(n => n.State == DeadlineState.Overdue);

        IReadOnlyList<Models.Note> due;
        try { due = await _repo.GetDueAsync(now); }
        catch { return; }

        var fresh = due.Where(n => _notified.Add((n.Id, n.Deadline!.Value))).ToList();
        if (fresh.Count == 0) return;

        if (_firstDeadlineCheck && fresh.Count > 1)
        {
            _notify.Notify("Deadline terlewat", $"{fresh.Count} catatan sudah melewati deadline.");
        }
        else
        {
            foreach (var n in fresh.Take(3))
                _notify.Notify("Deadline: " + Shorten(n.Content), DeadlineFormat.Format(n.Deadline!.Value, now));
        }
        _firstDeadlineCheck = false;
    }

    private static string Shorten(string s)
    {
        s = s.ReplaceLineEndings(" ");
        return s.Length > 50 ? s[..50] + "…" : s;
    }

    public async Task FlushAsync()
    {
        foreach (var n in Notes) await n.FlushAsync();
        await CommitPendingDeleteAsync();
    }
}
