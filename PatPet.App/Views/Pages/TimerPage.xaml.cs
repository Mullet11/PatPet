using System.Windows.Controls;
using System.Windows.Input;
using PatPet.ViewModels;

namespace PatPet.Views.Pages;

public partial class TimerPage : UserControl
{
    public TimerPage()
    {
        InitializeComponent();
        // gulir mouse di atas jam/menit/detik untuk menaikkan atau menurunkan
        foreach (var seg in new[] { HourSeg, MinSeg, SecSeg })
            seg.PreviewMouseWheel += OnSegmentWheel;
    }

    private void OnSegmentWheel(object sender, MouseWheelEventArgs e)
    {
        if (DataContext is not MainViewModel vm || sender is not StackPanel { Tag: string unit }) return;
        var step = Keyboard.Modifiers.HasFlag(ModifierKeys.Shift) ? 5 : 1;
        vm.Timer.StepCommand.Execute($"{unit}{(e.Delta > 0 ? "+" : "-")}{step}");
        e.Handled = true;
    }
}
