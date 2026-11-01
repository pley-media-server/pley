using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The rail's Collections (design spec §2.1; the Android app's TvCollectionsScreen): every movie
/// and show library's collections, a row each in the server's order of libraries, under
/// "Collections". The first collection has focus, or the one focused last when coming back.
/// </summary>
sealed class CollectionsScreen : ShellPage
{
    const float RowsTop = 80, MessageWidth = 520;

    readonly CollectionsModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly List<IDisposable> watches = new List<IDisposable>();
    readonly View body = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
    RowColumnView? column;
    string shownRows = "";
    LibraryMessage? shownMessage;
    /// <summary>
    /// Where focus was when the page opened with nothing to focus yet (the rail's entry): the
    /// first collection takes focus from there once it comes, not once someone has moved on.
    /// </summary>
    FocusItem? waitingOn;
    bool waiting;

    public CollectionsScreen(TvShell shell, PageKey.Collections key, CollectionsModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, key, "collections")
    {
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.Add(body);
        // The title draws over the rows, which scroll up under it.
        var title = Text.Label(CollectionText.Title, PleyType.PageTitle, PleyColors.Fg, width: 480);
        title.Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(PleyTv.SafeVertical));
        View.Add(title);
        watches.Add(model.State.Watch(Show));
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) ToRail();
    }

    /// <summary>
    /// Nothing on the page can take focus yet: the rail has it (its entry, when the rail opened the
    /// page), and the page takes it back once it can, unless someone has moved on meanwhile.
    /// </summary>
    void ToRail()
    {
        if (Tree.Current == null || Tree.Current.Parent != Shell.Rail.Group) Shell.FocusRail();
        waiting = true;
        waitingOn = Tree.Current;
    }

    public override void Gone()
    {
        foreach (var w in watches) w.Dispose();
        model.Dispose();
    }

    void Show(Loadable<IReadOnlyList<CollectionShelf>> state)
    {
        var message = CollectionText.CollectionsMessage(state);
        if (message != null)
        {
            if (message == shownMessage) return;
            Clear();
            Message(message);
            return;
        }
        var shelves = CollectionText.Shelves(state);
        var key = RowsKey(shelves);
        if (shownMessage == null && key == shownRows) return;
        var focusedName = Tree.Current?.Name;
        var focusWasIn = Tree.Current != null && Within(Group, Tree.Current);
        // The old rows go only once focus is on the new ones: the tile with focus hears it leave.
        var old = Detach();
        shownRows = key;
        if (shelves.Count > 0)
        {
            column = new RowColumnView(gate, top: RowsTop, headerY: RowsTop);
            foreach (var shelf in shelves)
            {
                var tiles = shelf.Collections.Value!.Select(c => c.Tile).ToList();
                column.Add(new RowView(shelf.LibraryName, tiles, shelf.TileKind, gate, images,
                    tile => go(new PageKey.Collection(tile.Id)), null, Shell.FocusRail, focusItem: Tree.Focus));
            }
            body.Add(column);
            Group.Add(column.Group);
            // Focus where it was (a row that changed keeps its collection), else on the first collection.
            if (focusWasIn && focusedName != null && Find(column.Group, focusedName) is { } again) Tree.Focus(again);
            else if (focusWasIn || TakesFocus()) Tree.Enter(column.Group);
        }
        if (focusWasIn && (Tree.Current == null || !Within(Group, Tree.Current))) ToRail();
        foreach (var view in old) view.Dispose();
    }

    /// <summary>Whether the page may take focus on its own: it has none, or it is still where the page opened.</summary>
    bool TakesFocus()
    {
        var take = Tree.Current == null || (waiting && Tree.Current == waitingOn);
        if (take) waiting = false;
        return take;
    }

    /// <summary>What the rows show, to make them again only when it changed.</summary>
    static string RowsKey(IReadOnlyList<CollectionShelf> shelves) =>
        string.Join("|", shelves.Select(s => s.Key + "=" + s.LibraryName + ":" + string.Join(",",
            s.Collections.Value!.Select(c => $"{c.Id};{c.Tile.Title};{c.Tile.Subtitle};{string.Join(" ", c.Tile.Images)}"))));

    /// <summary>Takes what shows off the page and out of focus's way; the caller lets the views go.</summary>
    List<View> Detach()
    {
        Group.Clear();
        var old = body.Children.ToList();
        foreach (var child in old) body.Remove(child);
        column = null;
        shownRows = "";
        shownMessage = null;
        return old;
    }

    void Clear()
    {
        var hadFocus = Tree.Current != null && Within(Group, Tree.Current);
        var old = Detach();
        if (hadFocus) ToRail();
        foreach (var view in old) view.Dispose();
    }

    /// <summary>
    /// Nothing to show (the Android app's TvMessage at y = 80): the title in Section, the sentence in
    /// Body Muted, 8 apart, and 24 under it Retry as a pill with focus; without one, focus goes to
    /// the rail.
    /// </summary>
    void Message(LibraryMessage m)
    {
        shownMessage = m;
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(RowsTop)) };
        view.Add(Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - 2 * PleyTv.Gutter));
        var words = Text.Label(m.Body, PleyType.Body, PleyColors.Muted, maxLines: 3, width: MessageWidth);
        words.VerticalAlignment = VerticalAlignment.Top;
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        body.Add(view);
        if (m.ActionLabel is { } action)
        {
            var buttons = new ButtonRow();
            buttons.Add(new ButtonView(action, ButtonKind.Pill, model.Retry));
            buttons.PositionY = Ui.Dp(PleyType.Section.Line + 8 + 8 + 16 + 8) + words.GetHeightForWidth(Ui.Dp(MessageWidth));
            view.Add(buttons);
            Group.Add(buttons.Group);
            Tree.Enter(buttons.Group);
        }
        else if (Tree.Current == null || Tree.Current.Parent != Shell.Rail.Group)
        {
            Shell.FocusRail();
        }
        waiting = false;
    }

    static bool Within(FocusGroup group, FocusNode node)
    {
        for (var n = node.Parent; n != null; n = n.Parent)
            if (n == group) return true;
        return false;
    }

    static FocusItem? Find(FocusGroup group, string name)
    {
        foreach (var child in group.Children)
        {
            if (child is FocusItem item && item.Name == name) return item;
            if (child is FocusGroup g && Find(g, name) is { } found) return found;
        }
        return null;
    }
}
