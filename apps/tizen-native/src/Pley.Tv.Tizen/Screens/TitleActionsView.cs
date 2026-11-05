using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Requests;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>What a title's main action does, for <see cref="TitleActionsView"/>.</summary>
sealed record TitleHandlers(Action Request, Action More, Action TellMeToo, Action StopFollowing, Action<long> Open, Action SignInAgain);

/// <summary>A round button after the main action: Details, Your requests, Overview.</summary>
sealed record ExtraButton(string Label, string Icon, Action Press);

/// <summary>
/// A title's one main action as a row of buttons (the Android app's TvTitleActions), the first that
/// applies: Sign in again; a restriction's words; Open; why it was declined; Requested or Following,
/// which say where things are and do nothing (they keep focus, as every button does), with the
/// request's stage and download beside them and Stop following; Tell me too; Open and Request more
/// seasons; Request. At the limit Request is disabled and the allowance says when more may be asked
/// for; without access, a line says so.
///
/// What it says comes under the buttons, the stage's own line first; over them where no button of
/// the main action's shows, and when <c>compact</c> (Discover's hero) only its first line, over them.
/// The extras come after, told whether the main action took the first focus.
/// </summary>
sealed class TitleActionsView : View
{
    /// <summary>The text column: x 96 to 560.</summary>
    const float NotesWidth = 464;

    /// <summary>The widest a stage gets beside a button.</summary>
    const float StageWidth = 260;

    sealed record Note(string Text, string? Icon = null, bool Danger = false);

    sealed record Pill(string Label, string Icon, bool Enabled, Action Press);

