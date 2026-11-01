using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Api;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// A playlist's page on TV (design spec §4.9; the Android app's TvPlaylistScreen): the item page's
/// top (the first entry's backdrop; "Video playlist" or "Smart playlist", the title, "12 items ·
/// 3h 12m" with a smart playlist's filter in words; Play with focus, Shuffle, Rename, Delete), then
/// its entries as rows: number, 160 × 90 still (90 × 90 cover for music), title and length, and
/// More. The rows scroll up over the backdrop, which darkens as the top goes.
///
/// OK on an entry plays from it; holding OK, the Menu key or More opens its actions: Play from
/// here, Go to it, Move, Remove. Move lifts the entry: Up and Down move it, OK drops it there (and
/// sends the order), Back puts it back. Smart playlists offer no edits.
/// </summary>
sealed class PlaylistScreen : ShellPage
{
    const float TextColumn = 464, HeroTop = 58, HeroHeight = 436, RowHeight = 106, RowAnchor = 140, ListBottom = 200;
    /// <summary>Under a row of buttons, the room their labels fade into.</summary>
    const float ButtonLabelRoom = 24;
    /// <summary>The page's dimming over the backdrop once its top has scrolled away: Bg at 92 %.</summary>
    const float Dimmed = 0.92f;

    readonly Shell layers;
    readonly PlaylistModel model;
    readonly Images images;
    readonly KeyRepeatGate gate;
    readonly Action<PageKey> go;
    readonly IDisposable watch;
    readonly BackdropView backdrop;
    readonly View dim = new View { BackgroundColor = PleyColors.Bg, Opacity = 0 };
    readonly View content = new View();
    readonly View heroText = new View();
    readonly View buttonsView = new View();
    readonly View rowsView = new View();
    readonly View hint = new View { BackgroundColor = PleyColors.Surface3 };
    readonly FocusGroup buttons = new FocusGroup(Axis.Row, "buttons");
    readonly FocusGroup entries = new FocusGroup(Axis.Column, "entries");
    readonly Dictionary<long, Entry> entryFocus = new Dictionary<long, Entry>();
    readonly Dictionary<long, EntryRowView> rowViews = new Dictionary<long, EntryRowView>();
    readonly Dictionary<FocusItem, PlaylistButton> buttonKinds = new Dictionary<FocusItem, PlaylistButton>();
    PlaylistPage? page;
    string shownKind = "";
    string shownButtons = "";
    string? shownBackdrop;
    FocusItem? deleteButton;
    /// <summary>The lifted entry, while one is being moved.</summary>
    long? moving;
    float scroll;
    Animation? scrolling;
    bool gone;

    public PlaylistScreen(TvShell shell, Shell layers, PlaylistModel model, Images images, KeyRepeatGate gate, Action<PageKey> go)
        : base(shell, new PageKey.Playlist(model.PlaylistId), "playlist")
    {
        this.layers = layers;
        this.model = model;
        this.images = images;
        this.gate = gate;
        this.go = go;
        View.BackgroundColor = PleyColors.Bg;
        backdrop = new BackdropView(images);
        View.Add(backdrop);
        View.Add(new ScrimView(ScrimView.Item));
        dim.Size = new Size(Ui.Dp(PleyTv.CanvasWidth), Ui.Dp(PleyTv.CanvasHeight));
        View.Add(dim);
        heroText.Position = new Position(Ui.Dp(PleyTv.Gutter), 0);
        buttonsView.PositionX = Ui.Dp(PleyTv.Gutter);
        content.Add(heroText);
        content.Add(buttonsView);
        content.Add(rowsView);
        View.Add(content);
        var hintText = Text.Label(PlaylistText.Moving, PleyType.Label, PleyColors.Fg);
        var width = hintText.NaturalSize.Width + Ui.Dp(40);
        hintText.Size = new Size(hintText.NaturalSize.Width + 2, Text.LineHeight(PleyType.Label));
        hintText.Position = new Position(Ui.Dp(20), Ui.Dp(12));
        hint.Add(hintText);
        hint.Size = new Size(width, Ui.Dp(12 * 2) + Text.LineHeight(PleyType.Label));
        hint.CornerRadius = hint.SizeHeight / 2;
        hint.Position = new Position((Ui.Dp(PleyTv.CanvasWidth) - width) / 2, Ui.Dp(PleyTv.CanvasHeight - PleyTv.SafeVertical) - hint.SizeHeight);
        hint.Hide();
        View.Add(hint);

        buttons.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        entries.Exit = d => d == Direction.Left ? Scrolling.Left(gate, Shell.FocusRail) : d == Direction.Right;
        buttons.ChildFocused += _ => ScrollTo(0);
        entries.ChildFocused += i => ScrollTo(HeroHeight + i * RowHeight - RowAnchor);

        model.Told += OnTold;
        model.PlayRequested += OnPlay;
        watch = model.State.Watch(Show);
    }

