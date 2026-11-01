using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Player;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Text subtitles over the video (the Android app's ScreenCaptions, for the server's cues): white
/// and bold with a dark outline, or on a box of 66 % black, in the size, face and position
/// Settings → Subtitles gives them. The size is a share of the screen's height; the text stays in
/// the TV's safe area, a cue without a line at the bottom, its last line 40 dp above the edge
/// (raised as Settings says), the next one above it; a cue's line, position, alignment and size
/// place it as WebVTT says. While the controls show, the text glides up above them, with them.
/// </summary>
sealed class SubtitleView : View
{
    /// <summary>The last line's distance from the bottom edge: the safe area and a little more, so outlines clear an overscanned edge.</summary>
    const float Bottom = PleyTv.SafeVertical + 13;
    /// <summary>While the controls show, above them: their times start at 399 on the 540 dp canvas.</summary>
    const float Lifted = PleyTv.CanvasHeight - 399 + 12;
    /// <summary>66 % black.</summary>
    static readonly Color BoxColor = new Color(0, 0, 0, 0xA8 / 255f);

    readonly View layer = new View();
    IReadOnlyList<Cue> cues = new Cue[0];
    Settings look;
    Animation? glide;

    public SubtitleView(Settings look)
    {
        this.look = look;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        layer.Size = Size;
        Add(layer);
    }

    /// <summary>The cues showing now, earliest first.</summary>
    public void Show(IReadOnlyList<Cue> showing)
    {
        if (Disposed) return;
        cues = showing;
        Draw();
    }

    /// <summary>Settings changed: drawn again when their look did.</summary>
    public void Restyle(Settings settings)
    {
        if (Disposed) return;
        var same = settings.SubtitleSize == look.SubtitleSize && settings.SubtitleBackground == look.SubtitleBackground
            && settings.SubtitleFont == look.SubtitleFont && settings.SubtitleRaise == look.SubtitleRaise;
        look = settings;
        if (!same) Draw();
    }

    /// <summary>Above the controls while they show, back down when they go, over the controls' fade.</summary>
    public void Lift(bool up)
    {
        if (Disposed) return;
        glide?.Stop();
        glide = new Animation(PleyMotion.OverlayMs) { DefaultAlphaFunction = new AlphaFunction(up ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn) };
        glide.AnimateTo(layer, "PositionY", up ? -Ui.Dp(Lifted - Bottom) : 0f);
        glide.Play();
    }

    /// <summary>The face a font setting names: Roboto for sans serif, the TV's own for the rest.</summary>
    static string Family(SubtitleFont font) => font switch
    {
        SubtitleFont.Serif => "serif",
        SubtitleFont.Monospace => "monospace",
        SubtitleFont.Casual => "cursive",
        _ => PleyType.Roboto,
    };

    void Draw()
    {
        foreach (var child in layer.Children.ToArray())
        {
            layer.Remove(child);
            child.Dispose();
        }
        if (cues.Count == 0) return;
        var height = Ui.Dp(PleyTv.CanvasHeight);
        var px = look.SubtitleSize * height;
        var line = px * 1.25f;
        var safeLeft = Ui.Dp(PleyTv.SafeHorizontal);
        var safeWidth = Ui.Dp(PleyTv.CanvasWidth - 2 * PleyTv.SafeHorizontal);
        var safeTop = Ui.Dp(PleyTv.SafeVertical);
        var floor = height - Ui.Dp(Bottom) - look.SubtitleRaise * height;
        var stack = floor;
        foreach (var cue in cues)
        {
            var room = cue.Size is double share ? safeWidth * (float)Math.Max(10, Math.Min(100, share)) / 100 : safeWidth;
            var box = Box(cue, px, room);
            var w = box.SizeWidth;
            var h = box.SizeHeight;
            // Across: at the cue's position (or where its alignment puts it), kept in the safe area.
            var anchor = safeLeft + safeWidth * (float)(cue.Position ?? cue.Align switch
            {
                "start" or "left" => 0,
                "end" or "right" => 100,
                _ => 50,
            }) / 100;
            var x = cue.Align switch
            {
                "start" or "left" => anchor,
                "end" or "right" => anchor - w,
                _ => anchor - w / 2,
            };
            x = Math.Max(safeLeft, Math.Min(safeLeft + safeWidth - w, x));
            // Down: the bottom ones stacked upwards; a line number from the top or the bottom; a percentage from the top.
            float y;
            if (cue.Line is not double at)
            {
                y = stack - h;
                stack = y - line * 0.15f;
            }
            else if (!cue.SnapToLines) y = height * (float)Math.Max(0, Math.Min(100, at)) / 100;
            else if (at >= 0) y = safeTop + (float)at * line;
            else y = floor - h + (float)(at + 1) * line;
            y = Math.Max(safeTop, Math.Min(height - safeTop - h, y));
            box.Position = new Position(x, y);
            layer.Add(box);
        }
    }

    /// <summary>One cue's text, as wide as its longest line (<paramref name="room"/> at most), on its box when Settings has one.</summary>
    View Box(Cue cue, float px, float room)
    {
        var dark = look.SubtitleBackground == SubtitleBackground.DarkBox;
        var padX = dark ? px * 0.25f : px * 0.1f;
        var padY = dark ? px * 0.08f : 0;
        var label = new TextLabel
        {
            EnableMarkup = true,
            Text = cue.Markup(),
            PixelSize = px,
            TextColor = Color.White,
            FontFamily = Family(look.SubtitleFont),
            FontStyle = Text.Weight(PleyType.Bold),
            MultiLine = true,
            HorizontalAlignment = cue.Align switch
            {
                "start" or "left" => HorizontalAlignment.Begin,
                "end" or "right" => HorizontalAlignment.End,
                _ => HorizontalAlignment.Center,
            },
            VerticalAlignment = VerticalAlignment.Top,
        };
        if (!dark)
        {
            var outline = new PropertyMap();
            outline.Add("color", new PropertyValue(Color.Black));
            outline.Add("width", new PropertyValue((int)Math.Max(2, Math.Round(px * 0.06f))));
            label.Outline = outline;
        }
        var w = Math.Min((float)Math.Ceiling(label.NaturalSize.Width) + 2, room - 2 * padX);
        var h = label.GetHeightForWidth(w);
        label.Size = new Size(w, h);
        label.Position = new Position(padX, padY);
        var box = new View { Size = new Size(w + 2 * padX, h + 2 * padY), BackgroundColor = dark ? BoxColor : Color.Transparent };
        box.Add(label);
        return box;
    }
}
