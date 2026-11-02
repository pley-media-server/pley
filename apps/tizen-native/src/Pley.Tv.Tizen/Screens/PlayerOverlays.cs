using System;
using System.Collections.Generic;
using Pley.Tv.Core.Player;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/*
 * The player's views (design spec §2.7; the Android app's TvPlayerControls, TvPlayerOverlays and
 * TvPlayerExtras): its buttons, and what shows over the video besides the controls: Skip, Up
 * Next, the sleep timer's notice, the method badge, the seek preview and toasts. The player
 * screen places them and gives them focus.
 */

/// <summary>
/// A button of the player (design spec §2.5): its glyph alone on the scrim; focused, the accent
/// fills a control square behind it, 1.08 larger, and its label fades in 8 dp below.
/// </summary>
sealed class PlayerButton : View
{
    readonly View box;
    readonly float iconSize;
    ImageView icon;
    readonly TextLabel label;

    public PlayerButton(string text, string iconName, float size, float iconSize, Action onClick)
    {
        this.iconSize = iconSize;
        SizeDp = size;
        Size = new Size(Ui.Dp(size), Ui.Dp(size));
        Item = new FocusItem(text) { Selected = onClick };
        box = new View
        {
            Size = new Size(Ui.Dp(size), Ui.Dp(size)),
            CornerRadius = Ui.Dp(PleyShape.ControlRadius),
            PivotPoint = global::Tizen.NUI.PivotPoint.Center,
            PositionUsesPivotPoint = false,
        };
        icon = Glyph(iconName);
        box.Add(icon);
        Add(box);
        label = Text.Label(text, PleyType.Caption, PleyColors.Fg, width: 200);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        label.Position = new Position(Ui.Dp(size / 2 - 100), Ui.Dp(size + 8));
        label.Opacity = 0;
        Add(label);
        Item.FocusChanged += Look;
    }

    public FocusItem Item { get; }

    public float SizeDp { get; }

    ImageView Glyph(string name)
    {
        var glyph = Icons.Icon(name, iconSize, PleyColors.Fg);
        var inset = (SizeWidth - Ui.Dp(iconSize)) / 2;
        glyph.Position = new Position(inset, inset);
        return glyph;
    }

    public void Set(string text, string iconName)
    {
        if (Disposed) return;
        label.Text = text;
        box.Remove(icon);
        icon.Dispose();
        icon = Glyph(iconName);
        icon.Color = Item.Focused ? PleyColors.OnAccent : PleyColors.Fg;
        box.Add(icon);
    }

    void Look(bool focused)
    {
        if (Disposed) return;
        box.BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
        icon.Color = focused ? PleyColors.OnAccent : PleyColors.Fg;
        var scale = focused ? PleyShape.RoundScale : 1f;
        box.Scale = new Vector3(scale, scale, 1);
        label.Opacity = focused ? 1 : 0;
    }
}

/// <summary>Fades a view in or out over <paramref name="ms"/>.</summary>
static class Fade
{
    public static Animation To(View view, bool on, int ms = 150)
    {
        var a = new Animation(ms) { DefaultAlphaFunction = new AlphaFunction(on ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn) };
        a.AnimateTo(view, "Opacity", on ? 1f : 0f);
        a.PlayHeld();
        return a;
    }

    /// <summary>Shows or hides <paramref name="view"/> at once.</summary>
    public static void SetVisible(this View view, bool on)
    {
        if (on) view.Show();
        else view.Hide();
    }
}

/// <summary>
/// Skip Intro / Skip Credits: a quiet button, dark and see-through with a faint outline over the
/// picture; focused, only its outline firms up: no accent fill, no growing, no countdown.
/// </summary>
sealed class SkipButton : View
{
    const float Height = 40, Pad = 18, Glyph = 18;

    readonly TextLabel label;
    readonly ImageView chevrons;

    public SkipButton(Action onSkip)
    {
        Item = new FocusItem("skip") { Selected = onSkip };
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        BorderlineOffset = -1;
        label = Text.Label("", PleyType.Label, PleyColors.Fg);
        Add(label);
        chevrons = Icons.Icon("chevrons-right", Glyph, PleyColors.Fg);
        Add(chevrons);
        Item.FocusChanged += Look;
        Look(false);
    }

    public FocusItem Item { get; }

    public float WidthDp { get; private set; }

