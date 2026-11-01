using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>A line of a track list: a disc header (<paramref name="Header"/>), or a track.</summary>
/// <param name="Key">Stays the same while the line is in the list: focus stays on it as the list changes.</param>
/// <param name="Number">The track's number column; the track playing shows a glyph in its place.</param>
/// <param name="Subtitle">Under the title, in Caption: the track's own artist, or "Artist · Album" in the queue.</param>
sealed record TrackLine(string Key, string Title, bool Header = false, string Number = "", string? Subtitle = null, string Duration = "",
    bool Current = false, bool Playing = false, bool Highlighted = false);

/// <summary>
/// A list of tracks (the Android app's TvTrackLine in a LazyColumn): lines 44 dp tall at least, 2 dp
/// apart, and disc headers between them. It scrolls inside its box to keep the focused line in view;
/// only the lines near the box have views, so a queue of a thousand tracks costs a screenful. Its
/// focus group holds the tracks, in order, and keeps focus on a track as the list changes around it.
/// </summary>
sealed class TrackListView : View
{
    const float Gap = 2, MinLine = 44, HeaderHeight = 14 + 18 + 4;
    /// <summary>How much of the line beyond the focused one stays in view as focus nears the box's edge.</summary>
    const float Lookahead = 48;

    readonly KeyRepeatGate gate;
    readonly float width, bottom;
    float height;
    readonly Action<string> pick;
    readonly Func<FocusItem, bool> focus;
    readonly View content = new View();
    readonly Dictionary<string, FocusItem> items = new Dictionary<string, FocusItem>();
    readonly Dictionary<string, TrackLineView> views = new Dictionary<string, TrackLineView>();
    /// <summary>The line of each track in the focus group, by its index there.</summary>
    readonly List<int> lineOf = new List<int>();
    IReadOnlyList<TrackLine> lines = Array.Empty<TrackLine>();
    float[] tops = Array.Empty<float>();
    float[] heights = Array.Empty<float>();
    float total;
    float scroll;
    Animation? scrolling;

