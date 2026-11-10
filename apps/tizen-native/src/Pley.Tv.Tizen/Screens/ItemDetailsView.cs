using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Details at the end of an item page (design spec §4.3; the Android app's TvDetails and
/// TvMediaInfo): who made it and the facts on a card when there's no cast row for them to go under
/// (<see cref="FactsStrip"/>), a row per version (the one that plays
/// ticked; OK chooses another), and Media info, closed until OK opens it: each file and every
/// stream in full, in two columns of blocks. Down reads on through the text as Android TV's readOn
/// does: past the last row, each Down moves the section up a step (160 dp) until its end is the safe
/// area above the screen's bottom, each step a focus that shows nothing, which Up takes back.
///
/// As on Android TV, nothing of it is drawn above the rows' header line (less 12 dp), so the page's
/// info block stays clear; a focused row moves it up only as far as keeps that row inside the safe
/// area, never above the line. Reading on, the info block steps aside (<see cref="Reading"/>) and
/// the text has the screen from the safe area down.
/// </summary>
sealed class ItemDetailsView : View
{
    /// <summary>Details end well inside the safe area: long lines are easier to read narrower.</summary>
    const float End = 200, LabelWidth = 160, RowHeight = 44, BlockWidth = 300, BlockGap = 24, BlockLabel = 104;
    /// <summary>What shows above the rows' header line: nothing within 12 dp of it, the safe area's top while reading on.</summary>
    const float Clearance = 12, ReadingAbove = PleyTv.FocusedRowHeaderY - PleyTv.SafeVertical;
    /// <summary>A step of reading on: a few lines of text, about a third of the screen.</summary>
    const float ReadStep = 160;

    readonly ItemPage page;
    readonly Action<long> chooseVersion;
    /// <summary>Draws nothing above the line: it starts that far above the section and clips what it holds.</summary>
    readonly View window = new View { ClippingMode = ClippingModeType.ClipToBoundingBox };
    readonly View inner = new View();
    readonly float width = PleyTv.CanvasWidth - PleyTv.Gutter - End;
    Animation? move;
    /// <summary>How far above the section the window reaches (dp), and how far up the section has moved.</summary>
    float above = Clearance, shift;
    bool reading;
    bool open;
    float mediaTop, toggleTop, toggleHeight;
    readonly List<View> mediaViews = new List<View>();
    readonly List<FocusItem> mediaItems = new List<FocusItem>();
    FocusItem? toggle;
    ImageView? chevron;

    public ItemDetailsView(ItemPage page, Action<long> chooseVersion)
    {
        this.page = page;
        this.chooseVersion = chooseVersion;
        Group = new FocusGroup(Axis.Column, "details");
        window.PositionY = -Ui.Dp(above);
        inner.PositionY = Ui.Dp(above);
        window.Add(inner);
        Add(window);
        Build();
    }

    public FocusGroup Group { get; }

    /// <summary>Reading on through the text (true) or back on a row or out of the section: the page's info block steps aside while true.</summary>
    public event Action<bool>? Reading;

    void Build()
    {
        var details = page.Details;
        float y = 0;
        // At the gutter, as the cards and rows are: at 0 the rail covered it.
        var header = Text.Label(ItemText.Details, PleyType.Section, PleyColors.Fg, width: width);
        header.PositionX = Ui.Dp(PleyTv.Gutter);
        inner.Add(header);
        y += PleyType.Section.Line + PleyTv.HeaderToTiles;

        var facts = ItemText.FactsUnderCast(page) ? Array.Empty<(string, string)>() : ItemText.Facts(details);
        if (facts.Count > 0)
        {
            var card = FactsCard(facts);
            card.PositionY = Ui.Dp(y);
            inner.Add(card);
            y += card.SizeHeight / Ui.Scale;
            // Nothing else to focus: the card takes a focus that shows nothing, so Down reaches it,
            // and reading on steps through what doesn't fit.
            if (details.Versions.Count == 0 && page.MediaInfo.Count == 0)
            {
                Reader(0, reads: false);
                Readers(0, y);
            }
        }

        if (details.Versions.Count > 0)
        {
            y += 20;
            var title = Text.Label(ItemText.Versions, PleyType.Title, PleyColors.Fg, width: width);
            title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y));
            inner.Add(title);
            y += PleyType.Title.Line + 8;
            foreach (var version in details.Versions)
            {
                var v = version;
                var row = new Row(version.Label, null, version.Selected ? "check" : null, width, () => chooseVersion(v.Id), checkedRow: version.Selected);
                row.PositionY = Ui.Dp(y);
                inner.Add(row);
                Group.Add(row.Item);
                Reveal(row.Item, y, RowHeight);
                y += RowHeight + 2;
            }
        }

        if (page.MediaInfo.Count > 0)
        {
            y += 20;
            var row = new Row(ItemText.MediaInfo, ItemText.MediaInfoHint, "chevron-down", width, Toggle);
            row.PositionY = Ui.Dp(y);
            inner.Add(row);
            Group.Add(row.Item);
            Reveal(row.Item, y, row.SizeHeight / Ui.Scale);
            toggleTop = y;
            toggleHeight = row.SizeHeight / Ui.Scale;
            toggle = row.Item;
            chevron = row.Icon;
            y += row.SizeHeight / Ui.Scale + 8;
            mediaTop = y;
        }
        Sized(y);
    }

    /// <summary>Media info opens and closes: each file's card and its blocks under the row.</summary>
    void Toggle()
    {
        open = !open;
        if (chevron != null) chevron.ResourceUrl = Icons.Url(open ? "chevron-up" : "chevron-down");
        foreach (var v in mediaViews) v.Dispose();
        mediaViews.Clear();
        foreach (var item in mediaItems) Group.Remove(item);
        mediaItems.Clear();
        var y = mediaTop;
        if (open)
        {
            foreach (var file in page.MediaInfo)
            {
                var card = FileCard(file);
                card.PositionY = Ui.Dp(y);
                inner.Add(card);
                mediaViews.Add(card);
                y += card.SizeHeight / Ui.Scale + 12;
            }
            mediaItems.AddRange(Readers(RowShift(toggleTop, toggleHeight), y));
        }
        Sized(y);
    }

    /// <summary>The section is <paramref name="height"/> dp tall: the window holds it and what shows above it.</summary>
    void Sized(float height)
    {
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(height));
        window.Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(height + above));
    }

    /// <summary>
    /// The steps of reading on from <paramref name="from"/> (dp moved up) through a section
    /// <paramref name="end"/> dp tall: 160 dp each, the last as far as brings its end to the safe
    /// area above the screen's bottom.
    /// </summary>
    List<FocusItem> Readers(float from, float end)
    {
        var most = Math.Max(0, PleyTv.FocusedRowHeaderY + end - (PleyTv.CanvasHeight - PleyTv.SafeVertical));
        var steps = new List<FocusItem>();
        for (var at = from; at < most - 0.5f;)
        {
            at = Math.Min(at + ReadStep, most);
            steps.Add(Reader(at, reads: true));
        }
        return steps;
    }

    /// <summary>A focus that shows nothing: the section moved up <paramref name="shift"/> dp, the info block aside while it <paramref name="reads"/> on.</summary>
    FocusItem Reader(float shift, bool reads)
    {
        var item = new FocusItem("read" + shift);
        Group.Add(item);
        item.FocusChanged += on =>
        {
            if (on && !Disposed) Move(shift, reads);
        };
        return item;
    }

    /// <summary>
    /// How far a focused row <paramref name="height"/> dp tall at <paramref name="top"/> moves the
    /// section up: only while that row would end below the safe area, never so far that its top goes
    /// above the line (Android TV's ItemPageSpec.details).
    /// </summary>
    static float RowShift(float top, float height) =>
        Math.Min(top, Math.Max(0, PleyTv.FocusedRowHeaderY + top + height - (PleyTv.CanvasHeight - PleyTv.SafeVertical)));

    /// <summary>A row <paramref name="height"/> dp tall at <paramref name="top"/> has focus: the section moves as <see cref="RowShift"/> says.</summary>
    void Reveal(FocusItem item, float top, float height)
    {
        item.FocusChanged += on =>
        {
            if (on && !Disposed) Move(RowShift(top, height), reads: false);
        };
    }

    /// <summary>
    /// Focus goes up out of the section: it comes back down to where it starts, under the row
    /// above, which it would otherwise cover.
    /// </summary>
    public void Rest()
    {
        if (!Disposed) Move(0, reads: false);
    }

    void Move(float dp, bool reads)
    {
        if (reads != reading)
        {
            reading = reads;
            // The window reaches up to the safe area while reading on; the section stays where it is.
            above = reads ? ReadingAbove : Clearance;
            window.PositionY = -Ui.Dp(above);
            window.SizeHeight = SizeHeight + Ui.Dp(above);
            inner.PositionY = Ui.Dp(above - shift);
            Reading?.Invoke(reads);
        }
        shift = dp;
        move?.Stop();
        move = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
        move.AnimateTo(inner, "PositionY", Ui.Dp(above - dp));
        move.Play();
    }

    /// <summary>The facts on a Surface2 card at 60 %: muted labels in a 160 dp column, values beside them, two lines at most.</summary>
    View FactsCard(IReadOnlyList<(string Label, string Value)> facts)
    {
        var card = new View { BackgroundColor = Fade(PleyColors.Surface2, 0.6f), CornerRadius = Ui.Dp(PleyShape.CardRadius), PositionX = Ui.Dp(PleyTv.Gutter) };
        float y = 16;
        var valueWidth = width - 32 - LabelWidth;
        foreach (var (label, value) in facts)
        {
            var l = Text.Label(label, PleyType.Body, PleyColors.Subtle, width: LabelWidth);
            l.Position = new Position(Ui.Dp(16), Ui.Dp(y + 3));
            card.Add(l);
            var v = Wrapped(value, PleyType.Body, Fade(PleyColors.Fg, 0.85f), valueWidth, 2);
            v.Position = new Position(Ui.Dp(16 + LabelWidth), Ui.Dp(y + 3));
            card.Add(v);
            y += v.SizeHeight / Ui.Scale + 6;
        }
        card.Size = new Size(Ui.Dp(width), Ui.Dp(y + 16));
        return card;
    }

    /// <summary>A file in Media info: its title, then its blocks two to a row.</summary>
    View FileCard(MediaFileInfo file)
    {
        var card = new View { BackgroundColor = Fade(PleyColors.Surface2, 0.6f), CornerRadius = Ui.Dp(PleyShape.CardRadius), PositionX = Ui.Dp(PleyTv.Gutter) };
        float y = 16;
        if (file.Title != null)
        {
            var title = Text.Label(file.Title, PleyType.Label, PleyColors.Fg, width: width - 32);
            title.Position = new Position(Ui.Dp(16), Ui.Dp(y));
            card.Add(title);
            y += PleyType.Label.Line + 10;
        }
        var blocks = MediaFacts.Blocks(file);
        for (var i = 0; i < blocks.Count; i += 2)
        {
            float tallest = 0;
            for (var j = i; j < Math.Min(blocks.Count, i + 2); j++)
            {
                var block = Block(blocks[j]);
                block.Position = new Position(Ui.Dp(16 + (j - i) * (BlockWidth + BlockGap)), Ui.Dp(y));
                card.Add(block);
                tallest = Math.Max(tallest, block.SizeHeight / Ui.Scale);
            }
            y += tallest + 14;
        }
        card.Size = new Size(Ui.Dp(width), Ui.Dp(y + 2));
        return card;
    }

    /// <summary>A block's title, then its facts: muted labels, values wrapping beside them.</summary>
    static View Block(MediaFactBlock block)
    {
        var view = new View();
        var title = Text.Label(block.Title, PleyType.Label, PleyColors.Fg, width: BlockWidth);
        view.Add(title);
        float y = PleyType.Label.Line + 4;
        foreach (var fact in block.Facts)
        {
            var label = Text.Label(fact.Label, PleyType.Caption, PleyColors.Muted, width: BlockLabel);
            label.PositionY = Ui.Dp(y + 2);
            view.Add(label);
            var value = Wrapped(fact.Value, PleyType.Caption, PleyColors.Fg, BlockWidth - BlockLabel, 3);
            value.Position = new Position(Ui.Dp(BlockLabel), Ui.Dp(y + 2));
            view.Add(value);
            y += value.SizeHeight / Ui.Scale + 4;
        }
        view.Size = new Size(Ui.Dp(BlockWidth), Ui.Dp(y));
        return view;
    }

    /// <summary>Text wrapping in <paramref name="width"/>, as tall as its lines (up to <paramref name="maxLines"/>).</summary>
    /// <summary>
    /// Who made it and the facts under the cast row (the Android app's TvFacts), where they're read
    /// beside the people: each a Caption label in Subtle over its value, two lines at most, four to a
    /// line across the 816 dp from the gutter to the safe area. Two lines of them fit under the cast
    /// row while it has focus. Never takes focus.
    /// </summary>
    public static View FactsStrip(IReadOnlyList<(string Label, string Value)> facts)
    {
        const int PerLine = 4;
        const float Width = 186, Gap = 24, LineGap = 12, LabelToValue = 2;
        var strip = new View { PositionX = Ui.Dp(PleyTv.Gutter) };
        float y = 0, line = 0;
        for (var i = 0; i < facts.Count; i++)
        {
            if (i > 0 && i % PerLine == 0)
            {
                y += line + LineGap;
                line = 0;
            }
            var x = i % PerLine * (Width + Gap);
            var (label, value) = facts[i];
            var l = Text.Label(label, PleyType.Caption, PleyColors.Subtle, width: Width);
            l.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            strip.Add(l);
            var v = Wrapped(value, PleyType.Body, Fade(PleyColors.Fg, 0.9f), Width, 2);
            v.Position = new Position(Ui.Dp(x), Ui.Dp(y + PleyType.Caption.Line + LabelToValue));
            strip.Add(v);
            line = Math.Max(line, PleyType.Caption.Line + LabelToValue + v.SizeHeight / Ui.Scale);
        }
        strip.Size = new Size(Ui.Dp(PerLine * Width + (PerLine - 1) * Gap), Ui.Dp(y + line));
        return strip;
    }

    static TextLabel Wrapped(string text, TextStyle style, Color color, float width, int maxLines)
    {
        var label = Text.Label(text, style, color, maxLines: maxLines, width: width);
        label.VerticalAlignment = VerticalAlignment.Top;
        var line = Text.LineHeight(style);
        var lines = Math.Max(1, Math.Min(maxLines, (int)Math.Round(label.GetHeightForWidth(Ui.Dp(width)) / line)));
        label.SizeHeight = line * lines;
        return label;
    }

    static Color Fade(Color c, float alpha) => new Color(c.R, c.G, c.B, alpha);

    /// <summary>A row as a side panel draws it (design spec §2.7): 44 dp or more, Body, a line under it, an icon at its end; focused, the accent pill.</summary>
    sealed class Row : View
    {
        readonly TextLabel label;
        readonly TextLabel? hint;
        readonly bool checkedRow;

        public Row(string text, string? supporting, string? icon, float width, Action select, bool checkedRow = false)
        {
            this.checkedRow = checkedRow;
            var height = supporting == null ? RowHeight : RowHeight + PleyType.Caption.Line;
            Size = new Size(Ui.Dp(width), Ui.Dp(height));
            PositionX = Ui.Dp(PleyTv.Gutter);
            CornerRadius = Ui.Dp(PleyShape.ControlRadius);
            Item = new FocusItem(text) { Selected = select };
            var room = width - 32 - (icon != null ? PleyTv.IconSize + 12 : 0);
            var top = (height - PleyType.Body.Line - (supporting != null ? PleyType.Caption.Line : 0)) / 2;
            label = Text.Label(text, PleyType.Body, PleyColors.Fg, width: room);
            label.Position = new Position(Ui.Dp(16), Ui.Dp(top));
            Add(label);
            if (supporting != null)
            {
                hint = Text.Label(supporting, PleyType.Caption, PleyColors.Muted, width: room);
                hint.Position = new Position(Ui.Dp(16), Ui.Dp(top + PleyType.Body.Line));
                Add(hint);
            }
            if (icon != null)
            {
                Icon = Icons.Icon(icon, PleyTv.IconSize, checkedRow ? PleyColors.Accent : PleyColors.Fg);
                Icon.Position = new Position(Ui.Dp(width - 16 - PleyTv.IconSize), Ui.Dp((height - PleyTv.IconSize) / 2));
                Add(Icon);
            }
            Item.FocusChanged += Look;
        }

        public FocusItem Item { get; }

        public ImageView? Icon { get; }

        void Look(bool focused)
        {
            if (Disposed) return;
            BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
            label.TextColor = focused ? PleyColors.OnAccent : PleyColors.Fg;
            if (hint != null) hint.TextColor = focused ? PleyColors.OnAccent : PleyColors.Muted;
            if (Icon != null) Icon.Color = focused ? PleyColors.OnAccent : checkedRow ? PleyColors.Accent : PleyColors.Fg;
        }
    }
}