    public void SetLabel(string text)
    {
        if (label.Text == text) return;
        label.Text = text;
        var textWidth = Text.Width(label);
        WidthDp = Pad + textWidth + 8 + Glyph + Pad;
        Size = new Size(Ui.Dp(WidthDp), Ui.Dp(Height));
        label.Size = new Size(Ui.Dp(textWidth), Ui.Dp(Height));
        label.Position = new Position(Ui.Dp(Pad), 0);
        chevrons.Position = new Position(Ui.Dp(Pad + textWidth + 8), Ui.Dp((Height - Glyph) / 2));
    }

    void Look(bool focused)
    {
        if (Disposed) return;
        BackgroundColor = new Color(0, 0, 0, focused ? 0.6f : 0.45f);
        BorderlineWidth = Ui.Dp(focused ? 2 : 1);
        BorderlineColor = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, focused ? 0.9f : 0.25f);
        var ink = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, focused ? 1f : 0.85f);
        label.TextColor = ink;
        chevrons.Color = ink;
    }
}

/// <summary>
/// Up Next (design spec §2.7): a 300 dp card on Surface2 at 96 %: the next one's still, "Up next ·
/// S2 · E6" and its title, then Play now, with a ring filling over the countdown ("Next in 7" when
/// motion is reduced), and Cancel.
/// </summary>
sealed class UpNextView : View
{
    public const float Width = 300;
    const float ImageHeight = 169, Pad = 16;

    readonly Images images;
    readonly bool reduced;
    readonly ImageView still;
    readonly TextLabel heading;
    readonly TextLabel name;
    readonly PlayNowButton play;
    readonly ButtonView cancel;

    public UpNextView(Images images, bool reducedMotion, Action onPlay, Action onCancel)
    {
        this.images = images;
        reduced = reducedMotion;
        CornerRadius = Ui.Dp(PleyShape.DialogRadius);
        BackgroundColor = new Color(PleyColors.Surface2.R, PleyColors.Surface2.G, PleyColors.Surface2.B, 0.96f);
        ClippingMode = ClippingModeType.ClipChildren;
        var well = new View { Size = new Size(Ui.Dp(Width), Ui.Dp(ImageHeight)), BackgroundColor = PleyColors.Surface3 };
        Add(well);
        still = new ImageView { Size = new Size(Ui.Dp(Width), Ui.Dp(ImageHeight)), FittingMode = FittingModeType.ScaleToFill, Opacity = 0 };
        Add(still);
        heading = Text.Label("", PleyType.Caption, PleyColors.Muted, width: Width - 2 * Pad);
        heading.Position = new Position(Ui.Dp(Pad), Ui.Dp(ImageHeight + Pad));
        Add(heading);
        name = Text.Label("", PleyType.Title, PleyColors.Fg, maxLines: 2, width: Width - 2 * Pad);
        name.VerticalAlignment = VerticalAlignment.Top;
        name.Position = new Position(Ui.Dp(Pad), Ui.Dp(ImageHeight + Pad + PleyType.Caption.Line));
        Add(name);
        play = new PlayNowButton(onPlay);
        Add(play);
        cancel = new ButtonView(PlayerText.Cancel, ButtonKind.Text, onCancel);
        Add(cancel);
        Group = new FocusGroup(Axis.Row, "up-next");
        Group.Add(play.Item);
        Group.Add(cancel.Item);
        // Up and Down stay on the card.
        Group.Exit = d => d is Direction.Up or Direction.Down;
    }

    public FocusGroup Group { get; }

    public FocusItem PlayNow => play.Item;

    public long? ItemId { get; private set; }

    public float HeightDp { get; private set; }

    /// <summary>Shows <paramref name="card"/>: drawn again for another item, its countdown stepped for the same one.</summary>
    public void Set(UpNextCard card)
    {
        if (Disposed) return;
        play.Countdown(card.SecondsLeft, reduced);
        PlaceCancel();
        if (ItemId == card.ItemId) return;
        ItemId = card.ItemId;
        Text.Show(heading, PlayerText.EpisodeLine(card.Heading) ?? card.Heading);
        name.Text = card.Title;
        var nameHeight = Math.Min(Math.Max(name.GetHeightForWidth(Ui.Dp(Width - 2 * Pad)), Text.LineHeight(PleyType.Title)), Text.LineHeight(PleyType.Title) * 2);
        name.SizeHeight = nameHeight;
        rowTop = ImageHeight + Pad + PleyType.Caption.Line + nameHeight / Ui.Scale + 16;
        play.Position = new Position(Ui.Dp(Pad), Ui.Dp(rowTop));
        PlaceCancel();
        HeightDp = rowTop + PlayNowButton.Height + Pad;
        Size = new Size(Ui.Dp(Width), Ui.Dp(HeightDp));
        still.Opacity = 0;
        if (card.ImageUrl is { } url)
        {
            var asked = card.ItemId;
            images.Load(new[] { url }, Width, file =>
            {
                if (file == null || Disposed || ItemId != asked) return;
                still.ResourceUrl = file;
                Fade.To(still, true, PleyMotion.ImageFadeMs);
            });
        }
    }

