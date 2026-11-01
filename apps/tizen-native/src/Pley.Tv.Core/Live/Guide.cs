using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;

namespace Pley.Tv.Core.Live;

/// <summary>
/// The programme guide's geometry on the 960 × 540 canvas, in dp (the Android app's TvGuide): two
/// hours across at 3.6 dp a minute in 432 dp beside the 160 dp channel column, 52 dp rows, what
/// has focus described in 92 dp above a 28 dp time header.
/// </summary>
public static class GuideLayout
{
    /// <summary>The grid shows this much time.</summary>
    public const long ViewS = 120 * 60;

    public const float DpPerMinute = 3.6f;

    public const float DpPerSecond = DpPerMinute / 60f;

    public const float RowHeight = 52;

    /// <summary>The channel cell: logo (56 × 34), number and name.</summary>
    public const float ChannelWidth = 160;

    /// <summary>The channel cell's filling starts this far left of the page's gutter, so its text lines up with it.</summary>
    public const float ChannelInset = 10;

    /// <summary>What has focus, above the grid: title, meta line, two lines of text, and the channel's 120 × 68 tile at its right.</summary>
    public const float InfoHeight = 92;

    /// <summary>The times across the top, the day at the left.</summary>
    public const float HeaderHeight = 28;

    /// <summary>Room under the last row.</summary>
    public const float BottomPadding = 27;

    /// <summary>Channel up and down move this many rows.</summary>
    public const int PageRows = 5;

    /// <summary>Narrower than this (px of the 960 canvas), a programme shows no text.</summary>
    public const float MinTextWidth = 70;

    /// <summary>A cell's programme keeps its title in view this far from its right end, when it began left of the grid.</summary>
    public const float TitleRoom = 90;

    /// <summary>Each cell is inset by 4 dp above and below and 2 dp at either side; its text by 10 dp.</summary>
    public const float CellInsetY = 4, CellInsetX = 2, CellPadding = 10;

    /// <summary>The now line: 2 dp, the accent at 85 %.</summary>
    public const float NowLineWidth = 2;

    /// <summary>A programme's progress line inside an airing cell.</summary>
    public const float ProgressHeight = 2;

    /// <summary>Programmes that ended show at 55 % unless focused.</summary>
    public const float PastAlpha = 0.55f;

    /// <summary>The x of <paramref name="time"/> in the grid that starts at <paramref name="start"/>, before it scrolls.</summary>
    public static float X(long time, long start) => (time - start) * DpPerSecond;

    /// <summary>How far the grid has scrolled when its left edge is at <paramref name="left"/>.</summary>
    public static float Scroll(long left, long start) => X(left, start);

    /// <summary>A programme's cell: its x from the grid's start (what began before the guide starts at its left edge), and its width.</summary>
    public static (float X, float Width) Cell(LiveProgram program, long start)
    {
        var from = Math.Max(program.Start, start);
        return (X(from, start), (program.Stop - from) * DpPerSecond);
    }

    /// <summary>The programmes that may come into view as the grid moves a screen either way.</summary>
    public static IReadOnlyList<LiveProgram> Visible(IReadOnlyList<LiveProgram>? programs, long left, long start) =>
        LiveRules.Between(programs, Math.Max(left - ViewS, start), left + 2 * ViewS);

    /// <summary>
    /// The half hours of the time header around <paramref name="left"/>, a screen either way: each
    /// with its x before the grid scrolls (the label sits 6 dp right of it), and whether it is
    /// midnight (then the label is the day, in the foreground colour, not the time in muted).
    /// </summary>
    public static IReadOnlyList<(long Time, float X)> Slots(long left, long start)
    {
        var out_ = new List<(long, float)>();
        for (var t = GuideCursor.Slot(left - ViewS); t < left + 2 * ViewS; t += LiveRules.SlotS)
            if (t >= start) out_.Add((t, X(t, start)));
        return out_;
    }
}

/// <summary>What OK on the guide does.</summary>
public enum GuideOk
{
    /// <summary>Nothing has focus (a row still loading).</summary>
    None,
    /// <summary>A programme on now, or a channel without guide: the live player.</summary>
    Watch,
    /// <summary>Another programme: its details in the side panel.</summary>
    Details,
}

