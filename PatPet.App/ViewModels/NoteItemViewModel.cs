using System.Globalization;
using System.Windows.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using PatPet.Models;
using PatPet.Services;

namespace PatPet.ViewModels;

public partial class NoteItemViewModel : ObservableObject
{
    private static readonly CultureInfo Indonesian = new("id-ID");

    private readonly NoteRepository _repo;
    private readonly DispatcherTimer _saveDebounce;
    private readonly bool _loading;
    private bool _dirty;

    [ObservableProperty] private string _content = "";
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(PinGlyph))]
    private bool _isPinned;
    [ObservableProperty] private bool _isDone;
    [ObservableProperty] private DateTime? _deadline;
    [ObservableProperty] private string _deadlineText = "";
    [ObservableProperty] private DeadlineState _state;
    [ObservableProperty] private DateTime _updatedAt;

    public NoteItemViewModel(NoteRepository repo, Note note, DateTime now)
    {
        _repo = repo;
        Id = note.Id;
        _saveDebounce = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(600) };
        _saveDebounce.Tick += async (_, _) => await FlushAsync();

        _loading = true;
        Content = note.Content;
        IsPinned = note.IsPinned;
        IsDone = note.IsDone;
        Deadline = note.Deadline;
        UpdatedAt = note.UpdatedAt;
        _loading = false;
        RefreshDeadline(now);
    }

    public long Id { get; }

    public string PinGlyph => IsPinned ? "" : "";

    public string UpdatedText => UpdatedAt.Date == DateTime.Today
        ? "diubah " + UpdatedAt.ToString("HH:mm", Indonesian)
        : "diubah " + UpdatedAt.ToString("d MMM", Indonesian);

    partial void OnUpdatedAtChanged(DateTime value) => OnPropertyChanged(nameof(UpdatedText));

    partial void OnContentChanged(string value)
    {
        if (_loading) return;
        _dirty = true;
        _saveDebounce.Stop();
        _saveDebounce.Start(); // autosave 600 ms setelah berhenti mengetik
    }

    partial void OnDeadlineChanged(DateTime? value)
    {
        if (_loading) return;
        RefreshDeadline(DateTime.Now);
        _ = SaveDeadlineAsync(value);
    }

    private async Task SaveDeadlineAsync(DateTime? value)
    {
        try { await _repo.SetDeadlineAsync(Id, value); }
        catch { /* dicoba lagi saat diubah berikutnya */ }
    }

    public void RefreshDeadline(DateTime now)
    {
        State = DeadlineFormat.StateOf(Deadline, IsDone, now);
        DeadlineText = Deadline is { } d
            ? (State == DeadlineState.Overdue ? "Lewat · " : "") + DeadlineFormat.Format(d, now)
            : "";
    }

    public async Task FlushAsync()
    {
        _saveDebounce.Stop();
        if (!_dirty) return;
        _dirty = false;
        await _repo.UpdateAsync(Id, Content);
        UpdatedAt = DateTime.Now;
    }
}
