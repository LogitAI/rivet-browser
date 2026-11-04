namespace Rivet.Setup
{
    using System;
    using System.Collections.Generic;
    using System.Runtime.InteropServices;
    using System.Windows;
    using System.Windows.Interop;
    using System.Windows.Media;
    using Microsoft.Win32;

    /// <summary>
    /// The colours of the setup window in the light and the dark theme, and the Mica backdrop
    /// behind them. The window follows the theme Windows uses for apps, and changes with it.
    /// </summary>
    internal static class Palette
    {
        private const int DwmwaUseImmersiveDarkMode = 20;
        private const int DwmwaWindowCornerPreference = 33;
        private const int DwmwaSystembackdropType = 38;
        private const int DwmwaMicaEffect = 1029;
        private const int DwmwcpRound = 2;
        private const int DwmsbtMainWindow = 2;

        /// <summary>The Windows build number (RtlGetVersion does not lie for compatibility).</summary>
        public static readonly int Build = ReadBuild();

        /// <summary>Windows 11 draws rounded corners and the Mica material.</summary>
        public static bool HasMica => Build >= 22000;

        /// <summary>Whether apps use the dark theme.</summary>
        public static bool IsDark()
        {
            try
            {
                using (var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"))
                {
                    return key?.GetValue("AppsUseLightTheme") is int light && light == 0;
                }
            }
            catch (Exception)
            {
                return false;
            }
        }

        /// <summary>Puts the brushes of one theme into the window's resources.</summary>
        public static void Apply(Window window, bool dark)
        {
            foreach (var pair in dark ? Dark : Light)
            {
                window.Resources[pair.Key] = Frozen(pair.Value);
            }

            // Setup uses the same paper or charcoal surface as Rivet itself. Mica may still
            // shape the window frame, but it must not dilute the product's signature colour.
            window.Resources["WindowBackground"] = Frozen(dark
                ? Rgb(0x20, 0x1E, 0x20)
                : Rgb(0xF2, 0xEE, 0xE5));

            window.Resources["ButtonBorder"] = Elevation(dark ? Argb(0x18, 0xFF, 0xFF, 0xFF) : Argb(0x0F, 0, 0, 0),
                                                         dark ? Argb(0x12, 0xFF, 0xFF, 0xFF) : Argb(0x29, 0, 0, 0));
            window.Resources["AccentBorder"] = Elevation(Argb(0x14, 0xFF, 0xFF, 0xFF),
                                                         dark ? Argb(0x24, 0, 0, 0) : Argb(0x66, 0, 0, 0));
            window.Resources["TextBoxBorder"] = Elevation(dark ? Argb(0x12, 0xFF, 0xFF, 0xFF) : Argb(0x0F, 0, 0, 0),
                                                          dark ? Argb(0x8B, 0xFF, 0xFF, 0xFF) : Argb(0x72, 0, 0, 0));

            var handle = new WindowInteropHelper(window).Handle;
            if (handle != IntPtr.Zero)
            {
                ApplyBackdrop(handle, dark);
            }
        }

        /// <summary>Asks the window manager for rounded corners, Mica and the dark frame.</summary>
        public static void ApplyBackdrop(IntPtr handle, bool dark)
        {
            int value = dark ? 1 : 0;
            int darkResult = DwmSetWindowAttribute(handle, DwmwaUseImmersiveDarkMode, ref value, sizeof(int));
            if (!HasMica)
            {
                BackdropResult = string.Format("dark frame 0x{0:X8}", darkResult);
                return;
            }

            value = DwmwcpRound;
            int cornerResult = DwmSetWindowAttribute(handle, DwmwaWindowCornerPreference, ref value, sizeof(int));
            int micaResult;
            if (Build >= 22621)
            {
                value = DwmsbtMainWindow;
                micaResult = DwmSetWindowAttribute(handle, DwmwaSystembackdropType, ref value, sizeof(int));
            }
            else
            {
                // Windows 11 21H2 knew Mica only by this older, undocumented attribute.
                value = 1;
                micaResult = DwmSetWindowAttribute(handle, DwmwaMicaEffect, ref value, sizeof(int));
            }

            BackdropResult = string.Format("dark frame 0x{0:X8}, rounded corners 0x{1:X8}, Mica 0x{2:X8}", darkResult, cornerResult, micaResult);
        }

        /// <summary>What the window manager answered to the last backdrop request, for the log.</summary>
        public static string BackdropResult { get; private set; } = "not asked";

        // The tokens of the approved design. Translucent fills sit on Mica, or on the solid
        // background where there is no Mica.
        private static readonly Dictionary<string, Color> Light = new Dictionary<string, Color>
        {
            ["TextPrimary"] = Rgb(0x1F, 0x20, 0x24),
            ["TextSecondary"] = Rgb(0x68, 0x64, 0x5D),
            ["TextDisabled"] = Argb(0x5C, 0, 0, 0),
            ["CardFill"] = Rgb(0xFB, 0xFA, 0xF6),
            ["CardStroke"] = Argb(0x2E, 0x1F, 0x20, 0x24),
            ["LayerFill"] = Rgb(0xE9, 0xE3, 0xD8),
            ["LayerStroke"] = Argb(0x33, 0x1F, 0x20, 0x24),
            ["ButtonFill"] = Rgb(0xFB, 0xFA, 0xF6),
            ["ButtonFillHover"] = Rgb(0xEE, 0xE8, 0xDD),
            ["ButtonFillPressed"] = Rgb(0xE3, 0xDA, 0xCD),
            ["ButtonFillDisabled"] = Argb(0x66, 0xFB, 0xFA, 0xF6),
            ["Accent"] = Rgb(0x7C, 0x2D, 0x4F),
            ["AccentHover"] = Rgb(0x69, 0x23, 0x41),
            ["AccentPressed"] = Rgb(0x52, 0x19, 0x31),
            ["AccentDisabled"] = Argb(0x37, 0, 0, 0),
            ["AccentText"] = Colors.White,
            ["AccentTextPressed"] = Argb(0xB3, 0xFF, 0xFF, 0xFF),
            ["ChipFill"] = Argb(0x0F, 0, 0, 0),
            ["Link"] = Rgb(0x7C, 0x2D, 0x4F),
            ["LinkHover"] = Rgb(0x52, 0x19, 0x31),
            ["Track"] = Argb(0x72, 0x1F, 0x20, 0x24),
            ["Critical"] = Rgb(0xC4, 0x2B, 0x1C),
            ["CriticalText"] = Colors.White,
            ["Neutral"] = Rgb(0x68, 0x64, 0x5D),
            ["NeutralText"] = Colors.White,
            ["SubtleHover"] = Argb(0x09, 0, 0, 0),
            ["SubtlePressed"] = Argb(0x06, 0, 0, 0),
            ["TextBoxFill"] = Rgb(0xFB, 0xFA, 0xF6),
            ["TextBoxFillHover"] = Rgb(0xF4, 0xEF, 0xE6),
            ["TextBoxFillFocused"] = Rgb(0xFF, 0xFC, 0xF7),
            ["FocusOuter"] = Rgb(0x7C, 0x2D, 0x4F),
            ["FocusInner"] = Argb(0xB3, 0xFF, 0xFC, 0xF7),
        };

        private static readonly Dictionary<string, Color> Dark = new Dictionary<string, Color>
        {
            ["TextPrimary"] = Rgb(0xF7, 0xF1, 0xE7),
            ["TextSecondary"] = Rgb(0xD1, 0xC7, 0xBB),
            ["TextDisabled"] = Argb(0x5D, 0xFF, 0xFF, 0xFF),
            ["CardFill"] = Argb(0x0D, 0xFF, 0xFF, 0xFF),
            ["CardStroke"] = Argb(0x1A, 0, 0, 0),
            ["LayerFill"] = Rgb(0x2A, 0x26, 0x29),
            ["LayerStroke"] = Argb(0x12, 0xFF, 0xFF, 0xFF),
            ["ButtonFill"] = Argb(0x0F, 0xFF, 0xFF, 0xFF),
            ["ButtonFillHover"] = Argb(0x15, 0xFF, 0xFF, 0xFF),
            ["ButtonFillPressed"] = Argb(0x08, 0xFF, 0xFF, 0xFF),
            ["ButtonFillDisabled"] = Argb(0x0B, 0xFF, 0xFF, 0xFF),
            ["Accent"] = Rgb(0xD8, 0x7C, 0xA4),
            ["AccentHover"] = Rgb(0xE7, 0x95, 0xB7),
            ["AccentPressed"] = Rgb(0xB8, 0x58, 0x81),
            ["AccentDisabled"] = Argb(0x28, 0xFF, 0xFF, 0xFF),
            ["AccentText"] = Rgb(0x32, 0x0B, 0x20),
            ["AccentTextPressed"] = Argb(0xCC, 0x32, 0x0B, 0x20),
            ["ChipFill"] = Argb(0x15, 0xFF, 0xFF, 0xFF),
            ["Link"] = Rgb(0xE7, 0x95, 0xB7),
            ["LinkHover"] = Rgb(0xF1, 0xB2, 0xCA),
            ["Track"] = Argb(0x8B, 0xFF, 0xFF, 0xFF),
            ["Critical"] = Rgb(0xFF, 0x99, 0xA4),
            ["CriticalText"] = Rgb(0x2B, 0x0A, 0x0E),
            ["Neutral"] = Rgb(0xC5, 0xC5, 0xC5),
            ["NeutralText"] = Rgb(0x1B, 0x1B, 0x1B),
            ["SubtleHover"] = Argb(0x0F, 0xFF, 0xFF, 0xFF),
            ["SubtlePressed"] = Argb(0x0A, 0xFF, 0xFF, 0xFF),
            ["TextBoxFill"] = Argb(0x0F, 0xFF, 0xFF, 0xFF),
            ["TextBoxFillHover"] = Argb(0x15, 0xFF, 0xFF, 0xFF),
            ["TextBoxFillFocused"] = Argb(0xB3, 0x1E, 0x1E, 0x1E),
            ["FocusOuter"] = Colors.White,
            ["FocusInner"] = Argb(0xB3, 0, 0, 0),
        };

        /// <summary>
        /// A control border whose bottom edge is darker, as Windows 11 draws its controls: the
        /// last pixel takes the bottom colour and blends into the edge colour above it.
        /// </summary>
        private static Brush Elevation(Color edge, Color bottom)
        {
            var brush = new LinearGradientBrush
            {
                MappingMode = BrushMappingMode.Absolute,
                StartPoint = new Point(0, 0),
                EndPoint = new Point(0, 3),
                RelativeTransform = new ScaleTransform(1, -1, 0, 0.5),
            };
            brush.GradientStops.Add(new GradientStop(bottom, 0.33));
            brush.GradientStops.Add(new GradientStop(edge, 1.0));
            brush.Freeze();
            return brush;
        }

        private static SolidColorBrush Frozen(Color color)
        {
            var brush = new SolidColorBrush(color);
            brush.Freeze();
            return brush;
        }

        private static Color Rgb(byte r, byte g, byte b) => Color.FromRgb(r, g, b);

        private static Color Argb(byte a, byte r, byte g, byte b) => Color.FromArgb(a, r, g, b);

        private static int ReadBuild()
        {
            var info = new OsVersionInfo { Size = Marshal.SizeOf(typeof(OsVersionInfo)) };
            try
            {
                return RtlGetVersion(ref info) == 0 ? info.BuildNumber : 0;
            }
            catch (Exception)
            {
                return 0;
            }
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct OsVersionInfo
        {
            public int Size;
            public int MajorVersion;
            public int MinorVersion;
            public int BuildNumber;
            public int PlatformId;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
            public string CsdVersion;
        }

        [DllImport("ntdll.dll")]
        private static extern int RtlGetVersion(ref OsVersionInfo info);

        [DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);
    }
}