    float rowTop;

    /// <summary>
    /// Cancel after Play now, placed again each time Play now's width changes (its countdown's ring
    /// or words come and go), with room for both to grow as focus scales them: placed once, Play
    /// now grew into it.
    /// </summary>
    void PlaceCancel()
    {
        var growth = (PleyShape.PillScale - 1) / 2 * (play.SizeWidth + cancel.SizeWidth);
        cancel.Position = new Position(Ui.Dp(Pad) + play.SizeWidth + Ui.Dp(12) + growth, Ui.Dp(rowTop + (PlayNowButton.Height - PleyTv.ButtonHeight) / 2));
    }

    /// <summary>"Play now", focused when the card shows, with the countdown's ring (or words) inside it.</summary>
    sealed class PlayNowButton : View
    {
        public const float Height = 48;
        const float Icon = 22, Ring = 22, Stroke = 3;

        readonly ImageView icon;
        readonly TextLabel label;
        readonly TextLabel words;
        readonly View ringTrack;
        readonly View ringDone;
        int? left;
        bool wordsOnly;

        public PlayNowButton(Action onPlay)
        {
            Item = new FocusItem(PlayerText.PlayNow) { Selected = onPlay };
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            BorderlineOffset = -1;
            PivotPoint = global::Tizen.NUI.PivotPoint.Center;
            PositionUsesPivotPoint = false;
            icon = Icons.Icon("play", Icon, PleyColors.Fg);
            icon.Position = new Position(Ui.Dp(20), Ui.Dp((Height - Icon) / 2));
            Add(icon);
            label = Text.Label(PlayerText.PlayNow, PleyType.Label, PleyColors.Fg);
            var labelWidth = Text.Width(label);
            label.Size = new Size(Ui.Dp(labelWidth), Ui.Dp(Height));
            label.Position = new Position(Ui.Dp(20 + Icon + 10), 0);
            Add(label);
            LabelEnd = 20 + Icon + 10 + labelWidth;
            words = Text.Label("", PleyType.Caption, PleyColors.Fg);
            words.Position = new Position(Ui.Dp(LabelEnd + 12), 0);
            Add(words);
            ringTrack = new View { Size = new Size(Ui.Dp(Ring), Ui.Dp(Ring)), Position = new Position(Ui.Dp(LabelEnd + 12), Ui.Dp((Height - Ring) / 2)) };
            Add(ringTrack);
            ringDone = new View { Size = ringTrack.Size, Position = ringTrack.Position };
            Add(ringDone);
            Item.FocusChanged += _ => Look();
            Countdown(null, false);
        }

        public FocusItem Item { get; }

        float LabelEnd { get; }

        public void Countdown(int? secondsLeft, bool reduced)
        {
            if (Disposed) return;
            left = secondsLeft;
            wordsOnly = reduced;
            var extra = 0f;
            if (secondsLeft is int n)
            {
                if (reduced)
                {
                    words.Text = PlayerText.NextIn(n);
                    var w = Text.Width(words);
                    words.Size = new Size(Ui.Dp(w), Ui.Dp(Height));
                    extra = 12 + w;
                }
                else extra = 12 + Ring;
            }
            words.SetVisible(secondsLeft != null && reduced);
            ringTrack.SetVisible(secondsLeft != null && !reduced);
            ringDone.SetVisible(secondsLeft != null && !reduced);
            Size = new Size(Ui.Dp(LabelEnd + extra + 16), Ui.Dp(Height));
            Look();
        }

