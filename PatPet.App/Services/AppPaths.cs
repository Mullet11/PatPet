namespace PatPet.Services;

public static class AppPaths
{
    public static string DataDir { get; } =
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "PatPet");

    public static string SettingsFile => Path.Combine(DataDir, "settings.json");
    public static string NotesDb => Path.Combine(DataDir, "notes.db");
    public static string BackupDir => Path.Combine(DataDir, "backups");
    public static string ErrorLog => Path.Combine(DataDir, "error.log");
}
