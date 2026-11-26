using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// The design's units: the Android TV app draws on a 960 × 540 dp canvas (PleyTv.CanvasWidth),
/// and this app's window is 1920 × 1080 pixels, so a dp or an sp is <see cref="Scale"/> pixels.
/// Screens are written in the Android app's dp, through <see cref="Dp"/>.
/// </summary>
static class Ui
{
    /// <summary>Pixels per dp: the window's width over the canvas's 960.</summary>
    public static float Scale { get; private set; } = 2f;

    public static void Measure(Window window) => Scale = window.Size.Width / 960f;

    public static float Dp(float dp) => dp * Scale;

    public static int DpInt(float dp) => (int)System.Math.Round(dp * Scale);
}

/// <summary>
/// Pley's colors (the Android app's PleyColors, design spec §1.1) in the theme and accent this TV
/// shows (docs/THEMES.md): Core's <see cref="PleyPalette"/>, each a token of
/// packages/themes/themes.json (ThemeTests), as NUI colors made once per pick. A pick draws the
/// pages again (App's Redraw); while a player shows, the colors are its, a light theme's dark twin's.
/// </summary>
static class PleyColors
{
    static ThemeColors pages = ThemeColors.Of(PleyPalette.Of(ThemePick.Default));
    static ThemeColors? player;
    static ThemeColors now = pages;

    /// <summary>The colors drawn from now on: the pick's, for the pages.</summary>
    public static void Use(ThemePick pick)
    {
        pages = ThemeColors.Of(PleyPalette.Of(pick));
        player = null;
        darkInk = null;
        now = pages;
    }

    /// <summary>
    /// A player, or anything over video, is being drawn (true) or has gone (false): its colors are
    /// the pages' dark twin's in a light theme, so a light frame never glares around the picture.
    /// </summary>
    public static void OverVideo(bool on)
    {
        if (!on)
        {
            now = pages;
            return;
        }
        var twin = pages.Palette.Player;
        now = player ??= twin == pages.Palette ? pages : ThemeColors.Of(twin);
    }

    /// <summary>The colors drawn now, by token (the Appearance page's previews draw others').</summary>
    public static ThemeColors Now => now;

    public static Color Bg => now.Bg;
    /// <summary>The expanded rail, settings groups.</summary>
    public static Color Surface => now.Surface;
    /// <summary>Tile wells, inputs, dialogs, panels, skeletons.</summary>
    public static Color Surface2 => now.Surface2;
    /// <summary>Unfocused buttons and chips, the current page in lists.</summary>
    public static Color Surface3 => now.Surface3;
    /// <summary>Dividers, hairlines, the progress track.</summary>
    public static Color Surface4 => now.Surface4;
    /// <summary>Decorative borders only.</summary>
    public static Color Outline => now.Outline;
    /// <summary>Edges of interactive things: input outlines, empty PIN dots, outlined pills.</summary>
    public static Color Control => now.Control;
    /// <summary>Primary text and icons.</summary>
    public static Color Fg => now.Fg;
    public static Color Muted => now.Muted;
    /// <summary>Tertiary text: dates, separators.</summary>
    public static Color Subtle => now.Subtle;
    public static Color Disabled => now.Disabled;
    /// <summary>TV focus fills; progress; checks.</summary>
    public static Color Accent => now.Accent;
    /// <summary>The TV focus ring.</summary>
    public static Color AccentBright => now.AccentBright;
    /// <summary>Content on Accent and AccentBright.</summary>
    public static Color OnAccent => now.OnAccent;
    /// <summary>The accent at 20 % over the page: a program airing in the guide, the New badge (the Android app's PrimaryContainer).</summary>
    public static Color AccentTint => now.AccentTint;
    public static Color BrandLight => now.BrandLight;
    public static Color Brand => now.Brand;
    public static Color BrandDeep => now.BrandDeep;
    public static Color BrandFar => now.BrandFar;
    public static Color BrandTile => now.BrandTile;
    public static Color Success => now.Success;
    /// <summary>Danger as a fill, whose words are <see cref="DangerFg"/>.</summary>
    public static Color Danger => now.Danger;
    /// <summary>Danger as words and icons.</summary>
    public static Color DangerText => now.Danger;
    /// <summary>Words on a Danger fill.</summary>
    public static Color DangerFg => now.DangerFg;
    /// <summary>Requests waiting for approval; words on it are Bg.</summary>
    public static Color Warning => now.Warning;
    /// <summary>Requests on their way: the tint behind their pill, a download's bar.</summary>
    public static Color Info => now.Info;
    /// <summary>Badges, the watched tick and round buttons on art: Bg at 80 %.</summary>
    public static Color OnArt => now.OnArt;
    /// <summary>The progress track over art: black at 60 %, as it sits on a picture.</summary>
    public static readonly Color ProgressTrackOnArt = new Color(0, 0, 0, 0.6f);
    /// <summary>The hairline inside a tile's well: Fg at 6 %.</summary>
    public static Color TileHairline => now.TileHairline;
    /// <summary>Behind dialogs, sheets and panels: the theme's shade at 60 %.</summary>
    public static Color Scrim => now.Scrim;
    /// <summary>
    /// Words on a color of the person's own, a profile's avatar, when it needs dark ones: the dark
    /// twin's page color, dark in every theme, as white is for the others.
    /// </summary>
    public static Color DarkInk => darkInk ??= ThemeColors.Of(pages.Palette.Player.Bg);