    /// <param name="pick">OK on a track, with its key.</param>
    /// <param name="focus">Puts focus on an item (the page's tree): where focus goes when the track it was on leaves the list.</param>
    /// <param name="bottomDp">Room under the last line, so it can scroll up off the box's foot.</param>
    public TrackListView(float widthDp, float heightDp, KeyRepeatGate gate, Action<string> pick, Func<FocusItem, bool> focus, float bottomDp = 0, string name = "tracks")
    {
        width = widthDp;
        height = heightDp;
        bottom = bottomDp;
        this.gate = gate;
        this.pick = pick;
        this.focus = focus;
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));
        ClippingMode = ClippingModeType.ClipChildren;
        Add(content);
        Group = new FocusGroup(Axis.Column, name);
        Group.ChildFocused += i =>
        {
            if (i >= 0 && i < lineOf.Count) Reveal(lineOf[i]);
        };
    }

    public FocusGroup Group { get; }

    /// <summary>Whether a track of the list has focus.</summary>
    public bool Focused => items.Values.Any(i => i.Focused);

    /// <summary>The focus item of the track <paramref name="key"/>; null when it isn't in the list.</summary>
    public FocusItem? Item(string key) => items.TryGetValue(key, out var item) ? item : null;

    /// <summary>The box is <paramref name="heightDp"/> tall now (the page above it grew or shrank).</summary>
    public void Resize(float heightDp)
    {
        if (heightDp == height) return;
        height = heightDp;
        SizeHeight = Ui.Dp(heightDp);
        ScrollTo(scroll, animate: false);
    }

    /// <summary>Coming into the list lands on the track <paramref name="key"/>, until another has had focus.</summary>
    public void Remember(string key)
    {
        if (items.TryGetValue(key, out var item)) Group.RememberChild(IndexOf(item));
    }

    /// <summary>Shows <paramref name="lines"/>; a line that didn't change keeps its view, and a track its focus.</summary>
    public void Set(IReadOnlyList<TrackLine> next)
    {
        var focusedAt = -1;
        string? remembered = Group.Last >= 0 && Group.Last < Group.Children.Count ? KeyOf(Group.Children[Group.Last]) : null;
        var keep = new HashSet<string>(next.Where(l => !l.Header).Select(l => l.Key));
        foreach (var key in items.Keys.ToList())
        {
            if (keep.Contains(key)) continue;
            if (items[key].Focused) focusedAt = IndexOf(items[key]);
            items.Remove(key);
        }
        Group.Clear();
        lineOf.Clear();
        lines = next;
        tops = new float[next.Count];
        heights = new float[next.Count];
        float y = 0;
        for (var i = 0; i < next.Count; i++)
        {
            var line = next[i];
            if (i > 0) y += Gap;
            tops[i] = y;
            heights[i] = HeightOf(line);
            y += heights[i];
            if (line.Header) continue;
            if (!items.TryGetValue(line.Key, out var item))
            {
                var key = line.Key;
                item = new FocusItem(key) { Selected = () => pick(key) };
                items[key] = item;
            }
            Group.Add(item);
            lineOf.Add(i);
        }
        total = y;
        if (remembered != null && items.TryGetValue(remembered, out var again)) Group.RememberChild(IndexOf(again));
        // The track with focus left the list: focus goes to the one in its place, else the last.
        if (focusedAt >= 0 && Group.Children.Count > 0) focus((FocusItem)Group.Children[Math.Min(focusedAt, Group.Children.Count - 1)]);
        ScrollTo(scroll, animate: false);
    }

    /// <summary>Scrolls to the line <paramref name="key"/>, <paramref name="linesAbove"/> lines above it showing.</summary>
    public void ShowLine(string key, int linesAbove = 0)
    {
        for (var i = 0; i < lines.Count; i++)
        {
            if (lines[i].Key != key) continue;
            ScrollTo(tops[Math.Max(0, i - linesAbove)], animate: false);
            return;
        }
    }

    static float HeightOf(TrackLine line) => line.Header
        ? HeaderHeight
        : Math.Max(MinLine, 12 + PleyType.Body.Line + (string.IsNullOrEmpty(line.Subtitle) ? 0 : PleyType.Caption.Line));

    int IndexOf(FocusItem item)
    {
        var children = Group.Children;
        for (var i = 0; i < children.Count; i++)
            if (children[i] == item) return i;
        return -1;
    }

    static string? KeyOf(FocusNode node) => (node as FocusItem)?.Name;

    /// <summary>Keeps the line at <paramref name="index"/> in view, with a line's room beyond it.</summary>
    void Reveal(int index)
    {
        var top = tops[index];
        var end = top + heights[index];
        var target = scroll;
        if (top - Lookahead < target) target = top - Lookahead;
        else if (end + Lookahead > target + height) target = end + Lookahead - height;
        ScrollTo(target, animate: true);
    }

    void ScrollTo(float target, bool animate)
    {
        var most = Math.Max(0, total + bottom - height);
        target = Math.Max(0, Math.Min(target, most));
        var changed = target != scroll;
        scroll = target;
        Draw();
        if (!changed) return;
        scrolling?.Stop();
        if (!animate)
        {
            content.PositionY = -Ui.Dp(target);
            return;
        }
        scrolling = Scrolling.Glide(gate);
        scrolling.AnimateTo(content, "PositionY", -Ui.Dp(target));
        scrolling.Play();
    }

    /// <summary>The lines in the box and half a box either side have views; the others go.</summary>
    void Draw()
    {
        var from = scroll - height / 2;
        var to = scroll + height * 1.5f;
        var shown = new HashSet<string>();
        for (var i = 0; i < lines.Count; i++)
        {
            if (tops[i] + heights[i] < from) continue;
            if (tops[i] > to) break;
            var line = lines[i];
            shown.Add(line.Key);
            if (views.TryGetValue(line.Key, out var view))
            {
                if (view.Line == line && view.Top == tops[i]) continue;
                view.Unparent();
                view.Dispose();
            }
            view = new TrackLineView(line, width, heights[i], tops[i], line.Header ? null : items[line.Key]);
            content.Add(view);
            views[line.Key] = view;
        }
        foreach (var key in views.Keys.ToList())
        {
            if (shown.Contains(key)) continue;
            views[key].Unparent();
            views[key].Dispose();
            views.Remove(key);
        }
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        scrolling?.Stop();
        base.Dispose(type);
    }
}

/// <summary>
/// One line of a track list (the Android app's TvTrackLine): the number in Caption, right-aligned in
/// 28 dp, or for the track playing the audio-lines glyph (the play glyph while paused) in the accent;
/// 16 dp on, the title in Body and the subtitle under it; the length at the right. The track playing
/// is in AccentBright on Fg at 6 %, the one the page was opened from on the accent at 14 %. Focused,
/// it is an accent pill with OnAccent ink, without scaling, as rows in lists are.
/// </summary>
sealed class TrackLineView : View
{
    const float Inset = 12, NumberWidth = 28, NumberGap = 16, Glyph = 18;