    public TitleActionsView(MainAction action, string kind, Allowance? allowance, bool manage, TitleHandlers on, bool compact, Func<bool, IEnumerable<ExtraButton>> extras)
    {
        var notes = new List<Note>();
        Pill? main = null, more = null;
        MediaRequest? stage = null;
        var unwatch = false;
        var noAccess = new Note(RequestText.NoAccessLine, "lock");
        void AllowanceFor(string of)
        {
            if (allowance != null) notes.AddRange(RequestKit.Text.Allowance(allowance, of).Select(line => new Note(line)));
        }
        switch (action)
        {
            case MainAction.SignInAgain:
                main = new Pill(RequestText.SignIn, "log-out", true, on.SignInAgain);
                notes.Add(new Note("This app signed in before it could ask. Signing in again lets it."));
                break;
            case MainAction.Restricted restricted:
                if (!string.IsNullOrWhiteSpace(restricted.Text)) notes.Add(new Note(restricted.Text!, "lock"));
                break;
            case MainAction.Open open:
                main = new Pill(RequestText.Open, "play", open.ItemId != null, () =>
                {
                    if (open.ItemId is { } id) on.Open(id);
                });
                if (open.ItemId == null) notes.Add(new Note(RequestText.ReadyElsewhere, "info"));
                break;
            case MainAction.Declined declined:
                notes.Add(new Note(RequestKit.Text.Declined(declined), "ticket-x", Danger: true));
                break;
            case MainAction.Mine mine:
                main = new Pill(RequestText.Requested, "check", true, () => { });
                stage = mine.Active;
                break;
            case MainAction.Following following:
                main = new Pill(RequestText.Following, "check", true, () => { });
                stage = following.Active;
                unwatch = following.Active?.Actions.Contains(RequestAction.Unwatch) == true;
                break;
            case MainAction.TellMeToo tell:
                if (tell.Ask == Ask.NoAccess) notes.Add(noAccess);
                else
                {
                    main = new Pill(RequestText.TellMeToo, "bookmark-plus", true, on.TellMeToo);
                    notes.Add(new Note(RequestText.AskedAlready));
                }
                break;
            case MainAction.Partial partial:
            {
                var asking = partial.More is Ask.Yes or Ask.Limit;
                var request = new Pill(RequestText.MoreSeasons, "plus", partial.More == Ask.Yes, on.More);
                if (partial.ItemId is { } itemId)
                {
                    main = new Pill(RequestText.Open, "play", true, () => on.Open(itemId));
                    if (asking) more = request;
                }
                else if (asking) main = request;
                if (partial.More == Ask.NoAccess) notes.Add(noAccess);
                else if (partial.More == Ask.Limit || (partial.More == Ask.Yes && !compact)) AllowanceFor(TitleKind.Show);
                break;
            }
            case MainAction.Request ask:
                if (ask.Ask == Ask.NoAccess) notes.Add(noAccess);
                else
                {
                    main = new Pill(RequestText.Request, "plus", ask.Ask == Ask.Yes, on.Request);
                    if (ask.Ask == Ask.Limit || !compact) AllowanceFor(kind);
                }
                break;
        }
        // The stage's own line comes first: it says more of the pill beside the button.
        if (stage != null && RequestKit.Text.PillLine(RequestRules.Pill(stage, manage)) is { } line) notes.Insert(0, new Note(line));
        var said = compact ? notes.Take(1).ToList() : notes;
        var above = compact || main == null;
        Claimed = main?.Enabled == true;

        Group = new FocusGroup(Axis.Row, "actions");
        var y = 0f;
        if (above && said.Count > 0)
        {
            y = Notes(said, y) + 14;
        }
        var row = new View { PositionY = Ui.Dp(y) };
        Add(row);
        float x = 0;
        void Place(View view, FocusItem? item, float width)
        {
            view.PositionX = Ui.Dp(x);
            row.Add(view);
            if (item != null) Group.Add(item);
            x += width + PleyTv.ButtonGap;
        }
        foreach (var pill in new[] { main, more })
        {
            if (pill == null) continue;
            var button = new ButtonView(pill.Label, ButtonKind.Pill, pill.Press, iconName: pill.Icon) { Enabled = pill.Enabled };
            if (pill == main && pill.Enabled) Main = button.Item;
            Place(button, button.Item, button.WidthDp);
        }
        if (stage != null)
        {
            // Beside a 40 dp button, centred on it.
            var block = RequestKit.StageBlock(stage, manage, StageWidth);
            block.PositionY = (Ui.Dp(PleyTv.ButtonHeight) - block.SizeHeight) / 2;
            Place(block, null, block.SizeWidth / Ui.Scale);
        }
        var labelled = false;
        if (unwatch)
        {
            var stop = new RoundButton(RequestText.StopFollowing, "bookmark-minus", on.StopFollowing);
            Place(stop, stop.Item, PleyTv.ButtonHeight);
            labelled = true;
        }
        foreach (var extra in extras(Claimed))
        {
            var round = new RoundButton(extra.Label, extra.Icon, extra.Press);
            Place(round, round.Item, PleyTv.ButtonHeight);
            Extras.Add(round.Item);
            labelled = true;
        }
        // A round button's label shows 6 dp under it while it has focus. As in the Android app, where
        // the label is part of the button, the row keeps that room, so nothing under it lies beneath
        // the label.
        y += PleyTv.ButtonHeight + (labelled ? 6 + PleyType.Caption.Line : 0);
        if (!above && said.Count > 0) y = Notes(said, y + 12);
        HeightDp = y;
        Size = new Size(Ui.Dp(PleyTv.CanvasWidth - PleyTv.Gutter - PleyTv.SafeHorizontal), Ui.Dp(y));
    }

    /// <summary>The buttons, in their row.</summary>
    public FocusGroup Group { get; }

    /// <summary>The main action's button while it can take focus.</summary>
    public FocusItem? Main { get; }

    /// <summary>The extras' buttons, in their order.</summary>
    public List<FocusItem> Extras { get; } = new List<FocusItem>();

    /// <summary>The main action took the first focus: it has a button that can take it.</summary>
    public bool Claimed { get; }

    public float HeightDp { get; }

    /// <summary>What the action says, muted and close together; where they end.</summary>
    float Notes(IReadOnlyList<Note> notes, float y)
    {
        var first = true;
        foreach (var note in notes)
        {
            if (!first) y += 4;
            first = false;
            var x = 0f;
            if (note.Icon != null)
            {
                var icon = Icons.Icon(note.Icon, 16, note.Danger ? PleyColors.DangerText : PleyColors.Muted);
                icon.Position = new Position(0, Ui.Dp(y + 2));
                Add(icon);
                x = 24;
            }
            var text = PanelColumn.Lines(note.Text, PleyType.Body, PleyColors.Muted, NotesWidth - x, 2);
            text.Position = new Position(Ui.Dp(x), Ui.Dp(y));
            Add(text);
            y += text.SizeHeight / Ui.Scale;
        }
        return y;
    }
}
