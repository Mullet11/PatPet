using System.Windows.Controls;
using System.Windows.Input;
using PatPet.ViewModels;

namespace PatPet.Views.Pages;

public partial class ClockPage : UserControl
{
    public ClockPage()
    {
        InitializeComponent();
        Pet.MouseLeftButtonUp += OnPetClicked;
    }

    private void OnPetClicked(object sender, MouseButtonEventArgs e)
    {
        (DataContext as MainViewModel)?.Pet.ClickCommand.Execute(null);
        e.Handled = true;
    }
}