    readonly FocusItem? item;
    readonly View fill;
    readonly TextLabel? number;
    readonly ImageView? glyph;
    readonly TextLabel title;
    readonly TextLabel? subtitle;
    readonly TextLabel? duration;

    public TrackLineView(TrackLine line, float widthDp, float heightDp, float topDp, FocusItem? item)
    {
        Line = line;
        Top = topDp;
        this.item = item;
        Position = new Position(0, Ui.Dp(topDp));
        Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp));
        fill = new View { Size = new Size(Ui.Dp(widthDp), Ui.Dp(heightDp)), CornerRadius = Ui.Dp(PleyShape.FieldRadius) };
        Add(fill);
        if (line.Header)
        {
            title = Text.Label(line.Title, PleyType.Label, PleyColors.Subtle, width: widthDp - Inset * 2);
            title.Position = new Position(Ui.Dp(Inset), Ui.Dp(14));
            fill.Add(title);
            return;
        }
        if (line.Current)
        {
            glyph = Icons.Icon(line.Playing ? "audio-lines" : "play", Glyph, PleyColors.Accent);
            glyph.Position = new Position(Ui.Dp(Inset + NumberWidth - Glyph), Ui.Dp((heightDp - Glyph) / 2));
            fill.Add(glyph);
        }
        else
        {
            number = Text.Label(line.Number, PleyType.Caption, PleyColors.Subtle, width: NumberWidth);
            number.HorizontalAlignment = HorizontalAlignment.End;
            number.Position = new Position(Ui.Dp(Inset), (Ui.Dp(heightDp) - number.SizeHeight) / 2);
            fill.Add(number);
        }
        var x = Inset + NumberWidth + NumberGap;
        var right = widthDp - Inset;
        if (line.Duration.Length > 0)
        {
            duration = Text.Label(line.Duration, PleyType.Caption, PleyColors.Muted);
            var w = duration.NaturalSize.Width / Ui.Scale + 1;
            duration.Size = new Size(Ui.Dp(w), Text.LineHeight(PleyType.Caption));
            duration.Position = new Position(Ui.Dp(right - w), (Ui.Dp(heightDp) - duration.SizeHeight) / 2);
            fill.Add(duration);
            right -= w + 12;
        }
        var room = Math.Max(40, right - x);
        title = Text.Label(line.Title, PleyType.Body, PleyColors.Fg, width: room);
        var block = title.SizeHeight;
        if (!string.IsNullOrEmpty(line.Subtitle))
        {
            subtitle = Text.Label(line.Subtitle!, PleyType.Caption, PleyColors.Muted, width: room);
            block += subtitle.SizeHeight;
        }
        var top = (Ui.Dp(heightDp) - block) / 2;
        title.Position = new Position(Ui.Dp(x), top);
        fill.Add(title);
        if (subtitle != null)
        {
            subtitle.Position = new Position(Ui.Dp(x), top + title.SizeHeight);
            fill.Add(subtitle);
        }
        if (item == null) return;
        item.FocusChanged += Look;
        Look(item.Focused);
    }

    public TrackLine Line { get; }

    /// <summary>Where the line is in its list, in dp.</summary>
    public float Top { get; }

    void Look(bool focused)
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        var accent = PleyColors.Accent;
        fill.BackgroundColor = focused ? accent
            : Line.Highlighted ? new Color(accent.R, accent.G, accent.B, 0.14f)
            : Line.Current ? new Color(PleyColors.Fg.R, PleyColors.Fg.G, PleyColors.Fg.B, 0.06f)
            : Color.Transparent;
        title.TextColor = focused ? PleyColors.OnAccent : Line.Current ? PleyColors.AccentBright : PleyColors.Fg;
        var secondary = focused ? PleyColors.OnAccent : PleyColors.Muted;
        if (subtitle != null) subtitle.TextColor = secondary;
        if (duration != null) duration.TextColor = secondary;
        if (number != null) number.TextColor = focused ? PleyColors.OnAccent : PleyColors.Subtle;
        if (glyph != null) glyph.Color = focused ? PleyColors.OnAccent : PleyColors.Accent;
    }

    protected override void Dispose(DisposeTypes type)
    {
        if (Disposed) return;
        if (item != null) item.FocusChanged -= Look;
        base.Dispose(type);
    }
}
