using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media.Animation;
using PatPet.Interop;

namespace PatPet.Views;

/// <summary>Jendela tray berisi tab. Dibuat saat dibuka, lalu ditutup dan dibuang saat disembunyikan.</summary>
public partial class PanelWindow : Window
{
    public const double PanelWidth = 400;

    public PanelWindow(bool attachedToBar)
    {
        InitializeComponent();
        Panel.SetAttached(attachedToBar);
        SourceInitialized += (_, _) => Win32.MakeToolWindow(new WindowInteropHelper(this).Handle);
        Loaded += (_, _) => Panel.BeginAnimation(OpacityProperty, new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(110)));
    }

    public event Action? CloseRequested;

    protected override void OnPreviewKeyDown(KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            CloseRequested?.Invoke();
            e.Handled = true;
            return;
        }
        base.OnPreviewKeyDown(e);
    }
}
