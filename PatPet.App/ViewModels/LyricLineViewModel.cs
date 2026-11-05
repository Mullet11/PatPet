using CommunityToolkit.Mvvm.ComponentModel;

namespace PatPet.ViewModels;

public partial class LyricLineViewModel : ObservableObject
{
    public LyricLineViewModel(string text) => Text = text.Length == 0 ? "♪" : text;

    public string Text { get; }

    [ObservableProperty] private bool _isCurrent;
}
