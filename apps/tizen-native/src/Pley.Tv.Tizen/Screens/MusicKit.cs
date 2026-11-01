using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>What the music pages share (the Android app's TvMusicKit.kt).</summary>
static class MusicKit
{
    /// <summary>Under a row of buttons, the room their labels fade into.</summary>
    public const float ButtonLabelRoom = 24;

    /// <summary>A failed page's title: offline, else the page's own ("Couldn't load this album").</summary>
    public static string FailedTitle(Exception e, string title) => Errors.Unreachable(e) ? Errors.OfflineTitle : title;

    /// <summary>
    /// A page-sized message with one action that has focus (not found, failed, nothing playing), as
    /// the Android app's MusicMessage: at the gutter from y = 60, the title in Title, the sentence in
    /// Body Muted, then the pill, 10 dp apart and 12 more before the pill.
    /// </summary>
    public static ButtonRow Message(View into, string title, string body, string action, Action onAction)
    {
        const float width = PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal;
        float y = 60;
        var heading = Text.Label(title, PleyType.Title, PleyColors.Fg, width: width);
        heading.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        into.Add(heading);
        y += PleyType.Title.Line + 10;
        var words = PanelColumn.Lines(body, PleyType.Body, PleyColors.Muted, width, 3);
        words.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        into.Add(words);
        y += words.SizeHeight / Ui.Scale + 10 + 12 + 10;
        var row = new ButtonRow();
        row.Add(new ButtonView(action, ButtonKind.Pill, onAction));
        row.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
        into.Add(row);
        return row;
    }

    /// <summary>A still block of a skeleton, in dp.</summary>
    public static View Block(float x, float y, float w, float h, float radius = PleyShape.BadgeRadius) => new View
    {
        Position = new Position(Ui.Dp(x), Ui.Dp(y)),
        Size = new Size(Ui.Dp(w), Ui.Dp(h)),
        CornerRadius = Ui.Dp(radius),
        BackgroundColor = PleyColors.Surface2,
    };

    /// <summary>Empties <paramref name="view"/>, letting go of what it held.</summary>
    public static void Empty(View view)
    {
        foreach (var child in view.Children.ToList())
        {
            view.Remove(child);
            child.Dispose();
        }
    }

    /// <summary>The profile's rating as five 16 dp stars, 2 apart: filled in the accent, else Surface4 (the Android app's Stars).</summary>
    public static View Stars(int rating)
    {
        const float size = 16, gap = 2;
        var row = new View { Size = new Size(Ui.Dp(size * 5 + gap * 4), Ui.Dp(size)) };
        var x = 0f;
        foreach (var filled in MusicPages.Stars(rating))
        {
            var star = Icons.Icon("star", size, filled ? PleyColors.Accent : PleyColors.Surface4);
            star.PositionX = Ui.Dp(x);
            row.Add(star);
            x += size + gap;
        }
        return row;
    }
}

/// <summary>
/// Art in a well (an album's, the artist's picture): Surface2 with the given corners, the picture
/// fading in over it once it has come; nothing more without one.
/// </summary>
sealed class ArtWell : View
{
    readonly float size;

    public ArtWell(float sizeDp, float radiusDp, Images images, string? url)
    {
        size = sizeDp;
        Size = new Size(Ui.Dp(sizeDp), Ui.Dp(sizeDp));
        CornerRadius = Ui.Dp(radiusDp);
        ClippingMode = ClippingModeType.ClipChildren;
        BackgroundColor = PleyColors.Surface2;
        if (url != null) images.Load(new[] { url }, sizeDp, Picture);
    }

    void Picture(string? file)
    {
        // Gone already: the page moved on before the picture came.
        if (file == null || Disposed) return;
        var px = Ui.Dp(size);
        var picture = new ImageView { ResourceUrl = file, Size = new Size(px, px), DesiredWidth = (int)px, DesiredHeight = (int)px, FittingMode = FittingModeType.ScaleToFill, Opacity = 0 };
        Add(picture);
        var fade = new Animation(PleyMotion.ImageFadeMs);
        fade.AnimateTo(picture, "Opacity", 1f);
        fade.Play();
    }
}

/// <summary>
/// The album art over the whole page, faint, unblurred (design spec §4.10, §6; the Android app's
/// FaintArt): at <c>alpha</c> on Bg, fading into the page from 40 % of the way down, so text stays
/// on solid ground. A new picture crossfades in once it has loaded.
/// </summary>
sealed class FaintArtView : View
{
    readonly Images images;
    readonly float alpha;
    readonly View pictures = new View();
    ImageView? shown;
    string? url;
    int asked;

    public FaintArtView(Images images, float alpha)
    {
        this.images = images;
        this.alpha = alpha;
        var size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        Size = size;
        pictures.Size = size;
        Add(pictures);
        Add(new View { Size = size, Background = Gradients.Vertical(new Color(PleyColors.Bg.R, PleyColors.Bg.G, PleyColors.Bg.B, 0), PleyColors.Bg, 0.4f) });
    }

    public void Show(string? next)
    {
        if (next == url) return;
        url = next;
        var mine = ++asked;
        if (next == null)
        {
            Swap(null);
            return;
        }
        images.Load(new[] { next }, BackdropView.WidthDp, file =>
        {
            if (mine != asked || Disposed) return;
            Swap(file);
        });
    }

    void Swap(string? file)
    {
        var before = shown;
        shown = null;
        if (file != null)
        {
            shown = new ImageView { ResourceUrl = file, Size = Size, FittingMode = FittingModeType.ScaleToFill, Opacity = 0 };
            pictures.Add(shown);
        }
        var fade = new Animation(PleyMotion.BackdropMs);
        if (shown != null) fade.AnimateTo(shown, "Opacity", alpha);
        if (before != null) fade.AnimateTo(before, "Opacity", 0f);
        fade.Finished += (s, e) =>
        {
            if (before == null) return;
            before.Unparent();
            before.Dispose();
        };
        fade.Play();
    }
}

/// <summary>
/// Play (a pill with its glyph), Shuffle and "Now playing" (round), 10 dp apart: the music pages'
/// buttons. Now playing shows while music of the page's plays; hidden, focus can't land on it.
/// </summary>
sealed class MusicButtons : View
{
    readonly Func<FocusItem, bool> focus;
    readonly ButtonView play;
    readonly RoundButton shuffle;
    readonly RoundButton now;

    public MusicButtons(Action onPlay, Action onShuffle, Action onNowPlaying, Func<FocusItem, bool> focus, bool onArt)
    {
        this.focus = focus;
        Group = new FocusGroup(Axis.Row, "music-buttons");
        play = new ButtonView(MusicText.Play, ButtonKind.Pill, onPlay, iconName: "play");
        shuffle = new RoundButton(MusicText.Shuffle, "shuffle", onShuffle, onArt);
        now = new RoundButton(MusicText.NowPlaying, "audio-lines", onNowPlaying, onArt);
        var x = 0f;
        foreach (var (view, item) in new (View, FocusItem)[] { (play, play.Item), (shuffle, shuffle.Item), (now, now.Item) })
        {
            view.PositionX = Ui.Dp(x);
            Add(view);
            Group.Add(item);
            x += view.SizeWidth / Ui.Scale + PleyTv.ButtonGap;
        }
        WidthDp = x - PleyTv.ButtonGap;
        Size = new Size(Ui.Dp(WidthDp), Ui.Dp(PleyTv.ButtonHeight));
        ShowNow(false);
    }

    public FocusGroup Group { get; }

    public FocusItem Play => play.Item;

    /// <summary>The row's width in dp with Now playing.</summary>
    public float WidthDp { get; }

    /// <summary>Where the row ends in dp, as it shows now.</summary>
    public float EndDp => (now.Visibility ? now.PositionX + now.SizeWidth : shuffle.PositionX + shuffle.SizeWidth) / Ui.Scale;

    public void Set(bool playEnabled, bool shuffleEnabled, bool nowPlaying)
    {
        play.Enabled = playEnabled;
        shuffle.Enabled = shuffleEnabled;
        ShowNow(nowPlaying);
    }

    void ShowNow(bool on)
    {
        if (now.Item.Shown == on && now.Visibility == on) return;
        var had = now.Item.Focused;
        now.Item.Shown = on;
        if (on) now.Show();
        else now.Hide();
        // Focus was on Now playing as the music stopped: the button before it.
        if (had && !on && !focus(shuffle.Item)) focus(play.Item);
    }
}