/// <summary>
/// Focus in the programme guide (the Android app's TvGuide), moved by the guide itself rather
/// than by geometry: a row (the channel), a time (the cursor, which says the programme) and the
/// grid's left edge.
///
/// - Left and Right go to the programme before and after, the grid following by half hours; Left
///   from the guide's first half hour, or on a channel without programmes, leaves for the
///   categories.
/// - Up and Down keep the time and change channel; Up from the first row leaves for the tabs.
/// - Channel up and down move 5 rows.
/// - OK on a programme on now (or a channel without guide) watches the channel, on another opens
///   its details, as does holding OK or Menu (<see cref="Details"/>).
/// - Back away from now comes back to now first (<see cref="AwayFromNow"/>, <see cref="BackToNow"/>).
/// </summary>
public sealed class GuideCursor
{
    public GuideCursor(long start, long now)
    {
        Start = start;
        Cursor = now;
        Left = LeftFor(now, start);
    }

    /// <summary>Where the guide starts (<see cref="LiveTvModel.GuideStart"/>).</summary>
    public long Start { get; }

    /// <summary>Where it ends: two days on.</summary>
    public long End => Start + LiveRules.GuideSpanS;

    /// <summary>The row with focus.</summary>
    public int Row { get; private set; }

    /// <summary>The time with focus: the programme on then has it.</summary>
    public long Cursor { get; private set; }

    /// <summary>The grid's left edge; the grid shows <see cref="GuideLayout.ViewS"/> from it (tell <see cref="LiveTvModel.ShowingTimes"/>).</summary>
    public long Left { get; private set; }

    /// <summary>The half hour <paramref name="time"/> is in.</summary>
    public static long Slot(long time) => LiveRules.FloorDiv(time, LiveRules.SlotS) * LiveRules.SlotS;

    /// <summary>Where the grid starts for <paramref name="now"/>: the half hour before the one it is in, never before the guide.</summary>
    public static long LeftFor(long now, long start) => Math.Max(Slot(now) - LiveRules.SlotS, start);

    /// <summary>The programme with focus, of the focused row's <paramref name="programs"/>.</summary>
    public LiveProgram? Current(IReadOnlyList<LiveProgram>? programs) => LiveRules.At(programs, Cursor);

    /// <summary>The list opens at <paramref name="row"/> (the landing) within <paramref name="total"/> rows.</summary>
    public void Place(int row, int total) => Row = Clamp(row, total);

    /// <summary>Keeps the row within <paramref name="total"/> rows, as the list's length changes.</summary>
    public void Fit(int total) => Row = Clamp(Row, total);

    static int Clamp(int row, int total) => Math.Max(0, Math.Min(row, Math.Max(0, total - 1)));

    /// <summary>Puts the cursor at <paramref name="time"/> (a programme's start, or a half hour), the grid following it.</summary>
    public void MoveTo(long time)
    {
        Cursor = Math.Max(Start, Math.Min(time, End - 1));
        if (Cursor < Left) Left = Math.Max(Slot(Cursor), Start);
        else if (Cursor >= Left + GuideLayout.ViewS - LiveRules.SlotS) Left = Math.Max(Start, Math.Min(Slot(Cursor) - LiveRules.SlotS, End - GuideLayout.ViewS));
    }

    /// <summary>Left on the focused row's <paramref name="programs"/>: false when focus leaves the guide for the categories.</summary>
    public bool MoveLeft(IReadOnlyList<LiveProgram>? programs)
    {
        var current = Current(programs);
        if (current != null && current.Start <= Start && Left > Start)
        {
            // The programme began with the guide or before it, and the view is scrolled on: the view
            // goes back through it first, a screen at a time. Gone by the programme alone, Left took
            // focus to the categories with the view still scrolled on, the time before it unseen.
            Left = Math.Max(Start, Math.Max(Slot(current.Start), Left - (GuideLayout.ViewS - LiveRules.SlotS)));
            Cursor = Math.Max(Math.Max(current.Start, Start), Left);
            return true;
        }
        var target = current != null ? current.Start - 1 : Cursor - LiveRules.SlotS;
        // Only from the guide's first half hour. Where a channel has no programmes it goes back a
        // half hour at a time, as Right goes on: refused there, Left took focus to the categories
        // from wherever Right had gone, with no way back through the time.
        if (target < Start) return false;
        if (current == null && Blank(programs) && Left > Start)
        {
            // Nothing in view: the view goes back a half hour with the cursor, as Right goes on.
            Left = Math.Max(Start, Left - LiveRules.SlotS);
            Cursor = Math.Max(Left, Slot(target));
            return true;
        }
        var before = LiveRules.At(programs, target);
        MoveTo(before != null ? Math.Max(before.Start, Slot(target)) : Slot(target));
        return true;
    }