    static Color? darkInk;

    /// <summary><paramref name="color"/> at <paramref name="alpha"/>: a wash of a theme's color.</summary>
    public static Color Alpha(Color color, float alpha) => new Color(color.R, color.G, color.B, alpha);
}

/// <summary>A palette's colors as NUI's, made once: <see cref="PleyColors"/>' now, or a preview's on the Appearance page.</summary>
sealed class ThemeColors
{
    ThemeColors(PleyPalette p)
    {
        Palette = p;
        Bg = Of(p.Bg);
        Surface = Of(p.Surface);
        Surface2 = Of(p.Surface2);
        Surface3 = Of(p.Surface3);
        Surface4 = Of(p.Surface4);
        Outline = Of(p.Outline);
        Control = Of(p.Control);
        Fg = Of(p.Fg);
        Muted = Of(p.Muted);
        Subtle = Of(p.Subtle);
        Disabled = Of(p.Disabled);
        Accent = Of(p.Accent);
        AccentBright = Of(p.AccentBright);
        OnAccent = Of(p.OnAccent);
        AccentTint = Of(p.Accent.Times(0.2f));
        BrandLight = Of(p.BrandLight);
        Brand = Of(p.Brand);
        BrandDeep = Of(p.BrandDeep);
        BrandFar = Of(p.BrandFar);
        BrandTile = Of(p.BrandTile);
        Success = Of(p.Success);
        Danger = Of(p.Danger);
        DangerFg = Of(p.DangerFg);
        Warning = Of(p.Warning);
        Info = Of(p.Info);
        OnArt = Of(p.OnArt);
        TileHairline = Of(p.TileHairline);
        Scrim = Of(p.Scrim);
    }

    public static ThemeColors Of(PleyPalette palette) => new ThemeColors(palette);

    public static Color Of(Rgba c) => new Color(c.R / 255f, c.G / 255f, c.B / 255f, c.A);

    public PleyPalette Palette { get; }
    public Color Bg { get; }
    public Color Surface { get; }
    public Color Surface2 { get; }
    public Color Surface3 { get; }
    public Color Surface4 { get; }
    public Color Outline { get; }
    public Color Control { get; }
    public Color Fg { get; }
    public Color Muted { get; }
    public Color Subtle { get; }
    public Color Disabled { get; }
    public Color Accent { get; }
    public Color AccentBright { get; }
    public Color OnAccent { get; }
    public Color AccentTint { get; }
    public Color BrandLight { get; }
    public Color Brand { get; }
    public Color BrandDeep { get; }
    public Color BrandFar { get; }
    public Color BrandTile { get; }
    public Color Success { get; }
    public Color Danger { get; }
    public Color DangerFg { get; }
    public Color Warning { get; }
    public Color Info { get; }
    public Color OnArt { get; }
    public Color TileHairline { get; }
    public Color Scrim { get; }
}

/// <summary>A text style: size and line height in sp, weight, tracking in sp and face (the Android app's PleyType).</summary>
sealed record TextStyle(float Size, float Line, string Weight, float Tracking = 0, string Family = PleyType.Roboto);