        void Look()
        {
            if (Disposed) return;
            var focused = Item.Focused;
            var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
            BackgroundColor = focused ? PleyColors.Accent : new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.12f);
            BorderlineWidth = focused ? 0 : Ui.Dp(1);
            BorderlineColor = PleyColors.Control;
            icon.Color = ink;
            label.TextColor = ink;
            words.TextColor = ink;
            var scale = focused ? PleyShape.PillScale : 1f;
            Scale = new Vector3(scale, scale, 1);
            if (left is int n && !wordsOnly)
            {
                ringTrack.Background = Arc(new Color(ink.R, ink.G, ink.B, 0.25f), 360, false);
                var done = Math.Max(0f, Math.Min(1f, (NextUp.CountdownSeconds - n) / (float)NextUp.CountdownSeconds));
                ringDone.Background = Arc(ink, 360 * done, true);
            }
        }

        /// <summary>An arc from the top, clockwise, <paramref name="sweep"/> degrees long.</summary>
        static PropertyMap Arc(Color color, float sweep, bool round) =>
            new ArcVisual { Thickness = Ui.Dp(Stroke), StartAngle = -90, SweepAngle = sweep, Cap = round ? ArcVisual.CapType.Round : ArcVisual.CapType.Butt, MixColor = color }.OutputVisualMap;
    }
}

/// <summary>
/// The sleep timer's notice, 30 s before it pauses: "Pausing in 28 s" and Keep watching, at the
/// bottom left (Skip and Up Next keep the right). It takes focus while the controls are hidden.
/// </summary>
sealed class SleepNoticeView : View
{
    const float Glyph = 20;

    readonly TextLabel text;
    readonly ButtonView keep;

    public SleepNoticeView(Action onKeep)
    {
        BackgroundColor = PleyColors.Surface3;
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        var moon = Icons.Icon("moon", Glyph, PleyColors.Muted);
        moon.Position = new Position(Ui.Dp(20), Ui.Dp(8 + (PleyTv.ButtonHeight - Glyph) / 2));
        Add(moon);
        text = Text.Label("", PleyType.Label, PleyColors.Fg);
        text.Position = new Position(Ui.Dp(20 + Glyph + 16), Ui.Dp(8));
        Add(text);
        keep = new ButtonView(SleepText.Keep(), ButtonKind.Pill, onKeep);
        Add(keep);
        Group = new FocusGroup(Axis.Row, "sleep-notice");
        Group.Add(keep.Item);
        Set(30);
    }

    public FocusGroup Group { get; }

    public FocusItem Keep => keep.Item;

    public float HeightDp => 8 + PleyTv.ButtonHeight + 8;

    int shownSeconds = -1;

    public void Set(int secondsLeft)
    {
        if (Disposed || secondsLeft == shownSeconds) return;
        shownSeconds = secondsLeft;
        // As wide as its longest count, so it doesn't shrink as the seconds go.
        text.Text = SleepText.Notice(30);
        var w = Text.Width(text);
        text.Text = SleepText.Notice(secondsLeft);
        text.Size = new Size(Ui.Dp(w), Ui.Dp(PleyTv.ButtonHeight));
        keep.Position = new Position(Ui.Dp(20 + Glyph + 16 + w + 16), Ui.Dp(8));
        Size = new Size(Ui.Dp(20 + Glyph + 16 + w + 16) + keep.SizeWidth + Ui.Dp(8), Ui.Dp(HeightDp));
    }
}

/// <summary>A toast at the top right (design spec §2.10): Surface3, Label in Fg (errors in DangerText), three lines at most, 420 dp wide at most.</summary>
sealed class ToastView : View
{
    const float MaxWidth = 420, PadX = 20, PadY = 10;

    readonly TextLabel text;

    public ToastView()
    {
        BackgroundColor = PleyColors.Surface3;
        CornerRadius = Ui.Dp(PleyShape.ControlRadius);
        text = Text.Label("", PleyType.Label, PleyColors.Fg, maxLines: 3);
        text.VerticalAlignment = VerticalAlignment.Top;
        text.Position = new Position(Ui.Dp(PadX), Ui.Dp(PadY));
        Add(text);
    }

    public float WidthDp { get; private set; }

