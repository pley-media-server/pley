using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// A tile on TV (the Android app's TvTile and ArtworkTile, design spec §2.3, D-pad rule 8): the
/// well with its picture, or the title on a placeholder until the picture comes, and its overlays;
/// a caption under it. Focused, the well grows to 1.08 and gets the accent ring around it, and the
/// caption turns bold. While a key repeats its focus look changes evenly over the time between moves.
/// </summary>
sealed class TileView : View
{
    readonly TileKind kind;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly float w, h, radius;
    readonly View frame;
    readonly View ring;
    readonly View well;
    readonly ImageView picture;
    readonly TextLabel? placeholder;
    /// <summary>The progress bar, the watched tick or count and the badges, made again for each tile shown.</summary>
    readonly View overlays;
    readonly TextLabel title;
    readonly TextLabel subtitle;
    FocusItem focus;
    /// <summary>The whole title, fitted again to the caption's width in the weight it shows (bold while focused is wider).</summary>
    string titleText = "";
    Animation? focusAnimation;
    bool pictureAsked;
    /// <summary>Which tile it shows, counted up at each: a picture asked for an earlier one is dropped.</summary>
    int shown;

    public TileView(TileModel model, TileKind kind, FocusItem focus, KeyRepeatGate gate, Images images)
    {
        this.kind = kind;
        this.images = images;
        this.gate = gate;
        this.focus = focus;
        Model = model;
        w = Ui.Dp(kind.Width);
        h = Ui.Dp(kind.Height);
        radius = kind.Round ? w / 2 : Ui.Dp(PleyShape.TileRadius);
        Size = new Size(w, h + Ui.Dp(PleyTv.TilesToCaption + PleyTv.CaptionHeight));

        // The well and its ring scale together, about the well's centre.
        frame = new View
        {
            Size = new Size(w, h),
            PivotPoint = global::Tizen.NUI.PivotPoint.Center,
            PositionUsesPivotPoint = false,
        };
        // The ring: a 3 dp stroke centred 4.5 dp outside the well, so 3 dp clear of it. Hidden at
        // rest rather than drawn clear: a grid has thirty tiles on screen.
        var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        ring = new View
        {
            Size = new Size(w + 2 * reach, h + 2 * reach),
            Position = new Position(-reach, -reach),
            BackgroundColor = Color.Transparent,
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1f,
            CornerRadius = kind.Round ? (w + 2 * reach) / 2 : radius + reach,
            Opacity = 0,
        };
        ring.Hide();
        frame.Add(ring);
        // Until the picture comes, and when there is none: the title's own gradient with the title on it
        // (the Android app's Placeholder); people's round tiles have no title on theirs. The well
        // doesn't clip: each clip is a pass of its own for the TV's GPU, and the picture rounds its
        // own corners, as the progress bar does.
        well = new View { Size = new Size(w, h), CornerRadius = radius };
        frame.Add(well);
        if (!kind.Round)
        {
            placeholder = Text.Label("", PleyType.Title, new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.7f), maxLines: 4);
            placeholder.Size = new Size(w - Ui.Dp(24), h - Ui.Dp(24));
            placeholder.Position = new Position(Ui.Dp(12), Ui.Dp(12));
            placeholder.HorizontalAlignment = HorizontalAlignment.Center;
            placeholder.VerticalAlignment = VerticalAlignment.Center;
            well.Add(placeholder);
        }
        picture = new ImageView
        {
            Size = new Size(w, h),
            CornerRadius = radius,
            // Decoded at the size it's drawn, not the file's.
            DesiredWidth = (int)w,
            DesiredHeight = (int)h,
            FittingMode = FittingModeType.ScaleToFill,
        };
        well.Add(picture);
        overlays = new View { Size = new Size(w, h) };
        well.Add(overlays);
        Add(frame);

        var captionTop = h + Ui.Dp(PleyTv.TilesToCaption);
        title = Text.Label("", PleyType.Caption, PleyColors.Fg, width: kind.Width);
        title.Position = new Position(0, captionTop);
        subtitle = Text.Label("", PleyType.Caption, PleyColors.Muted, width: kind.Width);
        subtitle.Position = new Position(0, captionTop + Ui.Dp(PleyType.Caption.Line));
        if (kind.Round)
        {
            title.HorizontalAlignment = HorizontalAlignment.Center;
            subtitle.HorizontalAlignment = HorizontalAlignment.Center;
        }
        Add(title);
        Add(subtitle);

