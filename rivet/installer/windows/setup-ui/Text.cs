namespace Rivet.Setup
{
    /// <summary>The words on the setup screens (English).</summary>
    internal static class Text
    {
        public const string Caption = "Rivet Browser Setup";
        public const string Title = "Rivet Browser";
        public const string Publisher = "LogitAI";
        public const string Architecture = "x64";
        public const string VersionChip = "Version {0}";

        public const string InstallText =
            "Setup installs the rivet command and the Rivet runtime for the browsers already on this computer.";
        public const string LocationHeader = "Install location";
        public const string ChangeButton = "Change";
        public const string LegalBefore = "By installing, you agree to the ";
        public const string LegalLink = "Apache License 2.0";
        public const string LegalAfter =
            ". Setup does not copy browser profiles, cookies, passkeys or passwords.";
        public const string LicenseUrl = "https://www.apache.org/licenses/LICENSE-2.0";

        public const string OptionsHeader = "Install location";
        public const string OptionsProgramLabel = "Program folder";
        public const string OptionsNote = "The rivet command is added to the system PATH.";
        public const string OptionsInvalid = "Choose a full path, such as C:\\Program Files\\Rivet Browser.";
        public const string BrowseButton = "Browse";
        public const string BrowseProgramTitle = "Choose the program folder";

        public const string ModifyText =
            "Rivet Browser is installed. Repair puts back missing or damaged files. Uninstall removes the program and leaves your browsers and their data alone.";
        public const string InstalledLocation = "Installed in {0}";

        public const string ProgressInstall = "Installing";
        public const string ProgressRepair = "Repairing";
        public const string ProgressUninstall = "Uninstalling";
        public const string ProgressLayout = "Copying the setup files";
        public const string ProgressPreparing = "Preparing\u2026";
        public const string ProgressPreviousVersion = "Removing the previous version\u2026";
        public const string ProgressCancelling = "Cancelling\u2026";

        public const string SuccessInstall = "Installed";
        public const string SuccessInstallText =
            "Open a new terminal and type rivet session start to begin.";
        public const string SuccessRepair = "Repaired";
        public const string SuccessUninstall = "Uninstalled";
        public const string SuccessUninstallText = "Your browsers and their data were not touched.";
        public const string SuccessLayout = "Setup files copied";
        public const string SuccessLayoutText = "The setup files are in {0}.";
        public const string CodeInText = "rivet session start";

        public const string RestartText = "Restart the computer to finish.";

        public const string FailureInstall = "Setup did not finish";
        public const string FailureUninstall = "Uninstall did not finish";
        public const string FailureCancelled = "You cancelled setup. It undid what it had done.";
        public const string FailureLogBefore = "The ";
        public const string FailureLogLink = "log file";
        public const string FailureLogAfter = " says what went wrong.";

        public const string NewerHeader = "A newer version is installed";
        public const string NewerText =
            "A newer Rivet Browser is already on this computer, so setup has nothing to do.";

        public const string HelpHeader = "Setup help";
        public const string HelpText =
            "/install | /repair | /uninstall | /layout [folder]\n" +
            "    installs, repairs, uninstalls or copies the setup\n" +
            "    files to a folder.\n" +
            "/passive | /quiet\n" +
            "    shows only progress, or nothing at all.\n" +
            "/norestart\n" +
            "    never restarts the computer.\n" +
            "/log log.txt\n" +
            "    writes a log to the file.\n" +
            "InstallFolder=<folder>\n" +
            "    sets the program folder.";

        public const string InstallButton = "Install";
        public const string CancelButton = "Cancel";
        public const string BackButton = "Back";
        public const string CloseButton = "Close";
        public const string DoneButton = "Done";
        public const string OkButton = "OK";
        public const string RepairButton = "Repair";
        public const string UninstallButton = "Uninstall";
        public const string RestartButton = "Restart";
        public const string Minimize = "Minimize";

        public const string ConfirmCancel = "Stop setup?";
    }
}
