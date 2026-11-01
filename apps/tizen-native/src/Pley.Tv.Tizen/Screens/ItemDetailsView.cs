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
/// TvMediaInfo): who made it and the facts on a card, a row per version (the one that plays
/// ticked; OK chooses another), and Media info, closed until OK opens it: each file and every
/// stream in full, in two columns of blocks. Down reads on through the text: the card and each
/// file take a focus that shows nothing, and the section moves up to show it.
/// </summary>
sealed class ItemDetailsView : View
{
    /// <summary>Details end well inside the safe area: long lines are easier to read narrower.</summary>
    const float End = 200, LabelWidth = 160, RowHeight = 44, BlockWidth = 300, BlockGap = 24, BlockLabel = 104;
    /// <summary>Room left above what reading on brings up, under the section's line.</summary>
    const float ReadRoom = 40;

    readonly ItemPage page;
    readonly Action<long> chooseVersion;
    readonly View inner = new View();
    readonly float width = PleyTv.CanvasWidth - PleyTv.Gutter - End;
    Animation? move;
    bool open;
    float mediaTop;
    readonly List<View> mediaViews = new List<View>();
    readonly List<FocusItem> mediaItems = new List<FocusItem>();
    FocusItem? toggle;
    ImageView? chevron;

    public ItemDetailsView(ItemPage page, Action<long> chooseVersion)
    {
        this.page = page;
        this.chooseVersion = chooseVersion;
        Group = new FocusGroup(Axis.Column, "details");
        Add(inner);
        Build();
    }

    public FocusGroup Group { get; }

    void Build()
    {
        var details = page.Details;
        float y = 0;
        var header = Text.Label(ItemText.Details, PleyType.Section, PleyColors.Fg, width: width);
        inner.Add(header);
        y += PleyType.Section.Line + PleyTv.HeaderToTiles;

        var facts = ItemText.Facts(details);
        if (facts.Count > 0)
        {
            var card = FactsCard(facts);
            card.PositionY = Ui.Dp(y);
            inner.Add(card);
            var cardTop = y;
            y += card.SizeHeight / Ui.Scale;
            // Nothing else to focus: the card takes a focus that shows nothing, so Down reaches it.
            if (details.Versions.Count == 0 && page.MediaInfo.Count == 0) Reader(cardTop);
        }

        if (details.Versions.Count > 0)
        {
            y += 20;
            var title = Text.Label(ItemText.Versions, PleyType.Title, PleyColors.Fg, width: width);
            title.PositionY = Ui.Dp(y);
            inner.Add(title);
            y += PleyType.Title.Line + 8;
            foreach (var version in details.Versions)
            {
                var v = version;
                var row = new Row(version.Label, null, version.Selected ? "check" : null, width, () => chooseVersion(v.Id), checkedRow: version.Selected);
                row.PositionY = Ui.Dp(y);
                inner.Add(row);
                Group.Add(row.Item);
                Reveal(row.Item, y);
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
            Reveal(row.Item, y);
            toggle = row.Item;
            chevron = row.Icon;
            y += row.SizeHeight / Ui.Scale + 8;
            mediaTop = y;
        }
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(y));
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
                mediaItems.Add(Reader(y));
                y += card.SizeHeight / Ui.Scale + 12;
            }
        }
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(y));
    }

    /// <summary>A focus that shows nothing, for reading on: the section moves up to show what is at <paramref name="top"/>.</summary>
    FocusItem Reader(float top)
    {
        var item = new FocusItem("read" + top);
        Group.Add(item);
        Reveal(item, top);
        return item;
    }

    /// <summary>Focus at <paramref name="top"/> in the section: it moves up so that much is in view, back down for the first rows.</summary>
    void Reveal(FocusItem item, float top)
    {
        item.FocusChanged += on =>
        {
            if (!on || Disposed) return;
            var shift = Math.Max(0, top - ReadRoom - 120);
            move?.Stop();
            move = new Animation(PleyMotion.FocusMs) { DefaultAlphaFunction = new AlphaFunction(AlphaFunction.BuiltinFunctions.EaseOut) };
            move.AnimateTo(inner, "PositionY", -Ui.Dp(shift));
            move.Play();
        };
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