        Fill(model);
        focus.FocusChanged += OnFocus;
    }

    public TileModel Model { get; private set; }
    public FocusItem Focus => focus;

    /// <summary>
    /// Shows another tile in this one, with the focus item of its place: a grid reuses its tiles as
    /// rows scroll by rather than make new ones, which is most of what a row costs.
    /// </summary>
    public void Bind(TileModel model, FocusItem item)
    {
        if (!ReferenceEquals(item, focus))
        {
            focus.FocusChanged -= OnFocus;
            focus = item;
            focus.FocusChanged += OnFocus;
        }
        focusAnimation?.Stop();
        quiet = false;
        Fill(model);
        Sync();
    }

    /// <summary>The tile's own parts: placeholder, overlays, caption; its picture is asked for again.</summary>
    void Fill(TileModel model)
    {
        Model = model;
        var key = PictureKey(model);
        // The same picture, shown: it stays, as the page draws the tile again with new data (back
        // from a film, a resume point moved).
        if (key != null && key == shownKey)
        {
            pictureAsked = true;
        }
        else
        {
            shown++;
            shownKey = null;
            pictureAsked = false;
            picture.Opacity = 0;
            picture.ResourceUrl = "";
            // A picture shown before comes back at once: no title and gradient under it to flash by.
            // A person's circle is Surface2, as on Android (the gradient is for a title, which it has
            // none of): the TV's gradient visual stays square whatever its corner radius, and drew a
            // dark square behind each round picture. A colour takes the well's.
            var known = key != null && Seen.Contains(key);
            if (kind.Round || known) well.BackgroundColor = PleyColors.Surface2;
            else
            {
                var (from, to) = Placeholder.Colors(model.Title);
                well.Background = Gradients.Diagonal(AvatarView.Rgb(from), AvatarView.Rgb(to));
            }
            if (placeholder != null) placeholder.Text = known ? "" : model.Title;
        }
        foreach (var child in overlays.Children.ToList())
        {
            overlays.Remove(child);
            child.Dispose();
        }
        Overlays(overlays, model, w, h);
        titleText = model.Title;
        title.Text = Text.Fit(titleText, PleyType.Caption with { Weight = focus.Focused ? PleyType.Bold : PleyType.Caption.Weight }, kind.Width);
        subtitle.Text = Text.Fit(model.Subtitle, PleyType.Caption, kind.Width);
    }

    /// <summary>The pictures this app has shown, by their addresses: shown again, they come at once.</summary>
    static readonly HashSet<string> Seen = new HashSet<string>();

    /// <summary>Emptied when it grows this large.</summary>
    const int SeenMost = 5000;

    /// <summary>The picture's fade in, while it runs.</summary>
    Animation? fadeIn;

    /// <summary>The picture shown now, by its addresses; null while none is.</summary>
    string? shownKey;

    static string? PictureKey(TileModel model) => model.Images.Count == 0 ? null : string.Join("|", model.Images);

    /// <summary>Asks for the picture once the tile is near the screen; it fades in over the placeholder.</summary>
    public void LoadPicture()
    {
        if (pictureAsked || Model.Images.Count == 0) return;
        pictureAsked = true;
        picture.Opacity = 0;
        var mine = shown;
        images.Load(Model.Images, kind.Width, file =>
        {
            // Gone, or showing another tile by now.
            if (file == null || Disposed || mine != shown) return;
            picture.ResourceUrl = file;
            var key = PictureKey(Model);
            shownKey = key;
            if (key != null && Seen.Contains(key))
            {
                // Shown before: at once, not faded in again.
                picture.Opacity = 1;
                well.BackgroundColor = PleyColors.Surface2;
                if (placeholder != null) placeholder.Text = "";
                return;
            }
            if (key != null)
            {
                if (Seen.Count >= SeenMost) Seen.Clear();
                Seen.Add(key);
            }
            // Held while it runs, as the player's timers are: held by nothing, it could be collected mid-fade.
            fadeIn?.Stop();
            var fade = fadeIn = new Animation(PleyMotion.ImageFadeMs);
            fade.AnimateTo(picture, "Opacity", 1f);
            // Once the picture is in, the well under it is Surface2, as on Android: a picture that
            // doesn't cover the box (a logo with clear parts) shows no gradient through it.
            fade.Finished += (s, e) =>
            {
                if (!Disposed && mine == shown) well.BackgroundColor = PleyColors.Surface2;
            };
            fade.PlayHeld();
        });
    }

    /// <summary>
    /// While its row slides under a held key, the row draws one ring that stays still and the tiles
    /// keep their resting look; set back, the tile shows its focus again.
    /// </summary>
    public bool Quiet
    {
        get => quiet;
        set
        {
            if (quiet == value) return;
            quiet = value;
            if (!value) Look(Focus.Focused, instant: false);
            else if (Focus.Focused) Look(false, instant: true);
        }
    }

    bool quiet;

    /// <summary>Shows focus as it is now, at once: a tile made in place of another that had it (a grid's page loaded).</summary>
    public void Sync() => Look(Focus.Focused, instant: true);

    /// <summary>A grid reuses its positions' focus items for new tiles: this one stops listening to its own.</summary>
    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        Focus.FocusChanged -= OnFocus;
        // Only when disposed by the app: collected, the animation may have gone first (Kit/Held.cs).
        if (type == DisposeTypes.Explicit) focusAnimation?.Stop();
        base.Dispose(type);
    }

    void OnFocus(bool on)
    {
        if (!quiet) Look(on, instant: false);
    }

    void Look(bool on, bool instant)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        // Drawn over its neighbours and their captions while focused.
        if (on) RaiseToTop();
        var weight = on ? PleyType.Bold : PleyType.Caption.Weight;
        title.FontStyle = Text.Weight(weight);
        title.Text = Text.Fit(titleText, PleyType.Caption with { Weight = weight }, kind.Width);
        var scale = on ? (kind.Round ? PleyShape.AvatarScale : PleyShape.TileScale) : 1f;
        focusAnimation?.Stop();
        // The ring shows while focused, and is hidden again once it has faded.
        if (on) ring.Show();
        if (instant)
        {
            frame.Scale = new Vector3(scale, scale, 1);
            ring.Opacity = on ? 1 : 0;
            if (!on) ring.Hide();
            return;
        }
        // A held key moves focus every 60 ms: each change lasts until the next has begun, evenly, so focus glides.
        var held = gate.Holding;
        focusAnimation = new Animation(held ? gate.GlideMs : on ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
        {
            DefaultAlphaFunction = new AlphaFunction(held ? AlphaFunction.BuiltinFunctions.Linear : on ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
        };
        focusAnimation.AnimateTo(frame, "Scale", new Vector3(scale, scale, 1));
        focusAnimation.AnimateTo(ring, "Opacity", on ? 1f : 0f);
        if (!on) focusAnimation.Finished += (s, e) =>
        {
            if (!Disposed && !Focus.Focused) ring.Hide();
        };
        Profile.Slowed(focusAnimation);
        focusAnimation.Play();
    }

    /// <summary>The progress bar along the bottom, the watched tick or unwatched count top right, badges bottom left.</summary>
    static void Overlays(View well, TileModel model, float w, float h)
    {
        var inset = Ui.Dp(6);
        var bar = model.Progress > 0 ? Ui.Dp(4) : 0;
        if (bar > 0)
        {
            // The well doesn't clip: the bar takes the well's bottom corners itself (top left, top right, bottom right, bottom left).
            var corner = Ui.Dp(PleyShape.TileRadius);
            var played = Math.Min(1f, model.Progress);
            well.Add(new View { Size = new Size(w, bar), Position = new Position(0, h - bar), BackgroundColor = PleyColors.ProgressTrackOnArt, CornerRadius = new Vector4(0, 0, corner, corner) });
            well.Add(new View { Size = new Size(w * played, bar), Position = new Position(0, h - bar), BackgroundColor = PleyColors.Accent, CornerRadius = new Vector4(0, 0, played >= 1f ? corner : 0, corner) });
        }
        // The count, tick and badges as the Android app draws them (TileOverlays): 2 dp corners, the
        // count on the accent, the tick an accent check on OnArt.
        var round = Ui.Dp(PleyShape.BadgeRadius);
        if (model.Unwatched > 0)
        {
            var count = Text.Label(model.Unwatched.ToString(System.Globalization.CultureInfo.InvariantCulture), PleyType.Badge, PleyColors.OnAccent);
            var pill = new View
            {
                Size = new Size(Math.Max(Ui.Dp(20), Ui.Dp(12) + count.NaturalSize2D.Width), Ui.Dp(18)),
                BackgroundColor = PleyColors.Accent,
                CornerRadius = round,
            };
            pill.Position = new Position(w - inset - pill.SizeWidth, inset);
            count.Size = new Size(pill.SizeWidth, pill.SizeHeight);
            count.HorizontalAlignment = HorizontalAlignment.Center;
            pill.Add(count);
            well.Add(pill);
        }
        else if (model.Watched)
        {
            var tick = new View
            {
                Size = new Size(Ui.Dp(20), Ui.Dp(20)),
                Position = new Position(w - inset - Ui.Dp(20), inset),
                BackgroundColor = PleyColors.OnArt,
                CornerRadius = round,
            };
            var glyph = Icons.Icon("check", 12, PleyColors.Accent);
            glyph.Position = new Position(Ui.Dp(4), Ui.Dp(4));
            tick.Add(glyph);
            well.Add(tick);
        }
        // Bottom left, 6 dp apart; the well doesn't clip, so a badge that would pass the art's edge is
        // left out, with those after it.
        var x = inset;
        foreach (var badge in model.Badges)
        {
            var text = Text.Label(badge, PleyType.Badge, PleyColors.Fg);
            var width = text.NaturalSize2D.Width + Ui.Dp(12);
            if (x + width > w - inset)
            {
                text.Dispose();
                break;
            }
            var box = new View
            {
                Size = new Size(width, Ui.Dp(18)),
                Position = new Position(x, h - bar - inset - Ui.Dp(18)),
                BackgroundColor = PleyColors.OnArt,
                CornerRadius = round,
            };
            text.Size = new Size(box.SizeWidth, box.SizeHeight);
            text.HorizontalAlignment = HorizontalAlignment.Center;
            box.Add(text);
            well.Add(box);
            x += width + Ui.Dp(6);
        }
    }
}

