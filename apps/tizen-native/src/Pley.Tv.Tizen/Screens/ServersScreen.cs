using System;
using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core;
using Pley.Tv.Core.Auth;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// "Choose your server" (design spec §4.7; the Android app's TvServersScreen): saved servers first,
/// then those that answer as they come; "Enter an address" (the address pad) and "Search this
/// network". Holding OK on a saved server forgets it.
/// </summary>
sealed class ServersScreen : Sheet
{
    const float Left = 96, Top = 27, Right = 48, RowWidth = 520, RowHeight = 64, RowGap = 4, ListMaxHeight = 300;
    /// <summary>A sweep takes this long at most (<see cref="Servers.SweepTotalMs"/>): its bar fills over it.</summary>
    const int SweepMs = Servers.SweepTotalMs;

    readonly Shell shell;
    readonly ServersModel model;
    readonly View list;
    readonly View listContent;
    readonly FocusGroup listGroup = new FocusGroup(Axis.Column, "servers");
    readonly ButtonRow buttons = new ButtonRow(12);
    readonly ButtonView enterAddress;
    readonly ButtonView searchNetwork;
    readonly View status;
    readonly TextLabel forgetHint;
    readonly List<ServerRowView> rows = new List<ServerRowView>();
    IReadOnlyList<ServerRow> shownRows = Array.Empty<ServerRow>();
    readonly IDisposable watch;
    readonly IDisposable anyKey;
    AddressPad? pad;
    bool touched;
    bool sweepShown;
    float listScroll;

    public ServersScreen(Shell shell, ServersModel model) : base("servers-screen")
    {
        this.shell = shell;
        this.model = model;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(Onboarding.BrandGlow());

        var title = Text.Label("Choose your server", PleyType.PageTitle, PleyColors.Fg, width: 600);
        title.SizeHeight = Ui.Dp(38);
        title.Position = new Position(Ui.Dp(Left), Ui.Dp(Top));
        View.Add(title);
        var logo = Onboarding.Logo(32);
        logo.Position = new Position(Ui.Dp(PleyTv.CanvasWidth - Right) - logo.SizeWidth, Ui.Dp(Top + 3));
        View.Add(logo);

        // The list scrolls inside its 300 dp; rows are drawn without scale, so nothing spills.
        list = new View
        {
            Position = new Position(Ui.Dp(Left), Ui.Dp(Top + 38 + 25)),
            Size = new Size(Ui.Dp(RowWidth), 0),
            ClippingMode = ClippingModeType.ClipChildren,
        };
        listContent = new View { WidthResizePolicy = ResizePolicyType.FillToParent };
        list.Add(listContent);
        View.Add(list);

        enterAddress = buttons.Add(new ButtonView("Enter an address", ButtonKind.Text, OpenPad));
        searchNetwork = buttons.Add(new ButtonView("Search this network", ButtonKind.Text, () => _ = model.SearchNetwork()));
        buttons.PositionX = Ui.Dp(Left);
        View.Add(buttons);

        status = new View { PositionX = Ui.Dp(Left), Size = new Size(Ui.Dp(RowWidth), 0) };
        View.Add(status);

        forgetHint = Text.Label("Hold OK on a saved server to forget it.", PleyType.Caption, PleyColors.Subtle, width: 600);
        forgetHint.Position = new Position(Ui.Dp(Left), Ui.Dp(PleyTv.CanvasHeight - Top - PleyType.Caption.Line));
        View.Add(forgetHint);

        Root.Add(listGroup);
        Root.Add(buttons.Group);
        listGroup.ChildFocused += ScrollTo;

        // D-pad rule 1 holds until the first key: focus follows the first server as it shows.
        anyKey = shell.Input.Push((key, down) =>
        {
            if (shell.Top == this) touched = true;
            return false;
        });
        model.Chosen += OnChosen;
        watch = model.State.Watch(Show);
        Gone += () =>
        {
            watch.Dispose();
            anyKey.Dispose();
            model.Chosen -= OnChosen;
            model.Dispose();
        };
        model.Start();
    }

    /// <summary>Where the app goes once a server is chosen.</summary>
    public Action<ChosenServer>? Chosen { get; set; }

    void OnChosen(ChosenServer chosen) => Chosen?.Invoke(chosen);

    void OpenPad()
    {
        pad = new AddressPad(shell, model);
        pad.Gone += () => pad = null;
        pad.Show();
    }