    public void Set(PlayerToast toast)
    {
        text.Text = toast.Text;
        text.TextColor = toast.Error ? PleyColors.DangerText : PleyColors.Fg;
        var inner = MaxWidth - 2 * PadX;
        var w = Math.Min(inner, Text.Width(text));
        var h = Math.Min(Math.Max(text.GetHeightForWidth(Ui.Dp(w)), Text.LineHeight(PleyType.Label)), Text.LineHeight(PleyType.Label) * 3);
        text.Size = new Size(Ui.Dp(w), h);
        WidthDp = w + 2 * PadX;
        Size = new Size(Ui.Dp(WidthDp), h + Ui.Dp(2 * PadY));
    }
}

/// <summary>
/// How it plays, badged as the phone's and the web's players do: a file played as it is in
/// Success, a direct stream in Info's blue, a transcode in the accent, on Bg at 80 %.
/// </summary>
sealed class MethodBadge : View
{
    static readonly Color Info = new Color(0x4C / 255f, 0x9A / 255f, 1f, 1f);

    readonly View tint = new View();
    readonly TextLabel text;

    public MethodBadge()
    {
        BackgroundColor = PleyColors.OnArt;
        CornerRadius = Ui.Dp(PleyShape.BadgeRadius);
        tint.CornerRadius = Ui.Dp(PleyShape.BadgeRadius);
        Add(tint);
        text = Text.Label("", PleyType.Caption, PleyColors.Fg);
        // The label has a little room past its text (Text.Width): centred, the text stays in the badge's middle.
        text.HorizontalAlignment = HorizontalAlignment.Center;
        text.Position = new Position(Ui.Dp(8), Ui.Dp(3));
        Add(text);
    }

    public float WidthDp { get; private set; }

    public void Set(string method)
    {
        if (text.Text == method) return;
        var colour = method == SessionText.MethodDirect ? PleyColors.Success
            : method.StartsWith(SessionText.MethodRemux, StringComparison.Ordinal) ? Info
            : PleyColors.Accent;
        text.Text = method;
        text.TextColor = colour;
        var w = Text.Width(text);
        text.Size = new Size(Ui.Dp(w), Text.LineHeight(PleyType.Caption));
        WidthDp = w + 16;
        Size = new Size(Ui.Dp(WidthDp), Text.LineHeight(PleyType.Caption) + Ui.Dp(6));
        tint.Size = Size;
        tint.BackgroundColor = new Color(colour.R, colour.G, colour.B, 0.2f);
    }
}

/// <summary>
/// A video's frame in a box of another shape (the seek preview, a chapter's card), as the player
/// shows the video: as large as fits, centred, black where it doesn't reach; never stretched to the
/// box. The server's previews keep the video's shape (a 2.39:1 film's are 320 × 134).
/// </summary>
static class Letterbox
{
    /// <summary>
    /// Shows <paramref name="picture"/> as <paramref name="crop"/>, the frame's part of its sheet
    /// (null: the whole picture, its shape known once it has loaded), in <paramref name="box"/>.
    /// </summary>
    public static void Show(ImageView picture, View box, string file, TrickplayFrame? crop)
    {
        box.BackgroundColor = Color.Black;
        var loaded = picture.ResourceUrl == file && picture.NaturalSize2D.Height > 0;
        picture.ResourceUrl = file;
        picture.PixelArea = crop == null
            ? new RelativeVector4(0, 0, 1, 1)
            : new RelativeVector4(crop.Left / (float)crop.SheetWidth, crop.Top / (float)crop.SheetHeight, crop.Width / (float)crop.SheetWidth, crop.Height / (float)crop.SheetHeight);
        if (crop != null) Fit(picture, box, crop.Width / (float)crop.Height);
        else if (loaded) Fit(picture, box, picture.NaturalSize2D.Width / (float)picture.NaturalSize2D.Height);
        else
        {
            // Its shape is the picture's own, once it's read.
            Fit(picture, box, 0);
            void Ready(object? s, EventArgs e)
            {
                picture.ResourceReady -= Ready;
                if (picture.ResourceUrl != file) return;
                var natural = picture.NaturalSize2D;
                Fit(picture, box, natural.Height > 0 ? natural.Width / (float)natural.Height : 0);
            }
            picture.ResourceReady += Ready;
        }
    }

    /// <summary><paramref name="picture"/> at <paramref name="aspect"/> (width over height) in <paramref name="box"/>, centred; 0 fills it.</summary>
    static void Fit(View picture, View box, float aspect)
    {
        float w = box.SizeWidth, h = box.SizeHeight;
        if (aspect > 0)
        {
            if (w / aspect <= h) h = w / aspect;
            else w = h * aspect;
        }
        picture.Size = new Size(w, h);
        picture.Position = new Position((box.SizeWidth - w) / 2, (box.SizeHeight - h) / 2);
    }
}