/// <summary>
/// Pley's TV type (PleyType.Tv, design spec §1.2): Roboto, as the Android app has it, which the app
/// ships (shared/res/fonts, made by scripts/fonts.mjs); weights 400, 500 and 700 only.
/// </summary>
static class PleyType
{
    public const string Roboto = "Roboto";
    /// <summary>The TV's own monospaced face, for the link code (the web TV app's fallback).</summary>
    public const string Monospace = "monospace";

    public const string Regular = "normal";
    public const string Medium = "medium";
    public const string Bold = "bold";

    /// <summary>A title without logo art; 2 lines at most.</summary>
    public static readonly TextStyle HeroTitle = new TextStyle(38, 44, Bold, -0.5f);
    /// <summary>Library, Search, Settings, Who's watching?</summary>
    public static readonly TextStyle PageTitle = new TextStyle(28, 34, Bold, -0.25f);
    /// <summary>Row headers.</summary>
    public static readonly TextStyle Section = new TextStyle(18, 24, Bold);
    /// <summary>Episode titles in info blocks; panel and dialog titles; the title on a playlist cover's placeholder.</summary>
    public static readonly TextStyle Title = new TextStyle(16, 22, Bold);
    /// <summary>Summaries on the hero and the item page.</summary>
    public static readonly TextStyle BodyLarge = new TextStyle(15, 22, Regular);
    /// <summary>Lists, settings rows, instructions, 2-line summaries.</summary>
    public static readonly TextStyle Body = new TextStyle(14, 20, Regular);
    /// <summary>Buttons, the metadata line, tabs, chips.</summary>
    public static readonly TextStyle Label = new TextStyle(14, 18, Medium, 0.1f);
    /// <summary>Tile captions, scrubber times.</summary>
    public static readonly TextStyle Caption = new TextStyle(13, 17, Medium, 0.2f);
    /// <summary>Quality badges, as written ("1080p").</summary>
    public static readonly TextStyle Badge = new TextStyle(11, 13, Bold, 0.4f);
    /// <summary>The link code, in a monospaced face.</summary>
    public static readonly TextStyle Code = new TextStyle(48, 56, Bold, 4, Monospace);
    /// <summary>PIN pad keys.</summary>
    public static readonly TextStyle PinDigit = new TextStyle(28, 28, Medium);
}

/// <summary>The TV layout's fixed lines on the 960 × 540 canvas (PleyTv, TvSafeArea, PleySpace).</summary>
static class PleyTv
{
    public const float CanvasWidth = 960, CanvasHeight = 540;
    /// <summary>What a TV keeps clear around the picture (overscan): 48 at the sides, 27 above and below.</summary>
    public const float SafeHorizontal = 48, SafeVertical = 27;
    /// <summary>The collapsed rail: 8 + a 56 dp item + 8.</summary>
    public const float RailCollapsed = 72;
    /// <summary>The expanded rail: 8 + a 256 dp item + 8.</summary>
    public const float RailExpanded = 272;
    /// <summary>Every page's left edge: the rail and 24. Rows keep their focused tile here.</summary>
    public const float Gutter = 96;
    /// <summary>The right edge of text over backdrops.</summary>
    public const float TextColumnEnd = 560;
    /// <summary>Where the page puts a focused row's header, under the info block.</summary>
    public const float FocusedRowHeaderY = 172;
    public const float RowHeaderHeight = 24;
    /// <summary>From a row header to its tiles: room for the focused tile's growth and ring.</summary>
    public const float HeaderToTiles = 14;
    public const float TilesToCaption = 14;
    /// <summary>A caption's two lines of Caption.</summary>
    public const float CaptionHeight = 34;
    /// <summary>Between one row's caption and the next row's header.</summary>
    public const float RowGap = 28;
    /// <summary>Between posters, squares and people.</summary>
    public const float TileGap = 14;
    /// <summary>Between landscape and episode tiles, which grow more when focused.</summary>
    public const float WideTileGap = 18;
    public const float ButtonGap = 10;
    public const float ButtonHeight = 40;
    public const float ListRowHeight = 44;
    public const float IconSize = 20;
    /// <summary>The rows' fade under the collapsed rail: Bg to this x…</summary>
    public const float RailFadeSolidUntil = 56;
    /// <summary>…and clear from this x.</summary>
    public const float RailFadeClearFrom = 72;
}