/// <summary>
/// The last tile of a row that has more (the Android app's TvSeeAllTile, design spec §2.4): a well of
/// the row's tile size on Surface2 with an arrow and "See all", focused like any tile so Right stays
/// meaningful at the row's end.
/// </summary>
sealed class SeeAllTile : View
{
    readonly View frame;
    readonly View ring;

    public SeeAllTile(TileKind kind, string label, FocusItem focus)
    {
        Focus = focus;
        var w = Ui.Dp(kind.Width);
        var h = Ui.Dp(kind.Height);
        var radius = kind.Round ? w / 2 : Ui.Dp(PleyShape.TileRadius);
        Size = new Size(w, h + Ui.Dp(PleyTv.TilesToCaption + PleyTv.CaptionHeight));
        frame = new View { Size = new Size(w, h), PivotPoint = global::Tizen.NUI.PivotPoint.Center, PositionUsesPivotPoint = false };
        var reach = Ui.Dp(PleyShape.RingOffset + PleyShape.RingStroke / 2);
        ring = new View
        {
            Size = new Size(w + 2 * reach, h + 2 * reach),
            Position = new Position(-reach, -reach),
            BackgroundColor = Color.Transparent,
            BorderlineWidth = Ui.Dp(PleyShape.RingStroke),
            BorderlineColor = PleyColors.AccentBright,
            BorderlineOffset = -1f,
            CornerRadius = kind.Round ? (w + 2 * reach) / 2 : radius + reach,
            Opacity = 0,
        };
        frame.Add(ring);
        var well = new View { Size = new Size(w, h), CornerRadius = radius, BackgroundColor = PleyColors.Surface2 };
        var text = Text.Label(label, PleyType.Label, PleyColors.Fg, width: kind.Width - 8);
        text.HorizontalAlignment = HorizontalAlignment.Center;
        var block = 32 + 8 + PleyType.Label.Line;
        var top = (kind.Height - block) / 2;
        var arrow = Icons.Icon("arrow-right", 32, PleyColors.Fg);
        arrow.Position = new Position((w - Ui.Dp(32)) / 2, Ui.Dp(top));
        well.Add(arrow);
        text.Position = new Position(Ui.Dp(4), Ui.Dp(top + 32 + 8));
        well.Add(text);
        frame.Add(well);
        Add(frame);
        focus.FocusChanged += Look;
    }

    public FocusItem Focus { get; }

    void Look(bool on)
    {
        if (Disposed) return;
        if (on) RaiseToTop();
        var scale = on ? PleyShape.TileScale : 1f;
        var a = new Animation(on ? PleyMotion.FocusMs : PleyMotion.UnfocusMs);
        a.AnimateTo(frame, "Scale", new Vector3(scale, scale, 1));
        a.AnimateTo(ring, "Opacity", on ? 1f : 0f);
        a.PlayHeld();
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        Focus.FocusChanged -= Look;
        base.Dispose(type);
    }
}
