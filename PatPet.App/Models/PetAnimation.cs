namespace PatPet.Models;

public enum PetState { Idle, Music, Sleep, Focus, Happy, Yawn }

public sealed class PetManifest
{
    public int FrameWidth { get; set; } = 32;
    public int FrameHeight { get; set; } = 32;
    public Dictionary<string, PetAnimation> Animations { get; set; } = new();
}

public sealed class PetAnimation
{
    public string File { get; set; } = "";
    public int Frames { get; set; } = 1;
    public int Fps { get; set; } = 6;
    public bool Loop { get; set; } = true;
    public string? Description { get; set; }
}