    /// <summary>Whether the row has nothing in view (a channel without a guide): the cursor would move unseen in it.</summary>
    bool Blank(IReadOnlyList<LiveProgram>? programs) => GuideLayout.Visible(programs, Left, Start).Count == 0;

    /// <summary>Right on the focused row's <paramref name="programs"/>: the next programme, or the next half hour; never past the guide.</summary>
    public void MoveRight(IReadOnlyList<LiveProgram>? programs)
    {
        var current = Current(programs);
        var target = current?.Stop ?? Slot(Cursor) + LiveRules.SlotS;
        if (target >= End) return;
        var lastLeft = Math.Max(Start, End - GuideLayout.ViewS);
        if (current == null && Blank(programs) && Left < lastLeft)
        {
            // Nothing in view (a channel without a guide): the view moves on a half hour with the
            // cursor at each press. Moved alone, the cursor went unseen until it neared the edge, and
            // Right seemed to take two presses where Left took one.
            Left = Math.Min(lastLeft, Left + LiveRules.SlotS);
            Cursor = Math.Max(Left, target);
            return;
        }
        MoveTo(target);
    }

    /// <summary>Up: false from the first row, where focus leaves for the tabs above.</summary>
    public bool MoveUp()
    {
        if (Row == 0) return false;
        Row--;
        return true;
    }

    public void MoveDown(int total)
    {
        if (Row < total - 1) Row++;
    }

    /// <summary>Channel up: 5 rows up.</summary>
    public void PageUp() => Row = Math.Max(0, Row - GuideLayout.PageRows);

    /// <summary>Channel down: 5 rows down.</summary>
    public void PageDown(int total) => Row = Math.Min(Math.Max(0, total - 1), Row + GuideLayout.PageRows);

    /// <summary>What OK (released, not held) does on the focused row: its channel <paramref name="channel"/> and programmes.</summary>
    public GuideOk Ok(LiveChannel? channel, IReadOnlyList<LiveProgram>? programs, long now)
    {
        if (channel == null) return GuideOk.None;
        var current = Current(programs);
        return current == null || current.AiringAt(now) ? GuideOk.Watch : GuideOk.Details;
    }

    /// <summary>Holding OK, or Menu or Info: the details of the programme with focus, when there is one.</summary>
    public LiveProgram? Details(IReadOnlyList<LiveProgram>? programs) => Current(programs);

    /// <summary>Whether Back first brings the guide back to now: focus is on something not airing, or the grid moved.</summary>
    public bool AwayFromNow(IReadOnlyList<LiveProgram>? programs, long now) =>
        Left != LeftFor(now, Start)
        // Where there is no programme (a channel without a guide), now's half hour: taken as away
        // there always, Back went on coming back to now and never left the guide.
        || (Current(programs) is { } on ? !on.AiringAt(now) : Slot(Cursor) != Slot(now));

    public void BackToNow(long now)
    {
        Cursor = now;
        Left = LeftFor(now, Start);
    }
}

/// <summary>One row of the categories column (the Android app's CategoryColumn), top to bottom.</summary>
public abstract record CategoryRow
{
    public abstract string Key { get; }

    /// <summary>The search field ("Search"), on the page's column only.</summary>
    public sealed record Search : CategoryRow
    {
        public override string Key => "search";
    }

    /// <summary>A section's heading, written in capitals: "GROUPS FOUND", "REGIONS", "GROUPS", "MORE GROUPS".</summary>
    public sealed record Heading(string Text) : CategoryRow
    {
        public override string Key => "heading:" + Text;
    }

    /// <summary>
    /// A category: its icon by Lucide's name (none for groups), name, count (null while unknown),
    /// whether it is the one shown, and how far its text starts from the row's left (dp).
    /// </summary>
    public sealed record Choice(LiveCategory Category, string Name, int? Count, bool Selected, float Indent, string? Icon = null, bool Found = false) : CategoryRow
    {
        public override string Key => (Found ? "found:" : "") + Category.Key;
    }

    /// <summary>A region, opened and closed with OK (one open at a time): chevron-down open, chevron-right closed.</summary>
    public sealed record Region(string Name, int Count, bool Open) : CategoryRow
    {
        public override string Key => "region:" + Name;
    }
}

