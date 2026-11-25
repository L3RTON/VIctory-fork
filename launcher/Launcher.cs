// VIctory Configuration: the settings dialog shown before the game starts.
// Graphics, upscaling (FSR) and input options are written to the title's
// settings.xml and passed to victory.exe as VICTORY_* environment
// variables; Play! then starts `victory.exe run <game>\GAME.exe --execute`.
//
// Built with the C# compiler that ships with Windows (.NET Framework 4):
// see build-launcher.ps1. A `banner.png` next to the exe fills the banner.
//
// SPDX-License-Identifier: GPL-3.0-or-later

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace VictoryLauncher
{
    static class Program
    {
        [STAThread]
        static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            var args = Environment.GetCommandLineArgs();
            var form = new ConfigForm();
            int page = 0;
            // `--page <n>`: opens on tab n. `--screenshot <file.png> [n]`: draws tab n to a picture and exits.
            if (args.Length >= 3 && args[1] == "--page") int.TryParse(args[2], out page);
            if (args.Length >= 3 && args[1] == "--screenshot")
            {
                if (args.Length >= 4) int.TryParse(args[3], out page);
                form.Shown += (s, e) =>
                {
                    form.Refresh();
                    using (var bmp = new Bitmap(form.Width, form.Height))
                    {
                        form.DrawToBitmap(bmp, new Rectangle(Point.Empty, form.Size));
                        bmp.Save(args[2], System.Drawing.Imaging.ImageFormat.Png);
                    }
                    form.Close();
                };
            }
            form.SelectTab(page);
            Application.Run(form);
        }
    }

    /// <summary>The dialog's settings, kept in launcher.ini next to the exe.</summary>
    class Settings
    {
        public string VictoryExe = "";
        public string GameDir = @"C:\Users\AleXT10\Downloads\xbox";
        public string Resolution = "1920 x 1080";
        public bool Windowed = true;
        public int Monitor = 1;
        public int Quality = 2;          // Low, Medium, High, Fantastic
        public int FpsLimit = 0;
        public bool RayTracing = false;   // the game's ray-traced shadows setting
        public bool RtBuilder = false;    // VIctory's ray-tracing structure builder (crashes today)
        public bool MotionBlur = true;
        public int Upscaler = 0;         // 0 console FSR4, 1 PC FSR 3.1
        public int FsrQuality = 3;
        public int FsrSharpen = 20;
        public string FsrDllDir = "";
        public bool Warmup = true;
        public bool ShowConsole = false;
        public Dictionary<string, string> Keys = new Dictionary<string, string>();

        public static string Dir { get { return AppDomain.CurrentDomain.BaseDirectory; } }
        public static string IniPath { get { return Path.Combine(Dir, "launcher.ini"); } }

        public static Settings Load()
        {
            var s = new Settings();
            s.VictoryExe = DefaultVictoryExe();
            foreach (var a in InputMap.Actions) s.Keys[a.Id] = a.Default;
            if (!File.Exists(IniPath)) return s;
            foreach (var line in File.ReadAllLines(IniPath))
            {
                int eq = line.IndexOf('=');
                if (eq <= 0) continue;
                string k = line.Substring(0, eq).Trim(), v = line.Substring(eq + 1).Trim();
                int n;
                bool isInt = int.TryParse(v, out n);
                if (k.StartsWith("key.")) { s.Keys[k.Substring(4)] = v; continue; }
                switch (k)
                {
                    case "victory_exe": if (v.Length > 0) s.VictoryExe = v; break;
                    case "game_dir": s.GameDir = v; break;
                    case "resolution": s.Resolution = v; break;
                    case "windowed": s.Windowed = v != "0"; break;
                    case "monitor": if (isInt) s.Monitor = n; break;
                    case "quality": if (isInt) s.Quality = n; break;
                    case "fps_limit": if (isInt) s.FpsLimit = n; break;
                    case "ray_tracing": s.RayTracing = v == "1"; break;
                    case "rt_builder": s.RtBuilder = v == "1"; break;
                    case "motion_blur": s.MotionBlur = v != "0"; break;
                    case "upscaler": if (isInt) s.Upscaler = n; break;
                    case "fsr_quality": if (isInt) s.FsrQuality = n; break;
                    case "fsr_sharpen": if (isInt) s.FsrSharpen = n; break;
                    case "fsr_dll_dir": s.FsrDllDir = v; break;
                    case "warmup": s.Warmup = v != "0"; break;
                    case "show_console": s.ShowConsole = v == "1"; break;
                }
            }
            return s;
        }

        public void Save()
        {
            var sb = new StringBuilder();
            sb.AppendLine("victory_exe=" + VictoryExe);
            sb.AppendLine("game_dir=" + GameDir);
            sb.AppendLine("resolution=" + Resolution);
            sb.AppendLine("windowed=" + (Windowed ? "1" : "0"));
            sb.AppendLine("monitor=" + Monitor);
            sb.AppendLine("quality=" + Quality);
            sb.AppendLine("fps_limit=" + FpsLimit);
            sb.AppendLine("ray_tracing=" + (RayTracing ? "1" : "0"));
            sb.AppendLine("rt_builder=" + (RtBuilder ? "1" : "0"));
            sb.AppendLine("motion_blur=" + (MotionBlur ? "1" : "0"));
            sb.AppendLine("upscaler=" + Upscaler);
            sb.AppendLine("fsr_quality=" + FsrQuality);
            sb.AppendLine("fsr_sharpen=" + FsrSharpen);
            sb.AppendLine("fsr_dll_dir=" + FsrDllDir);
            sb.AppendLine("warmup=" + (Warmup ? "1" : "0"));
            sb.AppendLine("show_console=" + (ShowConsole ? "1" : "0"));
            foreach (var kv in Keys) sb.AppendLine("key." + kv.Key + "=" + kv.Value);
            File.WriteAllText(IniPath, sb.ToString());
        }

        static string DefaultVictoryExe()
        {
            string[] tries = {
                Path.Combine(Dir, "victory.exe"),
                Path.GetFullPath(Path.Combine(Dir, @"..\target\release\victory.exe")),
                @"C:\Users\AleXT10\Desktop\VIctory\target\release\victory.exe",
            };
            foreach (var t in tries) if (File.Exists(t)) return t;
            return tries[tries.Length - 1];
        }
    }

    /// <summary>The pad controls the keyboard can stand in for (VICTORY_KEYMAP names).</summary>
    static class InputMap
    {
        public class Action
        {
            public string Id, Name, Default;
            public Action(string id, string name, string def) { Id = id; Name = name; Default = def; }
        }

        public static readonly Action[] Actions = {
            new Action("move_up", "Move forward (left stick up)", "W"),
            new Action("move_down", "Move back (left stick down)", "S"),
            new Action("move_left", "Move left (left stick left)", "A"),
            new Action("move_right", "Move right (left stick right)", "D"),
            new Action("look_up", "Look up (right stick up)", "I"),
            new Action("look_down", "Look down (right stick down)", "K"),
            new Action("look_left", "Look left (right stick left)", "J"),
            new Action("look_right", "Look right (right stick right)", "L"),
            new Action("a", "A (confirm)", "Enter"),
            new Action("b", "B (back)", "Backspace"),
            new Action("x", "X", "X"),
            new Action("y", "Y", "Y"),
            new Action("lb", "Left bumper", "Q"),
            new Action("rb", "Right bumper", "E"),
            new Action("lt", "Left trigger", "Z"),
            new Action("rt", "Right trigger", "C"),
            new Action("ls", "Left stick click", "F"),
            new Action("rs", "Right stick click", "H"),
            new Action("up", "D-pad up", "Up"),
            new Action("down", "D-pad down", "Down"),
            new Action("left", "D-pad left", "Left"),
            new Action("right", "D-pad right", "Right"),
            new Action("menu", "Menu", "Escape"),
            new Action("view", "View", "Tab"),
        };

        /// <summary>Key names offered, with their virtual-key codes.</summary>
        public static readonly Dictionary<string, int> KeyCodes = BuildKeys();

        static Dictionary<string, int> BuildKeys()
        {
            var d = new Dictionary<string, int>();
            for (char c = 'A'; c <= 'Z'; c++) d[c.ToString()] = c;
            for (char c = '0'; c <= '9'; c++) d[c.ToString()] = c;
            d["Enter"] = 0x0D; d["Escape"] = 0x1B; d["Tab"] = 0x09; d["Backspace"] = 0x08;
            d["Space"] = 0x20; d["Shift"] = 0x10; d["Ctrl"] = 0x11; d["Alt"] = 0x12;
            d["Up"] = 0x26; d["Down"] = 0x28; d["Left"] = 0x25; d["Right"] = 0x27;
            d["Insert"] = 0x2D; d["Delete"] = 0x2E; d["Home"] = 0x24; d["End"] = 0x23;
            d["Page Up"] = 0x21; d["Page Down"] = 0x22;
            for (int i = 1; i <= 12; i++) d["F" + i] = 0x6F + i;
            for (int i = 0; i <= 9; i++) d["Num " + i] = 0x60 + i;
            return d;
        }
    }

    class ConfigForm : Form
    {
        readonly Settings settings = Settings.Load();
        readonly TabControl tabs = new TabControl();
        readonly Dictionary<string, ComboBox> keyBoxes = new Dictionary<string, ComboBox>();

        ComboBox cbResolution, cbQuality, cbMonitor, cbFps, cbUpscaler, cbFsrQuality;
        CheckBox chkWindowed, chkRt, chkBlur, chkWarmup, chkConsole, chkRtBuilder;
        TrackBar tbSharpen;
        Label lblSharpen;
        TextBox txtFsrDir, txtExe, txtGame;

        static readonly string[] Qualities = { "Low", "Medium", "High", "Fantastic" };
        static readonly string[] FsrQualities = {
            "Native AA", "Quality", "Balanced", "Performance", "Ultra Performance",
        };

        public ConfigForm()
        {
            Text = "Launcher Configuration";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;
            StartPosition = FormStartPosition.CenterScreen;
            ClientSize = new Size(500, 470);
            Font = SystemFonts.MessageBoxFont;
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }

            // Banner.
            var banner = new PictureBox
            {
                Bounds = new Rectangle(12, 12, 476, 170),
                BorderStyle = BorderStyle.FixedSingle,
                BackColor = Color.FromArgb(240, 240, 240),
                SizeMode = PictureBoxSizeMode.Zoom,
            };
            string bannerPath = Path.Combine(Settings.Dir, "banner.png");
            if (File.Exists(bannerPath))
                try { banner.Image = Image.FromFile(bannerPath); } catch { }
            Controls.Add(banner);

            tabs.Bounds = new Rectangle(12, 192, 476, 222);
            tabs.TabPages.Add(GraphicsTab());
            tabs.TabPages.Add(FsrTab());
            tabs.TabPages.Add(InputTab());
            tabs.TabPages.Add(AdvancedTab());
            tabs.TabPages.Add(CreditsTab());
            Controls.Add(tabs);

            var play = new Button { Text = "Play!", Bounds = new Rectangle(302, 426, 86, 28) };
            play.Click += (s, e) => Play();
            var quit = new Button { Text = "Quit", Bounds = new Rectangle(402, 426, 86, 28) };
            quit.Click += (s, e) => Close();
            Controls.Add(play);
            Controls.Add(quit);
            AcceptButton = play;
            CancelButton = quit;
        }

        public void SelectTab(int n)
        {
            if (n >= 0 && n < tabs.TabCount) tabs.SelectedIndex = n;
        }

        // ------------------------------------------------------------ tabs

        static Label Text_(Control parent, string text, int x, int y)
        {
            var l = new Label { Text = text, AutoSize = true, Location = new Point(x, y + 3) };
            parent.Controls.Add(l);
            return l;
        }

        static ComboBox Combo(Control parent, int x, int y, int w, IEnumerable<string> items, string selected)
        {
            var cb = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(x, y), Width = w };
            foreach (var i in items) cb.Items.Add(i);
            int at = cb.Items.IndexOf(selected);
            cb.SelectedIndex = at >= 0 ? at : (cb.Items.Count > 0 ? 0 : -1);
            parent.Controls.Add(cb);
            return cb;
        }

        static CheckBox Check(Control parent, string text, int x, int y, bool on)
        {
            var c = new CheckBox { Text = text, Checked = on, AutoSize = true, Location = new Point(x, y + 2) };
            parent.Controls.Add(c);
            return c;
        }

        TextBox PathRow(Control parent, string caption, int y, string value, bool folder)
        {
            Text_(parent, caption, 16, y);
            var t = new TextBox { Text = value, Location = new Point(16, y + 22), Width = 400 };
            var b = new Button { Text = "...", Location = new Point(420, y + 21), Size = new Size(30, 23) };
            b.Click += (s, e) =>
            {
                if (folder)
                {
                    using (var d = new FolderBrowserDialog { SelectedPath = t.Text })
                        if (d.ShowDialog(this) == DialogResult.OK) t.Text = d.SelectedPath;
                }
                else
                {
                    using (var d = new OpenFileDialog { Filter = "victory.exe|victory.exe|Programs|*.exe", FileName = t.Text })
                        if (d.ShowDialog(this) == DialogResult.OK) t.Text = d.FileName;
                }
            };
            parent.Controls.Add(t);
            parent.Controls.Add(b);
            return t;
        }

        static IEnumerable<string> Resolutions()
        {
            var sizes = new List<Size> {
                new Size(1280, 720), new Size(1600, 900), new Size(1920, 1080),
                new Size(2560, 1440), new Size(3840, 2160),
            };
            foreach (var sc in Screen.AllScreens)
                if (!sizes.Contains(sc.Bounds.Size)) sizes.Add(sc.Bounds.Size);
            return sizes.Where(s => s.Width >= 640 && s.Width <= 7680 && s.Height >= 360 && s.Height <= 4320)
                        .OrderBy(s => s.Width * s.Height)
                        .Select(s => s.Width + " x " + s.Height);
        }

        TabPage GraphicsTab()
        {
            var p = new TabPage("Graphics") { UseVisualStyleBackColor = true };
            int lx = 56, cx = 176;
            Text_(p, "Screen resolution", lx, 18);
            cbResolution = Combo(p, cx, 18, 126, Resolutions(), settings.Resolution);
            chkWindowed = Check(p, "Windowed", 322, 18, settings.Windowed);
            Text_(p, "Graphics quality", lx, 50);
            cbQuality = Combo(p, cx, 50, 126, Qualities, Qualities[Clamp(settings.Quality, 0, 3)]);
            Text_(p, "Select monitor", lx, 82);
            var monitors = Screen.AllScreens.Select((s, i) => "Display " + (i + 1) + (s.Primary ? " (main)" : "")).ToList();
            cbMonitor = Combo(p, cx, 82, 126, monitors, "");
            cbMonitor.SelectedIndex = Clamp(settings.Monitor - 1, 0, monitors.Count - 1);
            Text_(p, "Frame rate limit", lx, 114);
            cbFps = Combo(p, cx, 114, 126, new[] { "Unlimited", "30", "60" },
                          settings.FpsLimit == 30 ? "30" : settings.FpsLimit == 60 ? "60" : "Unlimited");
            chkRt = Check(p, "Ray-traced shadows", lx, 148, settings.RayTracing);
            chkBlur = Check(p, "Motion blur", 260, 148, settings.MotionBlur);
            return p;
        }

        TabPage FsrTab()
        {
            var p = new TabPage("FSR") { UseVisualStyleBackColor = true };
            int lx = 56, cx = 176;
            Text_(p, "Upscaler", lx, 18);
            cbUpscaler = Combo(p, cx, 18, 230, new[] { "AMD FSR 4 (console)", "AMD FSR 3.1 (PC SDK)" }, "");
            cbUpscaler.SelectedIndex = Clamp(settings.Upscaler, 0, 1);
            Text_(p, "Quality mode", lx, 50);
            cbFsrQuality = Combo(p, cx, 50, 230, FsrQualities, FsrQualities[Clamp(settings.FsrQuality, 0, 4)]);
            Text_(p, "Sharpness", lx, 82);
            tbSharpen = new TrackBar
            {
                Minimum = 0, Maximum = 100, TickFrequency = 10, SmallChange = 5, LargeChange = 10,
                Value = Clamp(settings.FsrSharpen, 0, 100), Location = new Point(cx - 6, 78), Width = 196,
                AutoSize = false, Height = 30,
            };
            lblSharpen = Text_(p, tbSharpen.Value + "%", cx + 196, 82);
            tbSharpen.ValueChanged += (s, e) => lblSharpen.Text = tbSharpen.Value + "%";
            p.Controls.Add(tbSharpen);
            txtFsrDir = PathRow(p, "FSR 3.1 PC SDK folder (holds amd_fidelityfx_upscaler_dx12.dll)", 116, settings.FsrDllDir, true);
            cbUpscaler.SelectedIndexChanged += (s, e) => txtFsrDir.Enabled = cbUpscaler.SelectedIndex == 1;
            txtFsrDir.Enabled = cbUpscaler.SelectedIndex == 1;
            return p;
        }

        TabPage InputTab()
        {
            var p = new TabPage("Input") { UseVisualStyleBackColor = true };
            Text_(p, "Keyboard (used while no controller is connected):", 10, 6);
            var scroll = new Panel { Location = new Point(4, 30), Size = new Size(460, 160), AutoScroll = true };
            var keyNames = InputMap.KeyCodes.Keys.ToList();
            int y = 2;
            foreach (var a in InputMap.Actions)
            {
                var l = new Label { Text = a.Name, AutoSize = true, Location = new Point(8, y + 3) };
                string current;
                if (!settings.Keys.TryGetValue(a.Id, out current)) current = a.Default;
                var cb = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(250, y), Width = 150 };
                foreach (var k in keyNames) cb.Items.Add(k);
                cb.SelectedItem = keyNames.Contains(current) ? current : a.Default;
                keyBoxes[a.Id] = cb;
                scroll.Controls.Add(l);
                scroll.Controls.Add(cb);
                y += 28;
            }
            var reset = new Button { Text = "Defaults", Location = new Point(396, 4), Size = new Size(66, 23) };
            reset.Click += (s, e) => { foreach (var a in InputMap.Actions) keyBoxes[a.Id].SelectedItem = a.Default; };
            p.Controls.Add(reset);
            p.Controls.Add(scroll);
            return p;
        }

        TabPage AdvancedTab()
        {
            var p = new TabPage("Advanced") { UseVisualStyleBackColor = true };
            txtExe = PathRow(p, "VIctory (victory.exe)", 6, settings.VictoryExe, false);
            txtGame = PathRow(p, "Game folder (holds GAME.exe)", 58, settings.GameDir, true);
            chkWarmup = Check(p, "Prepare every shader before the game starts", 16, 116, settings.Warmup);
            chkConsole = Check(p, "Show the log window", 16, 140, settings.ShowConsole);
            chkRtBuilder = Check(p, "Ray tracing builder (experimental: the game crashes with it today)", 16, 164, settings.RtBuilder);
            return p;
        }

        TabPage CreditsTab()
        {
            var p = new TabPage("Credits") { UseVisualStyleBackColor = true };
            var box = new TextBox
            {
                Multiline = true,
                ReadOnly = true,
                ScrollBars = ScrollBars.Vertical,
                Dock = DockStyle.Fill,
                BackColor = SystemColors.Window,
                Text = LoadCredits().Replace("\r\n", "\n").Replace("\n", "\r\n"),
            };
            p.Controls.Add(box);
            return p;
        }

        /// <summary>credits.txt next to the exe, or a template to fill in.</summary>
        static string LoadCredits()
        {
            string path = Path.Combine(Settings.Dir, "credits.txt");
            if (File.Exists(path)) return File.ReadAllText(path);
            return "VIctory\n\nDEVELOPMENT\n  (name)\n\nSHADERS AND GRAPHICS\n  (name)\n\nTESTING\n  (name)\n\nTHANKS\n  (name)\n\n" +
                   "Edit credits.txt next to Launcher.exe to change this text.";
        }

        static int Clamp(int v, int lo, int hi) { return Math.Max(lo, Math.Min(hi, v)); }

        // ------------------------------------------------------------ play

        void Collect()
        {
            settings.Resolution = (string)cbResolution.SelectedItem;
            settings.Windowed = chkWindowed.Checked;
            settings.Monitor = cbMonitor.SelectedIndex + 1;
            settings.Quality = cbQuality.SelectedIndex;
            settings.FpsLimit = cbFps.SelectedIndex == 1 ? 30 : cbFps.SelectedIndex == 2 ? 60 : 0;
            settings.RayTracing = chkRt.Checked;
            settings.MotionBlur = chkBlur.Checked;
            settings.Upscaler = cbUpscaler.SelectedIndex;
            settings.FsrQuality = cbFsrQuality.SelectedIndex;
            settings.FsrSharpen = tbSharpen.Value;
            settings.FsrDllDir = txtFsrDir.Text.Trim();
            settings.VictoryExe = txtExe.Text.Trim();
            settings.GameDir = txtGame.Text.Trim();
            settings.Warmup = chkWarmup.Checked;
            settings.ShowConsole = chkConsole.Checked;
            settings.RtBuilder = chkRtBuilder.Checked;
            foreach (var kv in keyBoxes) settings.Keys[kv.Key] = (string)kv.Value.SelectedItem;
        }

        static string TitleSettingsPath()
        {
            string local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            return Path.Combine(local, @"victory\data\7952987A\storage\settings\settings.xml");
        }

        /// <summary>Sets `<name value="..." />` in settings.xml, keeping the rest byte for byte.</summary>
        static string SetValue(string xml, string name, string value)
        {
            var re = new Regex("(<" + name + @"\s+value="")[^""]*("")");
            return re.Replace(xml, "${1}" + value + "${2}", 1);
        }

        void WriteTitleSettings()
        {
            string path = TitleSettingsPath();
            if (!File.Exists(path)) return;
            string xml = File.ReadAllText(path);
            string backup = path + ".launcher-backup";
            if (!File.Exists(backup)) File.WriteAllText(backup, xml);
            var inv = CultureInfo.InvariantCulture;
            xml = SetValue(xml, "fsrQuality", settings.FsrQuality.ToString(inv));
            xml = SetValue(xml, "fsrSharpen", (settings.FsrSharpen / 100.0).ToString("0.000000", inv));
            xml = SetValue(xml, "rtDynamicShadows", settings.RayTracing ? "true" : "false");
            xml = SetValue(xml, "motionBlurScale", settings.MotionBlur ? "1.000000" : "0.000000");
            // Quality: level-of-detail distance as the title scales it.
            float[] lod = { 0.6f, 0.8f, 1.0f, 1.2f };
            xml = SetValue(xml, "lodScale", lod[Clamp(settings.Quality, 0, 3)].ToString("0.000000", inv));
            File.WriteAllText(path, xml);
        }

        string KeymapSpec()
        {
            var parts = new List<string>();
            foreach (var a in InputMap.Actions)
            {
                string key;
                int vk;
                if (settings.Keys.TryGetValue(a.Id, out key) && InputMap.KeyCodes.TryGetValue(key, out vk))
                    parts.Add(a.Id + ":" + vk);
            }
            return string.Join(",", parts);
        }

        void Play()
        {
            Collect();
            try { settings.Save(); } catch { }
            string exe = settings.VictoryExe;
            string game = Path.Combine(settings.GameDir, "GAME.exe");
            if (!File.Exists(exe))
            {
                MessageBox.Show(this, "victory.exe was not found:\n" + exe + "\n\nSet it on the Advanced tab.", Text,
                                MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            if (!File.Exists(game))
            {
                MessageBox.Show(this, "GAME.exe was not found in:\n" + settings.GameDir + "\n\nSet it on the Advanced tab.", Text,
                                MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }
            bool pcFsr = settings.Upscaler == 1 && settings.FsrDllDir.Length > 0 &&
                         File.Exists(Path.Combine(settings.FsrDllDir, "amd_fidelityfx_upscaler_dx12.dll"));
            if (settings.Upscaler == 1 && !pcFsr)
            {
                var r = MessageBox.Show(this, "The FSR 3.1 PC SDK folder does not hold amd_fidelityfx_upscaler_dx12.dll.\n" +
                                        "Start with the console's FSR 4 instead?", Text,
                                        MessageBoxButtons.OKCancel, MessageBoxIcon.Information);
                if (r != DialogResult.OK) return;
            }
            try { WriteTitleSettings(); }
            catch (Exception ex)
            {
                MessageBox.Show(this, "The game's settings.xml was not updated: " + ex.Message, Text);
            }

            var psi = new ProcessStartInfo(exe, "run \"" + game + "\" --execute")
            {
                UseShellExecute = false,
                CreateNoWindow = !settings.ShowConsole,
                WorkingDirectory = Path.GetDirectoryName(exe),
            };
            var env = psi.EnvironmentVariables;
            Action<string, string> set = (k, v) => { if (v == null) env.Remove(k); else env[k] = v; };
            set("VICTORY_DISPLAY_MODE", settings.Resolution.Replace(" ", ""));
            set("VICTORY_WINDOWED", settings.Windowed ? null : "0");
            set("VICTORY_MONITOR", settings.Monitor.ToString());
            set("VICTORY_FPS_LIMIT", settings.FpsLimit == 0 ? null : settings.FpsLimit.ToString());
            // Quality: texture detail where the card's memory allows.
            string[] mipDrop = { "2", "1", null, "0" };
            set("VICTORY_TEXTURE_MIP_DROP", mipDrop[Clamp(settings.Quality, 0, 3)]);
            set("VICTORY_BVH_BUILDER", settings.RtBuilder ? "1" : null);
            set("VICTORY_FIDELITYFX_DIR", pcFsr ? settings.FsrDllDir : null);
            set("VICTORY_WARMUP", settings.Warmup ? null : "0");
            set("VICTORY_KEYMAP", KeymapSpec());

            try
            {
                Process.Start(psi);
                Close();
            }
            catch (Exception ex)
            {
                MessageBox.Show(this, "Could not start the game: " + ex.Message, Text, MessageBoxButtons.OK, MessageBoxIcon.Error);
            }
        }
    }
}