/// <summary>
/// The seek preview while scrubbing (design spec §2.7): 240 × 135 with a 2 dp Fg 20 % edge, the
/// server's preview for the target (a part of one of its sheets) or the chapter's picture; the
/// target time under it and the chapter's name. Without a picture, only the time and chapter show.
/// </summary>
sealed class SeekPreviewView : View
{
    public const float PictureWidth = 240, PictureHeight = 135;

    readonly Images images;
    readonly View frame;
    readonly ImageView picture;
    readonly TextLabel time;
    readonly TextLabel chapter;
    /// <summary>Sheets fetched, by address: the next preview on the same sheet shows at once.</summary>
    readonly Dictionary<string, string> sheets = new Dictionary<string, string>();
    string? asked;

    public SeekPreviewView(Images images)
    {
        this.images = images;
        Size = new Size(Ui.Dp(PictureWidth), Ui.Dp(PictureHeight + 6 + PleyType.Label.Line + PleyType.Caption.Line));
        // Not clipped: the TV clips children to a rectangle, whatever the frame's corners, so a
        // picture filling the frame showed its square corners past the rounded edge. It rounds its
        // own, as a chapter card's does.
        frame = new View
        {
            Size = new Size(Ui.Dp(PictureWidth), Ui.Dp(PictureHeight)),
            CornerRadius = Ui.Dp(PleyShape.TileRadius),
            BackgroundColor = PleyColors.Surface2,
        };
        picture = new ImageView { Size = frame.Size, CornerRadius = Ui.Dp(PleyShape.TileRadius), FittingMode = FittingModeType.ScaleToFill };
        frame.Add(picture);
        frame.Add(new View
        {
            Size = frame.Size,
            CornerRadius = Ui.Dp(PleyShape.TileRadius),
            BorderlineWidth = Ui.Dp(2),
            BorderlineOffset = -1,
            BorderlineColor = new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.2f),
            BackgroundColor = Color.Transparent,
        });
        Add(frame);
        time = Text.Label("", PleyType.Label, PleyColors.Fg, width: PictureWidth);
        time.HorizontalAlignment = HorizontalAlignment.Center;
        Add(time);
        chapter = Text.Label("", PleyType.Caption, PleyColors.Muted, width: PictureWidth);
        chapter.HorizontalAlignment = HorizontalAlignment.Center;
        Add(chapter);
    }

    /// <summary>Whether a picture shows above the time: the server's previews, or the chapter's picture.</summary>
    public bool HasPicture { get; private set; }

    /// <summary>
    /// The preview for <paramref name="targetMs"/>: <paramref name="frameAt"/> the server's preview
    /// (null without), else <paramref name="chapterImage"/>; <paramref name="chapterLine"/> under the time.
    /// </summary>
    public void Set(long targetMs, TrickplayFrame? frameAt, string? chapterImage, string? chapterLine)
    {
        if (Disposed) return;
        time.Text = PlayerText.Time(targetMs);
        Text.Show(chapter, chapterLine ?? "");
        HasPicture = frameAt != null || chapterImage != null;
        frame.SetVisible(HasPicture);
        var under = HasPicture ? PictureHeight + 6 : 0;
        time.PositionY = Ui.Dp(under);
        chapter.PositionY = Ui.Dp(under + PleyType.Label.Line);
        if (frameAt != null) Show(frameAt.SheetUrl, frameAt);
        else if (chapterImage != null) Show(chapterImage, null);
    }

    /// <summary>The picture goes (the player going): a texture goes with its view only when it's collected.</summary>
    public void Forget()
    {
        if (Disposed) return;
        asked = null;
        picture.ResourceUrl = "";
    }

    void Show(string url, TrickplayFrame? crop)
    {
        void Apply(string file) => Letterbox.Show(picture, frame, file, crop);
        asked = url;
        if (sheets.TryGetValue(url, out var known))
        {
            Apply(known);
            return;
        }
        // The sheet comes with the token, through the app's pictures.
        images.Load(new[] { url }, crop != null ? crop.SheetWidth / Ui.Scale : PictureWidth, file =>
        {
            if (file == null || Disposed) return;
            sheets[url] = file;
            if (asked == url) Apply(file);
        });
    }
}
