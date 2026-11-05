using System.Globalization;

namespace PatPet.Services;

public enum DeadlineState { None, Done, Overdue, Today, Soon, Later }

public static class DeadlineFormat
{
    private static readonly CultureInfo Indonesian = new("id-ID");

    public static string Format(DateTime deadline, DateTime now)
    {
        var time = deadline.ToString("HH:mm", CultureInfo.InvariantCulture);
        var days = (deadline.Date - now.Date).Days;
        return days switch
        {
            0 => $"Hari ini {time}",
            1 => $"Besok {time}",
            -1 => $"Kemarin {time}",
            _ when deadline.Year == now.Year => deadline.ToString("ddd, d MMM", Indonesian) + " " + time,
            _ => deadline.ToString("d MMM yyyy", Indonesian) + " " + time,
        };
    }

    public static DeadlineState StateOf(DateTime? deadline, bool isDone, DateTime now)
    {
        if (deadline is not { } d) return DeadlineState.None;
        if (isDone) return DeadlineState.Done;
        if (d <= now) return DeadlineState.Overdue;
        if (d.Date == now.Date) return DeadlineState.Today;
        if (d - now <= TimeSpan.FromDays(2)) return DeadlineState.Soon;
        return DeadlineState.Later;
    }
}
