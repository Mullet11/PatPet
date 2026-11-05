using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using PatPet.Models;
using PatPet.Services;

namespace PatPet.ViewModels;

public partial class PetViewModel : ObservableObject
{
    private readonly PetBrain _brain;

    /// <summary>Animasi lengkap (halaman Waktu): idle, music, focus, happy, yawn, sleep.</summary>
    [ObservableProperty] private string _animation = "idle";
    [ObservableProperty] private bool _playOnce;
    [ObservableProperty] private int _playToken;
    [ObservableProperty] private string _moodText = "";

    /// <summary>Kucing yang menempel di atas layar hanya diam atau tiduran.</summary>
    [ObservableProperty] private string _restingAnimation = "idle";

    public PetViewModel(PetBrain brain)
    {
        _brain = brain;
        _brain.StateChanged += Apply;
        Apply(_brain.CurrentState);
    }

    private void Apply(PetState state)
    {
        PlayOnce = state is PetState.Happy or PetState.Yawn;
        Animation = state.ToString().ToLowerInvariant();
        PlayToken++;
        MoodText = state switch
        {
            PetState.Music => "♪ ikut ngangguk",
            PetState.Sleep => "Zzz…",
            PetState.Focus => "Fokus bareng!",
            PetState.Happy => "Senang! ♥",
            PetState.Yawn => "Hoaam…",
            _ => "Santai~",
        };
        // kucing di bar / ikon: ikut berubah untuk keadaan yang berlangsung lama
        // (tidur saat idle, headphone saat musik, membaca saat timer); reaksi sesaat (senang, menguap) tidak.
        RestingAnimation = state switch
        {
            PetState.Sleep => "sleep",
            PetState.Music => "music",
            PetState.Focus => "focus",
            _ => "idle",
        };
    }

    [RelayCommand]
    private void Click() => _brain.PetClicked();

    [RelayCommand]
    private void AnimationCompleted() => _brain.OneShotCompleted();
}