    public override void Enter()
    {
        if (!Tree.Enter(Group)) Shell.FocusRail();
    }

    /// <summary>Back while an entry is lifted puts it back where it was.</summary>
    public override bool Back()
    {
        if (moving == null) return false;
        moving = null;
        model.CancelMoves();
        Moved();
        return true;
    }

    public override void Gone()
    {
        gone = true;
        watch.Dispose();
        model.Told -= OnTold;
        model.PlayRequested -= OnPlay;
        scrolling?.Stop();
        model.Dispose();
    }

    void OnPlay(PlayRequest request) => Shell.Play?.Invoke(request);

    void OnTold(PlaylistEvent e)
    {
        switch (e)
        {
            case PlaylistEvent.Removed removed:
                Shell.Toast(PlaylistText.Removed(removed.Title));
                break;
            case PlaylistEvent.Deleted:
                // Gone: back to the playlists, unless this page was already left.
                if (Shell.Top == this) Shell.Back?.Invoke();
                break;
            case PlaylistEvent.Open open:
                go(open.Key);
                break;
            case PlaylistEvent.Failed failed:
                Shell.Toast(Errors.Message(failed.Error));
                break;
        }
    }

    void Show(Loadable<PlaylistPage> state)
    {
        if (state.Value is { } ready)
        {
            if (shownKind != "ready") Clear();
            shownKind = "ready";
            ShowPage(ready);
            return;
        }
        if (PlaylistText.PlaylistMessage(state) is { } message)
        {
            shownKind = "message";
            Clear();
            Message(message);
            return;
        }
        if (shownKind == "loading") return;
        shownKind = "loading";
        Clear();
        Skeleton();
    }

    void Clear()
    {
        var hadFocus = InPage(Tree.Current);
        Group.Clear();
        buttons.Clear();
        entries.Clear();
        entryFocus.Clear();
        buttonKinds.Clear();
        foreach (var view in new[] { heroText, buttonsView, rowsView })
            foreach (var child in view.Children.ToList())
            {
                view.Remove(child);
                child.Dispose();
            }
        rowViews.Clear();
        foreach (var child in content.Children.ToList())
        {
            if (child == heroText || child == buttonsView || child == rowsView) continue;
            content.Remove(child);
            child.Dispose();
        }
        page = null;
        shownButtons = "";
        deleteButton = null;
        moving = null;
        hint.Hide();
        scroll = 0;
        scrolling?.Stop();
        content.PositionY = 0;
        dim.Opacity = 0;
        if (hadFocus) Shell.FocusRail();
    }

    bool InPage(FocusNode? node)
    {
        for (var n = node?.Parent; n != null; n = n.Parent)
            if (n == Group) return true;
        return false;
    }