/// <summary>
/// The categories down the left of Live TV and in the player's channel list (the Android app's
/// CategoryColumn): under the search, the groups it names, then Favourites, Recently watched and
/// All channels with their counts, then the regions (one opened at a time) and the other groups.
/// Rows are 40 dp tall, 2 dp apart, in a 210 dp column; the one shown is filled with surface3
/// and its icon is the accent; focused, the accent fill.
/// </summary>
public static class LiveCategories
{
    /// <summary>The column's width beside the guide.</summary>
    public const float Width = 210;

    public const float RowHeight = 40;

    /// <summary>The rows' filling starts this far left of the gutter, so their text lines up with it.</summary>
    public const float Inset = 12;

    /// <summary>A group's name starts where the categories' names do, after their icons.</summary>
    public const float GroupInset = Inset + 26;

    /// <summary>A region's groups, further in.</summary>
    public const float RegionGroupInset = GroupInset + 16;

    /// <summary>The region the category <paramref name="selected"/> is in, which opens with the column.</summary>
    public static string? RegionOf(LiveCategory selected, GroupSections sections) =>
        selected is LiveCategory.Group g ? sections.Regions.FirstOrDefault(r => r.Groups.Any(x => x.Name == g.Name))?.Region : null;

    /// <summary>The column's rows: <paramref name="open"/> is the region opened (start with <see cref="RegionOf"/>).</summary>
    public static IReadOnlyList<CategoryRow> Rows(LiveGroups? groups, LiveCategory selected, string? open, bool search, IReadOnlyList<LiveGroup>? found = null)
    {
        var rows = new List<CategoryRow>();
        var sections = LiveRules.Sections(groups?.Groups ?? Array.Empty<LiveGroup>());
        if (search) rows.Add(new CategoryRow.Search());
        if (found is { Count: > 0 })
        {
            rows.Add(new CategoryRow.Heading(LiveText.FoundGroups.ToUpperInvariant()));
            foreach (var g in found) rows.Add(new CategoryRow.Choice(new LiveCategory.Group(g.Name), LiveRules.GroupName(g, g.Name), g.Count, false, GroupInset, Found: true));
        }
        rows.Add(new CategoryRow.Choice(new LiveCategory.Favorites(), LiveText.CategoryFavorites, groups?.Favorites, selected is LiveCategory.Favorites, Inset, "star"));
        rows.Add(new CategoryRow.Choice(new LiveCategory.Recent(), LiveText.CategoryRecent, groups?.Recent, selected is LiveCategory.Recent, Inset, "history"));
        rows.Add(new CategoryRow.Choice(new LiveCategory.All(), LiveText.CategoryAll, groups?.Total, selected is LiveCategory.All, Inset, "layout-grid"));
        if (sections.Regions.Count > 0)
        {
            rows.Add(new CategoryRow.Heading(LiveText.Regions.ToUpperInvariant()));
            foreach (var r in sections.Regions)
            {
                var expanded = open == r.Region;
                rows.Add(new CategoryRow.Region(r.Region, r.Count, expanded));
                if (!expanded) continue;
                foreach (var g in r.Groups)
                {
                    var category = new LiveCategory.Group(g.Name);
                    rows.Add(new CategoryRow.Choice(category, g.Label, g.Count, selected == category, RegionGroupInset));
                }
            }
        }
        var others = sections.Others;
        if (others.Count > 1 || (others.Count == 1 && sections.Regions.Count > 0))
        {
            rows.Add(new CategoryRow.Heading((sections.Regions.Count == 0 ? LiveText.Groups : LiveText.MoreGroups).ToUpperInvariant()));
            foreach (var g in others)
            {
                var category = new LiveCategory.Group(g.Name);
                rows.Add(new CategoryRow.Choice(category, g.Name.Length == 0 ? LiveText.NoGroupName : g.Name, g.Count, selected == category, GroupInset));
            }
        }
        return rows;
    }

    /// <summary>
    /// The row focus comes back to: the category shown's, or its region's while the region is
    /// closed; 0 when it isn't in the column.
    /// </summary>
    public static int ShownIndex(IReadOnlyList<CategoryRow> rows, LiveCategory selected, GroupSections sections)
    {
        for (var i = 0; i < rows.Count; i++)
            if (rows[i] is CategoryRow.Choice { Selected: true, Found: false }) return i;
        if (RegionOf(selected, sections) is { } region)
            for (var i = 0; i < rows.Count; i++)
                if (rows[i] is CategoryRow.Region r && r.Name == region) return i;
        return 0;
    }
}