    void Show(ServersState state)
    {
        if (!state.Rows.SequenceEqual(shownRows)) ShowRows(state.Rows);
        foreach (var r in rows) r.SetConnecting(state.Busy == r.Row.Url);
        searchNetwork.Enabled = !state.Sweeping;
        // Disabled while it has focus: focus goes to its neighbour, so it always shows.
        if (state.Sweeping && Tree.Current == searchNetwork.Item) Tree.Focus(enterAddress.Item);
        if (state.Rows.Any(r => r.Saved)) forgetHint.Show();
        else forgetHint.Hide();
        Layout(state);
        if (!touched)
        {
            if (rows.Count > 0) Tree.Focus(rows[0].Item);
            else Tree.Focus(enterAddress.Item);
        }
    }

    void ShowRows(IReadOnlyList<ServerRow> next)
    {
        var focusedUrl = rows.FirstOrDefault(r => r.Item.Focused)?.Row.Url;
        foreach (var r in rows)
        {
            r.Unparent();
            r.Dispose();
        }
        rows.Clear();
        listGroup.Clear();
        shownRows = next;
        for (var i = 0; i < next.Count; i++)
        {
            var row = next[i];
            var view = new ServerRowView(row, () => _ = model.Choose(row.Url), row.Saved ? () => AskForget(row) : null)
            {
                PositionY = Ui.Dp(i * (RowHeight + RowGap)),
            };
            rows.Add(view);
            listContent.Add(view);
            listGroup.Add(view.Item);
        }
        listContent.SizeHeight = Ui.Dp(Math.Max(0, next.Count * (RowHeight + RowGap) - RowGap));
        list.SizeHeight = Math.Min(listContent.SizeHeight, Ui.Dp(ListMaxHeight));
        if (focusedUrl != null)
        {
            // The row focus was on: again, or the first server, or Enter an address when it went.
            var again = rows.FirstOrDefault(r => r.Row.Url == focusedUrl) ?? rows.FirstOrDefault();
            Tree.Focus(again != null ? again.Item : enterAddress.Item);
        }
    }

    /// <summary>Keeps the focused row inside the list's 300 dp.</summary>
    void ScrollTo(int index)
    {
        var top = index * (RowHeight + RowGap);
        var view = ListMaxHeight;
        if (top < listScroll) listScroll = top;
        else if (top + RowHeight > listScroll + view) listScroll = top + RowHeight - view;
        listContent.PositionY = -Ui.Dp(listScroll);
    }

    void Layout(ServersState state)
    {
        var y = Top + 38 + 25 + list.SizeHeight / Ui.Scale + (rows.Count > 0 ? 16 : 0);
        buttons.PositionY = Ui.Dp(y);
        y += PleyTv.ButtonHeight + 16;
        status.PositionY = Ui.Dp(y);
        ShowStatus(state);
    }

    /// <summary>Under the buttons: searching, none found, the sweep's progress, or what went wrong.</summary>
    void ShowStatus(ServersState state)
    {
        foreach (var child in status.Children.ToList())
        {
            if (child is SweepBar) continue;
            status.Remove(child);
            child.Dispose();
        }
        var bar = status.Children.OfType<SweepBar>().FirstOrDefault();
        float y = 0;
        void Line(View v, float heightDp)
        {
            v.PositionY = Ui.Dp(y);
            status.Add(v);
            y += heightDp + 8;
        }
        if (state.Sweeping)
        {
            var line = new StatusLine(RowWidth, PleyType.Caption);
            line.Set(state.SweepRange != null ? $"Checking {state.SweepRange}…" : "Checking this network…");
            Line(line, PleyType.Caption.Line);
            if (bar == null)
            {
                bar = new SweepBar();
                status.Add(bar);
            }
            bar.PositionY = Ui.Dp(y);
            y += 4 + 8;
            if (!sweepShown) bar.Run(SweepMs);
            sweepShown = true;
        }
        else
        {
            sweepShown = false;
            if (bar != null)
            {
                status.Remove(bar);
                bar.Dispose();
            }
            if (state.Searching)
            {
                var line = new StatusLine(RowWidth);
                line.Set("Looking for Pley on your network…", waiting: true);
                Line(line, PleyType.Label.Line);
            }
            else if (state.Searched && state.Rows.Count == 0 && state.Problem == null)
            {
                Line(Paragraph("No servers found. Make sure this TV and the server are on the same network.", PleyColors.Muted, out var h), h);
            }
        }
        if (state.Problem != null) Line(Paragraph(state.Problem.Text, PleyColors.DangerText, out var h), h);
    }

    static TextLabel Paragraph(string text, Color color, out float heightDp)
    {
        var label = Text.Label(text, PleyType.Body, color, maxLines: 3, width: RowWidth);
        label.VerticalAlignment = VerticalAlignment.Top;
        var height = Math.Min(label.GetHeightForWidth(Ui.Dp(RowWidth)), Ui.Dp(PleyType.Body.Line * 3));
        label.SizeHeight = height;
        heightDp = height / Ui.Scale;
        return label;
    }

