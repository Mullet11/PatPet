using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Media;
using PatPet.Services;

namespace PatPet.Views.Controls;

/// <summary>Chip "deadline" yang membuka kalender + jam di popup.</summary>
public partial class DeadlinePicker : UserControl
{
    public static readonly DependencyProperty ValueProperty = DependencyProperty.Register(
        nameof(Value), typeof(DateTime?), typeof(DeadlinePicker),
        new FrameworkPropertyMetadata(null, FrameworkPropertyMetadataOptions.BindsTwoWayByDefault, (d, _) => ((DeadlinePicker)d).UpdateLabel()));

    public static readonly DependencyProperty PlaceholderProperty = DependencyProperty.Register(
        nameof(Placeholder), typeof(string), typeof(DeadlinePicker),
        new PropertyMetadata("Deadline", (d, _) => ((DeadlinePicker)d).UpdateLabel()));

    public DeadlinePicker()
    {
        InitializeComponent();
        Pop.Opened += (_, _) => { LoadEditor(); Toggle.IsHitTestVisible = false; };
        Pop.Closed += (_, _) => Toggle.IsHitTestVisible = true; // klik chip saat terbuka cukup menutup
        // Calendar menahan mouse capture setelah tanggal diklik; lepaskan agar tombol lain langsung bisa diklik
        Cal.PreviewMouseUp += (_, _) => { if (Mouse.Captured is CalendarItem) Mouse.Capture(null); };
        TimeBox.KeyDown += (_, e) => { if (e.Key == Key.Enter) OnSave(this, new RoutedEventArgs()); };
        UpdateLabel();
    }

    public DateTime? Value { get => (DateTime?)GetValue(ValueProperty); set => SetValue(ValueProperty, value); }
    public string Placeholder { get => (string)GetValue(PlaceholderProperty); set => SetValue(PlaceholderProperty, value); }

    private void UpdateLabel()
    {
        if (Value is { } v)
        {
            LabelText.Text = DeadlineFormat.Format(v, DateTime.Now);
            Toggle.Foreground = (Brush)FindResource(v <= DateTime.Now ? "DangerBrush" : "AccentBrush");
        }
        else
        {
            LabelText.Text = Placeholder;
            Toggle.Foreground = (Brush)FindResource("MutedBrush");
        }
    }

    private void LoadEditor()
    {
        ErrorText.Visibility = Visibility.Collapsed;
        var d = Value ?? DateTime.Today;
        Cal.SelectedDate = d.Date;
        Cal.DisplayDate = d.Date;
        TimeBox.Text = Value?.ToString("HH:mm", CultureInfo.InvariantCulture) ?? "23:59";
    }

    private void SetDate(DateTime d)
    {
        Cal.SelectedDate = d.Date;
        Cal.DisplayDate = d.Date;
    }

    private void OnToday(object sender, RoutedEventArgs e) => SetDate(DateTime.Today);
    private void OnTomorrow(object sender, RoutedEventArgs e) => SetDate(DateTime.Today.AddDays(1));
    private void OnNextWeek(object sender, RoutedEventArgs e) => SetDate(DateTime.Today.AddDays(7));

    private void OnClear(object sender, RoutedEventArgs e)
    {
        Value = null;
        Toggle.IsChecked = false;
    }

    private void OnSave(object sender, RoutedEventArgs e)
    {
        if (!TryParseTime(TimeBox.Text, out var time))
        {
            ErrorText.Visibility = Visibility.Visible;
            return;
        }
        Value = (Cal.SelectedDate ?? DateTime.Today).Date + time;
        Toggle.IsChecked = false;
    }

    internal static bool TryParseTime(string? s, out TimeSpan time)
    {
        time = default;
        s = s?.Trim().Replace('.', ':');
        return !string.IsNullOrEmpty(s)
               && TimeSpan.TryParseExact(s, new[] { @"h\:mm", @"hh\:mm" }, CultureInfo.InvariantCulture, out time)
               && time >= TimeSpan.Zero && time < TimeSpan.FromDays(1);
    }
}