    /// <summary>Still blocks in the top's shapes, while nothing is known.</summary>
    void Skeleton()
    {
        float y = HeroTop;
        void Block(float w, float h, float radius = PleyShape.BadgeRadius)
        {
            content.Add(new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(y)), Size = new Size(Ui.Dp(w), Ui.Dp(h)), CornerRadius = Ui.Dp(radius), BackgroundColor = PleyColors.Surface2 });
            y += h + 14;
        }
        Block(120, 18);
        Block(360, 44);
        Block(200, 18);
        y += 120 + 14;
        Block(128, PleyTv.ButtonHeight, PleyShape.ControlRadius);
    }

    /// <summary>Not found (Back) or failed (Retry): a heading, a sentence and the pill with focus.</summary>
    void Message(LibraryMessage m)
    {
        var view = new View { Position = new Position(Ui.Dp(PleyTv.Gutter), Ui.Dp(HeroTop)) };
        view.Add(Text.Label(m.Title, PleyType.Section, PleyColors.Fg, width: PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal));
        var words = PanelColumn.Lines(m.Body, PleyType.Body, PleyColors.Muted, 520, 3);
        words.PositionY = Ui.Dp(PleyType.Section.Line + 8);
        view.Add(words);
        content.Add(view);
        if (m.ActionLabel is not { } action) return;
        var row = new ButtonRow();
        row.Add(new ButtonView(action, ButtonKind.Pill, () =>
        {
            if (m.Action == MessageAction.Back)
            {
                if (Shell.Top == this) Shell.Back?.Invoke();
            }
            else model.Retry();
        }, iconName: m.Action == MessageAction.Retry ? "refresh-cw" : null));
        row.PositionY = Ui.Dp(PleyType.Section.Line + 8 + 16) + words.SizeHeight;
        view.Add(row);
        row.Group.Exit = d => d == Direction.Left && Shell.FocusRail();
        Group.Add(row.Group);
        if (Shell.Top == this) Tree.Enter(row.Group);
    }

    void ShowPage(PlaylistPage next)
    {
        var first = page == null;
        var before = page;
        page = next;
        if (next.Backdrop.FirstOrDefault() != shownBackdrop)
        {
            shownBackdrop = next.Backdrop.FirstOrDefault();
            backdrop.Show(next.Backdrop);
        }
        if (first)
        {
            Group.Add(buttons);
            Group.Add(entries);
        }
        var lost = SyncEntries(next);
        ShowHero(next);
        ShowButtons(next, first);
        if (moving != null) hint.Show();
        else hint.Hide();
        foreach (var e in entryFocus.Values) e.More.Enabled = moving == null;
        // Focus was on an entry that has gone: the one below, else above; Delete when none is left.
        if (lost is { } went)
        {
            var standIn = before == null ? null : PlaylistRules.StandIn(before.Rows, went);
            if (standIn is { } id && entryFocus.TryGetValue(id, out var entry)) Tree.Focus(entry.Pill);
            else if (deleteButton != null) Tree.Focus(deleteButton);
        }
        // The lifted entry moved: the page follows it.
        if (moving is { } lifted && IndexOf(lifted) is var at && at >= 0)
        {
            entries.RememberChild(at);
            ScrollTo(HeroHeight + at * RowHeight - RowAnchor);
        }
        else ScrollTo(scroll);
    }

    int IndexOf(long entryId)
    {
        var rows = page?.Rows;
        if (rows == null) return -1;
        for (var i = 0; i < rows.Count; i++)
            if (rows[i].EntryId == entryId) return i;
        return -1;
    }

    /// <summary>
    /// Each entry keeps its focus items while it is in the playlist, so focus stays on it as it
    /// moves; the rows' group follows the order shown. Answers the entry that had focus and went.
    /// </summary>
    long? SyncEntries(PlaylistPage next)
    {
        var current = Tree.Current;
        long? lost = null;
        var ids = new HashSet<long>(next.Rows.Select(r => r.EntryId));
        foreach (var e in entryFocus.Values.ToList())
        {
            if (ids.Contains(e.Id)) continue;
            if (current == e.Pill || current == e.More) lost = e.Id;
            entryFocus.Remove(e.Id);
            if (rowViews.TryGetValue(e.Id, out var view))
            {
                rowViews.Remove(e.Id);
                view.Unparent();
                view.Dispose();
            }
        }
        var last = entries.Last;
        entries.Clear();
        foreach (var row in next.Rows)
        {
            if (!entryFocus.TryGetValue(row.EntryId, out var e))
            {
                e = NewEntry(row.EntryId);
                entryFocus[row.EntryId] = e;
            }
            entries.Add(e.Group);
        }
        entries.RememberChild(Math.Min(last, next.Rows.Count - 1));
        return lost;
    }

    Entry NewEntry(long id)
    {
        var group = new FocusGroup(Axis.Row, "entry/" + id) { Remember = false };
        var pill = new FocusItem("entry/" + id);
        var more = new FocusItem(PlaylistText.More);
        pill.Selected = () =>
        {
            var at = IndexOf(id);
            if (at >= 0) model.Play(from: at);
        };
        pill.LongSelected = () =>
        {
            if (moving == null && Row(id) is { } row) Actions(row);
        };
        pill.Menu = pill.LongSelected;
        pill.OwnKey = (key, down) => moving == id && Lifted(id, key, down);
        more.Selected = () =>
        {
            if (moving == null && Row(id) is { } row) Actions(row);
        };
        group.Add(pill);
        group.Add(more);
        return new Entry(id, group, pill, more);
    }

    PlaylistRow? Row(long entryId) => page?.Rows.FirstOrDefault(r => r.EntryId == entryId);

    /// <summary>Keys on the lifted entry: Up and Down move it, OK drops it, Left, Right and Menu do nothing.</summary>
    bool Lifted(long id, string key, bool down)
    {
        switch (key)
        {
            case "Up":
            case "Down":
                if (down) model.MoveBy(id, key == "Up" ? -1 : 1, send: false);
                return true;
            case "Return":
            case "KP_Enter":
                if (!down)
                {
                    moving = null;
                    model.SendOrder();
                    Moved();
                }
                return true;
            case "Left":
            case "Right":
            case "XF86SysMenu":
            case "XF86Menu":
            case "Menu":
                return true;
            default:
                return false;
        }
    }

    /// <summary>An entry was lifted or let go: the rows, More and the hint show it.</summary>
    void Moved()
    {
        if (moving != null) hint.Show();
        else hint.Hide();
        foreach (var e in entryFocus.Values) e.More.Enabled = moving == null;
        DrawRows(true);
    }

    /// <summary>
    /// The top's text: the kind with its icon, the title, the Smart badge and the facts; then the
    /// buttons at the top's foot and, with no entries, the line saying how to add some.
    /// </summary>
    void ShowHero(PlaylistPage p)
    {
        foreach (var child in heroText.Children.ToList())
        {
            heroText.Remove(child);
            child.Dispose();
        }
        float y = HeroTop;
        var icon = Icons.Icon(p.Video ? "list-video" : "list-music", 18, PleyColors.Muted);
        icon.PositionY = Ui.Dp(y);
        heroText.Add(icon);
        var kind = Text.Label(PlaylistText.KindLine(p), PleyType.Caption, PleyColors.Muted, width: TextColumn - 26);
        kind.Position = new Position(Ui.Dp(26), Ui.Dp(y + 9) - Text.LineHeight(PleyType.Caption) / 2);
        heroText.Add(kind);
        y += 18 + 10;
        var title = PanelColumn.Lines(p.Title, PleyType.HeroTitle, PleyColors.Fg, TextColumn, 2);
        title.PositionY = Ui.Dp(y);
        heroText.Add(title);
        y += title.SizeHeight / Ui.Scale + 12;
        float x = 0;
        if (p.Smart)
        {
            var badge = Badge(PlaylistText.SmartBadge);
            badge.Position = new Position(0, Ui.Dp(y));
            heroText.Add(badge);
            x = badge.SizeWidth / Ui.Scale + 8;
        }
        var facts = PlaylistText.FactsLine(p);
        if (facts.Length > 0)
        {
            var line = PanelColumn.Lines(facts, PleyType.Label, PleyColors.Muted, TextColumn - x, 2);
            line.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            heroText.Add(line);
        }

        var emptyHeight = 0f;
        if (PlaylistText.EmptyLine(p) is { } empty)
        {
            var words = PanelColumn.Lines(empty, PleyType.Body, PleyColors.Muted, TextColumn, 3);
            emptyHeight = words.SizeHeight / Ui.Scale;
            words.PositionY = Ui.Dp(HeroHeight - emptyHeight);
            heroText.Add(words);
        }
        buttonsView.PositionY = Ui.Dp(HeroHeight - emptyHeight - ButtonLabelRoom - 12 - PleyTv.ButtonHeight);
    }

    /// <summary>A quality badge's look ("Smart"): Badge type in a 1 dp Control edge.</summary>
    static View Badge(string text)
    {
        var t = Text.Label(text, PleyType.Badge, PleyColors.Fg);
        var box = new View
        {
            Size = new Size(t.NaturalSize.Width + Ui.Dp(12), Ui.Dp(18)),
            BorderlineWidth = Ui.Dp(1),
            BorderlineColor = PleyColors.Control,
            BorderlineOffset = -1,
            CornerRadius = Ui.Dp(PleyShape.BadgeRadius),
        };
        t.Size = new Size(box.SizeWidth, box.SizeHeight);
        t.HorizontalAlignment = HorizontalAlignment.Center;
        box.Add(t);
        return box;
    }

    /// <summary>
    /// Play (a pill), Shuffle, Rename and Delete (round), made again only when which can be pressed
    /// changes. The first time, focus goes where the page starts: Play, else Rename, else Delete.
    /// </summary>
    void ShowButtons(PlaylistPage p, bool first)
    {
        var list = PlaylistText.Buttons(p);
        var signature = string.Join(",", list.Select(b => $"{b.Button}:{b.Enabled}"));
        if (signature == shownButtons) return;
        shownButtons = signature;
        PlaylistButton? focusedKind = Tree.Current != null && buttonKinds.TryGetValue(Tree.Current, out var k) ? k : null;
        buttonKinds.Clear();
        // Let go only once focus is on their replacements: the one with focus hears it leave.
        var old = buttonsView.Children.ToList();
        foreach (var child in old) buttonsView.Remove(child);
        var last = buttons.Last;
        buttons.Clear();
        float x = 0;
        FocusItem? start = null;
        FocusItem? again = null;
        var startKind = PlaylistText.FirstFocus(p);
        foreach (var b in list)
        {
            var kind = b.Button;
            View view;
            FocusItem item;
            if (b.Button == PlaylistButton.Play)
            {
                var pill = new ButtonView(b.Label, ButtonKind.Pill, () => Press(kind), iconName: b.Icon) { Enabled = b.Enabled };
                view = pill;
                item = pill.Item;
            }
            else
            {
                var round = new RoundButton(b.Label, b.Icon, () => Press(kind));
                round.Item.Enabled = b.Enabled;
                if (!b.Enabled) round.Opacity = 0.4f;
                view = round;
                item = round.Item;
            }
            buttonKinds[item] = kind;
            view.PositionX = Ui.Dp(x);
            buttonsView.Add(view);
            buttons.Add(item);
            x += view.SizeWidth / Ui.Scale + PleyTv.ButtonGap;
            if (kind == startKind) start = item;
            if (kind == focusedKind && b.Enabled) again = item;
            if (kind == PlaylistButton.Delete) deleteButton = item;
        }
        if (first && start != null)
        {
            buttons.RememberChild(buttons.Children.ToList().IndexOf(start));
            Group.RememberChild(0);
            if (Shell.Top == this) Tree.Focus(start);
        }
        else
        {
            buttons.RememberChild(Math.Min(last, buttons.Children.Count - 1));
            if (again != null) Tree.Focus(again);
            // The focused button can't be pressed now (Play, once the last entry went): where the page starts.
            else if (focusedKind != null && start != null) Tree.Focus(start);
        }
        foreach (var child in old) child.Dispose();
    }

    void Press(PlaylistButton button)
    {
        var p = page;
        if (p == null) return;
        switch (button)
        {
            case PlaylistButton.Play:
                model.Play();
                break;
            case PlaylistButton.Shuffle:
                model.Play(shuffle: true);
                break;
            case PlaylistButton.Rename:
                Rename(p);
                break;
            case PlaylistButton.Delete:
                Delete(p);
                break;
        }
    }

    /// <summary>
    /// Scrolls the page to <paramref name="target"/> (clamped to what the list holds): the top
    /// and the rows move up, and the backdrop darkens as the top goes.
    /// </summary>
    void ScrollTo(float target)
    {
        var count = page?.Rows.Count ?? 0;
        var most = Math.Max(0, HeroHeight + count * RowHeight + ListBottom - PleyTv.CanvasHeight);
        target = Math.Max(0, Math.Min(target, most));
        var changed = target != scroll;
        scroll = target;
        DrawRows(false);
        if (!changed) return;
        scrolling?.Stop();
        scrolling = Scrolling.Glide(gate);
        scrolling.AnimateTo(content, "PositionY", -Ui.Dp(target));
        scrolling.AnimateTo(dim, "Opacity", Math.Min(1f, target / HeroHeight) * Dimmed);
        scrolling.Play();
    }

    /// <summary>
    /// The rows on screen and two either side have views; the others go. A row is drawn again when
    /// its entry, place or lift changed (<paramref name="all"/>: every one, an entry lifted or let go).
    /// </summary>
    void DrawRows(bool all)
    {
        var p = page;
        if (p == null || gone) return;
        var first = Math.Max(0, (int)Math.Floor((scroll - HeroHeight) / RowHeight) - 2);
        var last = Math.Min(p.Rows.Count - 1, (int)Math.Ceiling((scroll + PleyTv.CanvasHeight - HeroHeight) / RowHeight) + 2);
        var keep = new HashSet<long>();
        for (var i = first; i <= last; i++) keep.Add(p.Rows[i].EntryId);
        foreach (var id in rowViews.Keys.ToList())
        {
            if (keep.Contains(id)) continue;
            rowViews[id].Unparent();
            rowViews[id].Dispose();
            rowViews.Remove(id);
        }
        for (var i = first; i <= last; i++)
        {
            var row = p.Rows[i];
            var lifted = moving == row.EntryId;
            if (rowViews.TryGetValue(row.EntryId, out var view))
            {
                if (!all && Same(view.Row, row) && view.Index == i && view.Lifted == lifted) continue;
                view.Unparent();
                view.Dispose();
            }
            if (!entryFocus.TryGetValue(row.EntryId, out var entry)) continue;
            view = new EntryRowView(row, i, p.Video, lifted, entry, images);
            view.Position = new Position(Ui.Dp(PleyTv.Gutter - 16), Ui.Dp(HeroHeight + i * RowHeight));
            rowsView.Add(view);
            rowViews[row.EntryId] = view;
        }
    }

    /// <summary>Whether a row would be drawn the same: each answer makes new rows, most of them unchanged.</summary>
    static bool Same(PlaylistRow a, PlaylistRow b) =>
        a.EntryId == b.EntryId && a.Title == b.Title && a.Subtitle == b.Subtitle && a.Duration == b.Duration
        && a.Tile.Progress == b.Tile.Progress && a.Tile.Images.SequenceEqual(b.Tile.Images);

    /// <summary>An entry's actions in a side panel: its title and line, then Play from here, Go to it, and Move and Remove for a playlist that isn't smart.</summary>
    void Actions(PlaylistRow row)
    {
        var p = page;
        if (p == null) return;
        var sheet = new PanelSheet(layers);
        sheet.Column.Title(row.Title, row.Subtitle);
        foreach (var action in PlaylistText.EntryActions(row, p))
        {
            var chosen = action.Action;
            var view = sheet.Column.Choice(new PanelChoice(action.Label, () =>
            {
                sheet.Close();
                Act(chosen, row);
            }, action.Icon, danger: action.Danger));
            view.Enabled = action.Enabled;
        }
        sheet.Show();
    }

    void Act(EntryAction action, PlaylistRow row)
    {
        switch (action)
        {
            case EntryAction.PlayFromHere:
                model.Play(from: Math.Max(0, IndexOf(row.EntryId)));
                break;
            case EntryAction.GoTo:
                go(model.Opens(row));
                break;
            case EntryAction.Move:
                if (!entryFocus.TryGetValue(row.EntryId, out var entry)) return;
                moving = row.EntryId;
                Tree.Focus(entry.Pill);
                Moved();
                break;
            case EntryAction.Remove:
                // Gone at once: focus goes to the entry below, else above, else Delete (ShowPage).
                model.Remove(row.EntryId, undoable: false);
                Shell.Toast(PlaylistText.Removed(row.Title));
                break;
        }
    }

    /// <summary>Rename: the field with the title as it is (OK types), its hint, and Save once the title will do.</summary>
    void Rename(PlaylistPage p)
    {
        var sheet = new PanelSheet(layers);
        var column = sheet.Column;
        column.Title(PlaylistText.RenameTitle);
        var field = new TextInput(layers.Input, PanelColumn.Inner, PlaylistText.FormTitle, action: InputMethod.ActionButtonTitleType.Done) { Value = p.Title };
        column.Place(field, TextInput.Height, 0, 12, field.Item);
        column.Sentence(PlaylistText.FormHint, PleyType.Caption, PleyColors.Muted, 4, 12);
        void Save()
        {
            if (model.Rename(field.Value)) sheet.Close();
        }
        // 4 in from the edge, so the pill's focus growth isn't cut off.
        var save = new ButtonView(PlaylistText.Save, ButtonKind.Pill, Save);
        column.Place(save, PleyTv.ButtonHeight, 4, 12 + 4 + 12, save.Item);
        save.Enabled = PlaylistRules.Check(field.Value) == null;
        field.Changed += text => save.Enabled = PlaylistRules.Check(text) == null;
        field.Submitted = () =>
        {
            if (PlaylistRules.Check(field.Value) == null) Save();
        };
        sheet.Show(field.Item);
    }

    /// <summary>Delete, asked first: Cancel has focus; Delete reads as a danger.</summary>
    void Delete(PlaylistPage p)
    {
        var dialog = new Dialog(layers, PlaylistText.DeleteTitle, PlaylistText.DeleteBody(p.Title));
        dialog.Button(PlaylistText.Cancel, dialog.Close, initial: true);
        dialog.Button(PlaylistText.Delete, () =>
        {
            dialog.Close();
            model.Delete();
        }, PleyColors.DangerText);
        dialog.Show();
    }

    /// <summary>An entry's focus: the row itself and its More, side by side.</summary>
    sealed record Entry(long Id, FocusGroup Group, FocusItem Pill, FocusItem More);

    /// <summary>
    /// One entry as a row (design spec §2.7: rows fill without scaling): its number, its still,
    /// its two lines and its length; focused, an accent fill with OnAccent ink. Lifted, Surface3
    /// with the AccentBright ring, and up and down chevrons in place of the length. More sits 12
    /// to its right.
    /// </summary>
    sealed class EntryRowView : View
    {
        const float Width = PleyTv.CanvasWidth - PleyTv.SafeHorizontal - (PleyTv.Gutter - 16), MoreGap = 12;
        const float Fill = Width - MoreGap - PleyTv.ButtonHeight;

        readonly Entry entry;
        readonly View fill;
        readonly TextLabel number;
        readonly TextLabel title;
        readonly TextLabel? subtitle;
        readonly TextLabel? duration;
        readonly MoreButton more;

        public EntryRowView(PlaylistRow row, int index, bool video, bool lifted, Entry entry, Images images)
        {
            Row = row;
            Index = index;
            Lifted = lifted;
            this.entry = entry;
            Size = new Size(Ui.Dp(Width), Ui.Dp(RowHeight));
            fill = new View { Size = new Size(Ui.Dp(Fill), Ui.Dp(RowHeight)), CornerRadius = Ui.Dp(PleyShape.CardRadius) };
            if (lifted)
            {
                fill.BorderlineWidth = Ui.Dp(PleyShape.RingStroke);
                fill.BorderlineColor = PleyColors.AccentBright;
                fill.BorderlineOffset = -1;
            }
            Add(fill);
            number = Text.Label((index + 1).ToString(System.Globalization.CultureInfo.InvariantCulture), PleyType.Label, PleyColors.Subtle, width: 28);
            number.HorizontalAlignment = HorizontalAlignment.End;
            number.Position = new Position(Ui.Dp(16), (Ui.Dp(RowHeight) - number.SizeHeight) / 2);
            fill.Add(number);
            var thumbWidth = video ? 160f : 90f;
            var thumb = new Thumb(row.Tile, thumbWidth, 90, images);
            thumb.Position = new Position(Ui.Dp(16 + 28 + 16), Ui.Dp((RowHeight - 90) / 2));
            fill.Add(thumb);
            var x = 16 + 28 + 16 + thumbWidth + 20;
            var right = Fill - 16;
            if (lifted)
            {
                var up = Icons.Icon("chevron-up", 22, PleyColors.AccentBright);
                var down = Icons.Icon("chevron-down", 22, PleyColors.AccentBright);
                up.Position = new Position(Ui.Dp(right - 22), Ui.Dp(RowHeight / 2 - 22));
                down.Position = new Position(Ui.Dp(right - 22), Ui.Dp(RowHeight / 2));
                fill.Add(up);
                fill.Add(down);
                right -= 22;
            }
            else if (row.Duration.Length > 0)
            {
                duration = Text.Label(row.Duration, PleyType.Caption, PleyColors.Muted);
                var w = duration.NaturalSize.Width / Ui.Scale + 1;
                duration.Size = new Size(Ui.Dp(w), Text.LineHeight(PleyType.Caption));
                duration.Position = new Position(Ui.Dp(right - w), (Ui.Dp(RowHeight) - duration.SizeHeight) / 2);
                fill.Add(duration);
                right -= w + 16;
            }
            var room = Math.Max(40, right - x);
            title = Text.Label(row.Title, PleyType.Title, PleyColors.Fg, width: room);
            var block = title.SizeHeight;
            if (row.Subtitle.Length > 0)
            {
                subtitle = Text.Label(row.Subtitle, PleyType.Caption, PleyColors.Muted, width: room);
                block += subtitle.SizeHeight;
            }
            var top = (Ui.Dp(RowHeight) - block) / 2;
            title.Position = new Position(Ui.Dp(x), top);
            fill.Add(title);
            if (subtitle != null)
            {
                subtitle.Position = new Position(Ui.Dp(x), top + title.SizeHeight);
                fill.Add(subtitle);
            }
            // More's 40 dp box sits a little low, with the room for its label under it (as on Android).
            more = new MoreButton(entry.More);
            more.Position = new Position(Ui.Dp(Fill + MoreGap), Ui.Dp((RowHeight - PleyTv.ButtonHeight - ButtonLabelRoom / 2) / 2 + ButtonLabelRoom / 2));
            Add(more);
            entry.Pill.FocusChanged += Look;
            Look(entry.Pill.Focused);
        }

        public PlaylistRow Row { get; }
        public int Index { get; }
        public bool Lifted { get; }

        void Look(bool focused)
        {
            // Let go already: a focus change can still reach it on its way to the view in its place.
            if (Disposed) return;
            var accent = focused && !Lifted;
            fill.BackgroundColor = Lifted ? PleyColors.Surface3 : accent ? PleyColors.Accent : Color.Transparent;
            title.TextColor = accent ? PleyColors.OnAccent : PleyColors.Fg;
            var secondary = accent ? PleyColors.OnAccent : PleyColors.Muted;
            if (subtitle != null) subtitle.TextColor = secondary;
            if (duration != null) duration.TextColor = secondary;
            number.TextColor = accent ? PleyColors.OnAccent : PleyColors.Subtle;
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            entry.Pill.FocusChanged -= Look;
            base.Dispose(type);
        }
    }

    /// <summary>An entry's still or cover: its picture over the title's own gradient, and how much was played along its foot.</summary>
    sealed class Thumb : View
    {
        public Thumb(TileModel tile, float width, float height, Images images)
        {
            var w = Ui.Dp(width);
            var h = Ui.Dp(height);
            Size = new Size(w, h);
            CornerRadius = Ui.Dp(PleyShape.TileRadius);
            ClippingMode = ClippingModeType.ClipChildren;
            var (from, to) = Placeholder.Colors(tile.Title);
            Background = Gradients.Diagonal(AvatarView.Rgb(from), AvatarView.Rgb(to));
            var picture = new ImageView
            {
                Size = new Size(w, h),
                DesiredWidth = (int)w,
                DesiredHeight = (int)h,
                FittingMode = FittingModeType.ScaleToFill,
                Opacity = 0,
            };
            Add(picture);
            if (tile.Progress > 0)
            {
                var bar = Ui.Dp(4);
                Add(new View { Size = new Size(w, bar), Position = new Position(0, h - bar), BackgroundColor = PleyColors.ProgressTrackOnArt });
                Add(new View { Size = new Size(w * Math.Min(1f, tile.Progress), bar), Position = new Position(0, h - bar), BackgroundColor = PleyColors.Accent });
            }
            if (tile.Images.Count == 0) return;
            images.Load(tile.Images, width, file =>
            {
                if (file == null || Disposed) return;
                picture.ResourceUrl = file;
                var fade = new Animation(PleyMotion.ImageFadeMs);
                fade.AnimateTo(picture, "Opacity", 1f);
                fade.Play();
            });
        }
    }

    /// <summary>
    /// More at a row's end (the Android app's TvRoundButton off art): a 40 dp Surface3 square with
    /// the Control edge and the ellipsis, its label fading in 6 dp under it while it has focus.
    /// Its focus item is the entry's, which outlives the row's view.
    /// </summary>
    sealed class MoreButton : View
    {
        readonly FocusItem item;
        readonly View box;
        readonly ImageView icon;
        readonly TextLabel label;
        Animation? focusAnimation;

        public MoreButton(FocusItem item)
        {
            this.item = item;
            Size = new Size(Ui.Dp(PleyTv.ButtonHeight), Ui.Dp(PleyTv.ButtonHeight));
            box = new View
            {
                Size = new Size(Ui.Dp(PleyTv.ButtonHeight), Ui.Dp(PleyTv.ButtonHeight)),
                CornerRadius = Ui.Dp(PleyShape.ControlRadius),
                BorderlineWidth = Ui.Dp(1),
                BorderlineOffset = -1,
                PivotPoint = global::Tizen.NUI.PivotPoint.Center,
            };
            icon = Icons.Icon("ellipsis", PleyTv.IconSize, PleyColors.Fg);
            icon.Position = new Position(Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2), Ui.Dp((PleyTv.ButtonHeight - PleyTv.IconSize) / 2));
            box.Add(icon);
            Add(box);
            label = Text.Label(PlaylistText.More, PleyType.Caption, PleyColors.Fg, width: 120);
            label.HorizontalAlignment = HorizontalAlignment.Center;
            label.Position = new Position(Ui.Dp(PleyTv.ButtonHeight / 2 - 60), Ui.Dp(PleyTv.ButtonHeight + 6));
            label.Opacity = 0;
            Add(label);
            Opacity = item.Enabled ? 1f : 0.4f;
            item.FocusChanged += Look;
            Look(item.Focused);
        }

        void Look(bool focused)
        {
            if (Disposed) return;
            box.BackgroundColor = focused ? PleyColors.Accent : PleyColors.Surface3;
            box.BorderlineColor = focused ? Color.Transparent : PleyColors.Control;
            icon.Color = focused ? PleyColors.OnAccent : PleyColors.Fg;
            focusAnimation?.Stop();
            var scale = focused ? PleyShape.RoundScale : 1f;
            focusAnimation = new Animation(focused ? PleyMotion.FocusMs : PleyMotion.UnfocusMs)
            {
                DefaultAlphaFunction = new AlphaFunction(focused ? AlphaFunction.BuiltinFunctions.EaseOut : AlphaFunction.BuiltinFunctions.EaseIn),
            };
            focusAnimation.AnimateTo(box, "Scale", new Vector3(scale, scale, 1));
            focusAnimation.AnimateTo(label, "Opacity", focused ? 1f : 0f);
            focusAnimation.Play();
        }

        protected override void Dispose(DisposeTypes type)
        {
            if (Disposed) return;
            item.FocusChanged -= Look;
            focusAnimation?.Stop();
            base.Dispose(type);
        }
    }
}