/// <summary>Corner radii (PleyShape) and the focus look (PleyFocus).</summary>
static class PleyShape
{
    public const float BadgeRadius = 2, TileRadius = 4, FieldRadius = 4, ControlRadius = 6, CardRadius = 6, DialogRadius = 8;
    /// <summary>The focus ring: a 3 dp AccentBright stroke centered 4.5 dp outside the tile.</summary>
    public const float RingStroke = 3, RingOffset = 4.5f;
    /// <summary>A focused scrubber thumb's ring: a 2 dp stroke 2 dp clear of it.</summary>
    public const float ThumbRingStroke = 2, ThumbRingOffset = 2;
    public const float TileScale = 1.08f, TilePressedScale = 1.04f, RoundScale = 1.08f, AvatarScale = 1.1f, PillScale = 1.05f;
}

/// <summary>How Pley moves (PleyMotion, design spec §5.1): short, eased, never bouncing.</summary>
static class PleyMotion
{
    public const int FocusMs = 150, UnfocusMs = 120, PressMs = 100, RevealMs = 150, BackdropMs = 300, RestMs = 400, OverlayMs = 200, ScreenMs = 220, ImageFadeMs = 150;
}

/// <summary>
/// Text the way the Android app sets it: the style's size and weight, one line unless asked,
/// ending in "…" when it doesn't fit. Not its tracking (letterSpacing): the TV's NUI has that only
/// as CharacterSpacing, which its API doesn't name; it adds a fraction of a pixel a letter, so
/// text keeps the font's own spacing.
///
/// A line is cut to fit here (<see cref="Fit"/>), not by NUI's ellipsis: on the TV, labels the
/// pages made drew as "…" alone (tile captions, row headers, Live TV's categories), whether
/// their text fitted or not, until something laid them out again.
/// </summary>
static class Text
{
    public static TextLabel Label(string text, TextStyle style, Color color, int maxLines = 1, float width = 0)
    {
        var label = new TextLabel(maxLines == 1 && width > 0 ? Fit(text, style, width) : text)
        {
            PixelSize = Ui.Dp(style.Size),
            TextColor = color,
            MultiLine = maxLines > 1,
            Ellipsis = maxLines > 1,
            FontFamily = style.Family,
            FontStyle = Weight(style.Weight),
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (width > 0) label.Size = new Size(Ui.Dp(width), LineHeight(style) * maxLines);
        return label;
    }

    /// <summary>One measuring label, never on screen: measuring on a fresh label each time costs a label each time.</summary>
    static TextLabel? measurer;

    /// <summary>
    /// <paramref name="text"/> as it fits on one line <paramref name="widthDp"/> wide: as it is,
    /// or as much of it as fits with "…" after.
    /// </summary>
    public static string Fit(string text, TextStyle style, float widthDp) =>
        Fit(text, Ui.Dp(style.Size), style.Family, style.Weight, Ui.Dp(widthDp));

    /// <summary>
    /// Puts <paramref name="text"/> in <paramref name="label"/>, cut to fit its width as
    /// <see cref="Fit(string, TextStyle, float)"/> does: for a one-line label of a set width whose
    /// text changes. A label sized to its text after takes the text as it is.
    /// </summary>
    public static void Show(TextLabel label, string text) =>
        label.Text = label.MultiLine || label.SizeWidth <= 0 ? text : Fit(text, label.PixelSize, label.FontFamily, WeightOf(label), label.SizeWidth);

    /// <summary>
    /// Text as fitted, by font and width: a grid's rows come and go as it scrolls, and a tile's
    /// title is fitted again at each focus, so each is measured once. Emptied when it grows large.
    /// </summary>
    static readonly System.Collections.Generic.Dictionary<string, string> Fitted = new System.Collections.Generic.Dictionary<string, string>();

    const int FittedMost = 4000;

    static string Fit(string text, float pixelSize, string family, string weight, float widthPx)
    {
        if (text.Length == 0) return text;
        var key = $"{pixelSize}|{family}|{weight}|{widthPx}|{text}";
        if (Fitted.TryGetValue(key, out var known)) return known;
        // A pixel or two of room: NaturalSize rounds to the pixel.
        var room = widthPx - 2;
        string fitted;
        var whole = Measure(text, pixelSize, family, weight);
        if (whole <= room) fitted = text;
        else
        {
            // The longest start that fits with "…" after it: from where the width says, a letter or
            // two either way, as each measuring lays the text out (a guide's row has dozens of titles).
            bool Fits(int n) => Measure(text.Substring(0, n).TrimEnd() + "…", pixelSize, family, weight) <= room;
            var n = System.Math.Max(1, System.Math.Min(text.Length - 1, (int)(text.Length * room / whole)));
            if (Fits(n))
            {
                while (n + 1 < text.Length && Fits(n + 1)) n++;
            }
            else
            {
                do n--;
                while (n > 0 && !Fits(n));
            }
            fitted = n == 0 ? "…" : text.Substring(0, n).TrimEnd() + "…";
        }
        if (Fitted.Count >= FittedMost) Fitted.Clear();
        Fitted[key] = fitted;
        return fitted;
    }

    /// <summary>The font <see cref="measurer"/> is set to, so it is set again only when it changes.</summary>
    static (float, string, string) measuring;

    /// <summary>The width in pixels <paramref name="text"/> takes on one line in that font.</summary>
    static float Measure(string text, float pixelSize, string family, string weight)
    {
        measurer ??= new TextLabel();
        if (measuring != (pixelSize, family, weight))
        {
            measuring = (pixelSize, family, weight);
            measurer.PixelSize = pixelSize;
            measurer.FontFamily = family;
            measurer.FontStyle = Weight(weight);
        }
        measurer.Text = text;
        return measurer.NaturalSize.Width;
    }

    /// <summary>The weight a label's font style names ("normal", "medium", "bold").</summary>
    static string WeightOf(TextLabel label)
    {
        using var style = label.FontStyle;
        using var value = style.Find(0, "weight");
        return value != null && value.Get(out string weight) ? weight : PleyType.Regular;
    }

    /// <summary>The fonts the app ships: Roboto, before any text is drawn.</summary>
    public static void LoadFonts() =>
        FontClient.Instance.AddCustomFontDirectory($"{(global::Tizen.Applications.Application.Current.DirectoryInfo.SharedResource)}fonts");

    /// <summary>
    /// The width in dp a label needs for its text on one line. It is measured on a fresh label
    /// of the same font: a label already sized for other text (the player's badge going from
    /// "Direct play" to "Transcoding") can measure as that size's "…". NaturalSize rounds to the
    /// pixel, so this adds a little room.
    /// </summary>
    public static float Width(TextLabel label)
    {
        using var probe = new TextLabel(label.Text ?? "") { PixelSize = label.PixelSize, FontFamily = label.FontFamily, FontStyle = label.FontStyle };
        return probe.NaturalSize.Width / Ui.Scale + 3;
    }

    /// <summary>Each style's line height in its font, in pixels, measured once.</summary>
    static readonly System.Collections.Generic.Dictionary<(float, string, string), float> FontLines = new System.Collections.Generic.Dictionary<(float, string, string), float>();

    /// <summary>
    /// A line of <paramref name="style"/> in pixels: the design's line height, or the font's own on
    /// the TV when that is taller. Text in a box too short for its font shows as "…" alone. Text is
    /// centered in its box, so a pixel or two more doesn't move it.
    /// </summary>
    public static float LineHeight(TextStyle style)
    {
        var key = (style.Size, style.Weight, style.Family);
        if (!FontLines.TryGetValue(key, out var fontLine))
        {
            using var probe = new TextLabel("Ágjy") { PixelSize = Ui.Dp(style.Size), FontFamily = style.Family, FontStyle = Weight(style.Weight) };
            fontLine = (float)System.Math.Ceiling(probe.NaturalSize.Height);
            FontLines[key] = fontLine;
        }
        return System.Math.Max(Ui.Dp(style.Line), fontLine);
    }

    public static PropertyMap Weight(string weight)
    {
        var map = new PropertyMap();
        map.Add("weight", new PropertyValue(weight));
        return map;
    }
}