    void AskForget(ServerRow row)
    {
        var dialog = new Dialog(shell, $"Forget {row.Name}?", "Pley signs this TV out of it and deletes what it saved from it.");
        dialog.Button("Cancel", dialog.Close, initial: true);
        dialog.Button("Forget", () =>
        {
            dialog.Close();
            _ = model.Forget(row.Url);
        }, PleyColors.DangerText);
        dialog.Show();
    }
}

/// <summary>One server: a 64 dp row, 520 wide; focused, an Accent fill without scale (rows in lists don't scale).</summary>
sealed class ServerRowView : View
{
    readonly View iconBox;
    readonly ImageView icon;
    readonly TextLabel name;
    readonly TextLabel line;
    readonly TextLabel? tag;
    readonly View? dot;
    readonly bool tagWarns;
    bool connecting;

    public ServerRowView(ServerRow row, Action onChoose, Action? onForget)
    {
        Row = row;
        Size = new Size(Ui.Dp(520), Ui.Dp(64));
        CornerRadius = Ui.Dp(PleyShape.CardRadius);
        Item = new FocusItem(row.Url) { Selected = onChoose, LongSelected = onForget, Menu = onForget };

        iconBox = new View
        {
            Size = new Size(Ui.Dp(40), Ui.Dp(40)),
            Position = new Position(Ui.Dp(12), Ui.Dp(12)),
            CornerRadius = Ui.Dp(PleyShape.FieldRadius),
        };
        icon = Icons.Icon("monitor", 22, PleyColors.Fg);
        icon.Position = new Position(Ui.Dp(9), Ui.Dp(9));
        iconBox.Add(icon);
        Add(iconBox);

        string? tagText = null;
        if (row.Lookalike) (tagText, tagWarns) = ("Not your saved server", true);
        else if (row.Unencrypted) (tagText, tagWarns) = ("Not encrypted", true);
        else if (row.Found) tagText = "On this network";
        float tagWidth = 0;
        if (tagText != null)
        {
            tag = Text.Label(tagText, PleyType.Caption, PleyColors.Muted);
            tagWidth = tag.NaturalSize.Width / Ui.Scale + 2;
            tag.Size = new Size(Ui.Dp(tagWidth), Ui.Dp(64));
            tag.Position = new Position(Ui.Dp(520 - 12 - tagWidth), 0);
            Add(tag);
            if (!tagWarns)
            {
                dot = new View
                {
                    Size = new Size(Ui.Dp(8), Ui.Dp(8)),
                    Position = new Position(Ui.Dp(520 - 12 - tagWidth - 8 - 8), Ui.Dp(28)),
                    CornerRadius = Ui.Dp(4),
                    BackgroundColor = PleyColors.Success,
                };
                Add(dot);
                tagWidth += 16;
            }
            tagWidth += 12;
        }
        var textWidth = 520 - 68 - 12 - tagWidth;
        name = Text.Label(row.Name, PleyType.Title, PleyColors.Fg, width: textWidth);
        name.Position = new Position(Ui.Dp(68), Ui.Dp(12.5f));
        line = Text.Label("", PleyType.Caption, PleyColors.Muted, width: textWidth);
        line.Position = new Position(Ui.Dp(68), Ui.Dp(12.5f + PleyType.Title.Line));
        Add(name);
        Add(line);
        Item.FocusChanged += _ => Look();
        Look();
    }

    public ServerRow Row { get; }
    public FocusItem Item { get; }

    public void SetConnecting(bool on)
    {
        if (on == connecting) return;
        connecting = on;
        Look();
    }

    void Look()
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        var focused = Item.Focused;
        BackgroundColor = focused ? PleyColors.Accent : Color.Transparent;
        var ink = focused ? PleyColors.OnAccent : PleyColors.Fg;
        var soft = focused ? PleyColors.OnAccent : PleyColors.Muted;
        iconBox.BackgroundColor = focused ? new Color(PleyColors.OnAccent.R, PleyColors.OnAccent.G, PleyColors.OnAccent.B, 0.12f) : PleyColors.Surface3;
        icon.Color = ink;
        name.TextColor = ink;
        var danger = false;
        if (connecting) Text.Show(line, $"Connecting to {Row.Address}…");
        else if (Row.Unreachable)
        {
            Text.Show(line, "Can't reach it");
            danger = true;
        }
        else Text.Show(line, Row.Address);
        line.TextColor = danger && !focused ? PleyColors.DangerText : soft;
        if (tag != null) tag.TextColor = tagWarns && !focused ? PleyColors.DangerText : soft;
    }
}
