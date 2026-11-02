using System;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// Typing a server's address (design spec §2.11, tv-addendum §4.4; the Android app's TvAddressEntry):
/// a field and Connect under it. The TV's keyboard opens with the screen, so a name or a whole https
/// address types as easily as an IP; once it is closed, the remote's digit keys type too. Port 7539
/// is added when none is typed. Back goes back to the list.
/// </summary>
sealed class AddressEntry : Sheet
{
    const float Top = 27, FieldWidth = 480;

    readonly Shell shell;
    readonly ServersModel model;
    readonly TextInput field;
    readonly ButtonView connect;
    readonly TextLabel problem;
    readonly IDisposable watch;
    readonly IDisposable keys;
    string connectLabel = "Connect";

    public AddressEntry(Shell shell, ServersModel model) : base("address-entry")
    {
        this.shell = shell;
        this.model = model;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(Onboarding.BrandGlow());

        var center = PleyTv.CanvasWidth / 2;
        var title = Text.Label("Enter your server's address", PleyType.Title, PleyColors.Fg, width: 600);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        title.Position = new Position(Ui.Dp(center - 300), Ui.Dp(Top));
        View.Add(title);

        var fieldTop = Top + PleyType.Title.Line + 8;
        field = new TextInput(shell.Input, FieldWidth, "192.168.1.20 or pley.example.com", InputMethod.PanelLayoutType.URL, InputMethod.ActionButtonTitleType.Go)
        {
            Position = new Position(Ui.Dp(center - FieldWidth / 2), Ui.Dp(fieldTop)),
        };
        field.Changed += _ => model.AddressEdited();
        field.Submitted = Connect;
        View.Add(field);

        var connectTop = fieldTop + TextInput.Height + 14;
        connect = new ButtonView("Connect", ButtonKind.Pill, Connect);
        connect.PositionY = Ui.Dp(connectTop);
        LayoutConnect();
        View.Add(connect);

        // Under it: how addresses work, and what went wrong.
        var hint = Paragraph("Port 7539 is added when none is typed.", PleyType.Caption, PleyColors.Muted, 1);
        hint.Position = new Position(Ui.Dp(center - FieldWidth / 2), Ui.Dp(connectTop + PleyTv.ButtonHeight + 24));
        View.Add(hint);
        problem = Paragraph("", PleyType.Body, PleyColors.DangerText, 6);
        problem.Position = new Position(hint.PositionX, hint.PositionY + hint.SizeHeight + Ui.Dp(12));
        View.Add(problem);

        Root.Add(field.Item);
        Root.Add(connect.Item);

        // D-pad rule 10: with the keyboard closed, the remote's digit keys type into the field, with the
        // letters and "." ":" "/" "-" "_" a keyboard sends. While it is open, the field takes every key first.
        keys = shell.Input.Push((key, down) =>
        {
            if (shell.Top != this) return false;
            var typed = Typed(key);
            if (typed == null) return false;
            if (down)
            {
                if (typed == "\b") field.DeleteLast();
                else field.Type(typed);
            }
            return true;
        });
        watch = model.State.Watch(Show);
        Gone += () =>
        {
            watch.Dispose();
            keys.Dispose();
        };
    }

    public void Show()
    {
        shell.Push(this);
        Tree.Focus(field.Item);
        field.Start();
    }

    void Show(ServersState state)
    {
        problem.Text = state.AddressProblem?.Text ?? "";
        var label = state.CheckingAddress ? "Checking…" : "Connect";
        if (connectLabel != label)
        {
            connectLabel = label;
            connect.SetText(label);
            LayoutConnect();
        }
    }

    void Connect()
    {
        var text = field.Value.Trim();
        if (text.Length > 0) _ = model.Connect(text);
    }

    void LayoutConnect() => connect.PositionX = Ui.Dp(PleyTv.CanvasWidth / 2 - connect.WidthDp / 2);

    /// <summary>What a remote or keyboard key types: a digit, a letter, ".", ":", "/", "-", "_", "\b" for delete; null for other keys.</summary>
    static string? Typed(string key)
    {
        if (key.Length == 1 && char.IsLetterOrDigit(key[0])) return key.ToLowerInvariant();
        if (key.StartsWith("KP_") && key.Length == 4 && char.IsDigit(key[3])) return key.Substring(3);
        return key switch
        {
            "period" or "KP_Decimal" => ".",
            "colon" => ":",
            "slash" or "KP_Divide" => "/",
            "minus" or "KP_Subtract" => "-",
            "underscore" => "_",
            "BackSpace" or "Delete" or "Clear" => "\b",
            _ => null,
        };
    }

    static TextLabel Paragraph(string text, TextStyle style, Color color, int lines)
    {
        var label = Text.Label(text, style, color, maxLines: lines, width: FieldWidth);
        label.HorizontalAlignment = HorizontalAlignment.Center;
        label.VerticalAlignment = VerticalAlignment.Top;
        return label;
    }
}
