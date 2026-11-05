namespace PatPet.Models;

public sealed record Note(
    long Id,
    string Content,
    bool IsPinned,
    DateTime CreatedAt,
    DateTime UpdatedAt,
    DateTime? Deadline = null,
    bool IsDone = false);
