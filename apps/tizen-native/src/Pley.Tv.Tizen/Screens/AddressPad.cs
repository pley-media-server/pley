using System;
using Pley.Tv.Core;
using Pley.Tv.Core.Ui;
using Pley.Tv.Tizen.Kit;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Screens;

/// <summary>
/// The address pad (design spec §2.11, tv-addendum §4.4; the Android app's TvAddressPad): the PIN
/// pad's keys with "." and ":", ⌫ and Connect under them, below a field that starts with the TV's
/// network ("192.168.1."). The remote's digit keys type too; OK on the field opens the TV's
/// keyboard, for names. Port 7539 is added when none is typed. Back goes back to the list.
/// </summary>
sealed class AddressPad : Sheet
{
    const float FieldWidth = 360, HintLeft = 96, HintTop = 121, HintWidth = 240;

    readonly Shell shell;
    readonly ServersModel model;
    readonly TextInput field;
    readonly Keypad keypad;
    readonly PadKeyView delete;
    readonly ButtonView connect;
    readonly View bottom;
    readonly TextLabel problem;
    readonly IDisposable watch;
    readonly IDisposable digits;
    string connectLabel = "Connect";

    public AddressPad(Shell shell, ServersModel model) : base("address-pad")
    {
        this.shell = shell;
        this.model = model;
        View.BackgroundColor = PleyColors.Bg;
        View.Add(Onboarding.BrandGlow());

        // Beside the pad: how addresses work, and what went wrong.
        var hint = Paragraph("Port 7539 is added when none is typed. Press OK on the field to type a name.", PleyType.Caption, PleyColors.Muted, 4);
        hint.Position = new Position(Ui.Dp(HintLeft), Ui.Dp(HintTop));
        View.Add(hint);
        problem = Paragraph("", PleyType.Body, PleyColors.DangerText, 8);
        problem.Position = new Position(Ui.Dp(HintLeft), hint.PositionY + hint.SizeHeight + Ui.Dp(12));
        View.Add(problem);

        var center = PleyTv.CanvasWidth / 2;
        var title = Text.Label("Enter your server's address", PleyType.Title, PleyColors.Fg, width: 600);
        title.HorizontalAlignment = HorizontalAlignment.Center;
        title.Position = new Position(Ui.Dp(center - 300), Ui.Dp(27));
        View.Add(title);

        field = new TextInput(shell.Input, FieldWidth, "192.168.1.20 or pley.example.com", InputMethod.PanelLayoutType.URL, InputMethod.ActionButtonTitleType.Go)
        {
            Position = new Position(Ui.Dp(center - FieldWidth / 2), Ui.Dp(27 + PleyType.Title.Line + 8)),
        };
        field.Changed += _ => model.AddressEdited();
        field.Submitted = Connect;
        View.Add(field);

        keypad = new Keypad(PadKey.AddressKeys, OnKey);
        var keypadTop = 27 + PleyType.Title.Line + 8 + TextInput.Height + 14;
        keypad.Position = new Position(Ui.Dp(center) - keypad.SizeWidth / 2, Ui.Dp(keypadTop));
        View.Add(keypad);

        // ⌫ and Connect, a key's gap apart, under the keys.
        bottom = new View();
        delete = new PadKeyView(new PadKey.Delete(), OnKey);
        bottom.Add(delete);
        connect = new ButtonView("Connect", ButtonKind.Pill, Connect);
        bottom.Add(connect);
        var bottomGroup = new FocusGroup(Axis.Row, "pad-actions");
        bottomGroup.Add(delete.Item);
        bottomGroup.Add(connect.Item);
        bottom.PositionY = keypad.PositionY + keypad.SizeHeight + Ui.Dp(PadKeyView.Gap);
        LayoutBottom();
        View.Add(bottom);

        Root.Add(field.Item);
        Root.Add(keypad.Group);
        Root.Add(bottomGroup);

        // D-pad rule 10: the remote's digit keys type into the pad, with "." and ":" and letters a keyboard sends.
        digits = shell.Input.Push((key, down) =>
        {
            if (shell.Top != this) return false;
            var typed = Typed(key);
            if (typed == null) return false;
            if (down)
            {
                if (typed == "\b")
                {
                    keypad.Flash(new PadKey.Delete());
                    delete.Flash();
                    field.DeleteLast();
                }
                else
                {
                    keypad.Flash(typed.Length == 1 && char.IsDigit(typed[0]) ? new PadKey.Digit(typed[0] - '0') : new PadKey.Symbol(typed));
                    field.Type(typed);
                }
            }
            return true;
        });
        watch = model.State.Watch(Show);
        Gone += () =>
        {
            watch.Dispose();
            digits.Dispose();
        };
    }

    public void Show()
    {
        shell.Push(this);
        if (field.Value.Length == 0 && model.State.Value.Subnet != null) field.Value = model.State.Value.Subnet!;
        Tree.Focus(keypad.Initial);
    }

    void Show(ServersState state)
    {
        problem.Text = state.AddressProblem?.Text ?? "";
        var label = state.CheckingAddress ? "Checking…" : "Connect";
        if (connectLabel != label)
        {
            connectLabel = label;
            connect.SetText(label);
            LayoutBottom();
        }
        if (field.Value.Length == 0 && state.Subnet != null) field.Value = state.Subnet;
    }

    void OnKey(PadKey key)
    {
        switch (key)
        {
            case PadKey.Digit d:
                field.Type(d.Value.ToString());
                break;
            case PadKey.Symbol s:
                field.Type(s.Text);
                break;
            case PadKey.Delete:
                field.DeleteLast();
                break;
            case PadKey.Ok:
                Connect();
                break;
        }
    }

    void Connect()
    {
        var text = field.Value.Trim();
        if (text.Length > 0) _ = model.Connect(text);
    }

    void LayoutBottom()
    {
        var width = PadKeyView.KeySize + PadKeyView.Gap + connect.WidthDp;
        bottom.Size = new Size(Ui.Dp(width), Ui.Dp(PadKeyView.KeySize));
        bottom.PositionX = Ui.Dp(PleyTv.CanvasWidth / 2 - width / 2);
        connect.Position = new Position(Ui.Dp(PadKeyView.KeySize + PadKeyView.Gap), Ui.Dp((PadKeyView.KeySize - PleyTv.ButtonHeight) / 2));
    }

    /// <summary>What a remote or keyboard key types: a digit, ".", ":", a letter, "\b" for delete; null for other keys.</summary>
    static string? Typed(string key)
    {
        if (key.Length == 1 && char.IsLetterOrDigit(key[0])) return key.ToLowerInvariant();
        if (key.StartsWith("KP_") && key.Length == 4 && char.IsDigit(key[3])) return key.Substring(3);
        return key switch
        {
            "period" or "KP_Decimal" => ".",
            "colon" => ":",
            "minus" or "KP_Subtract" => "-",
            "BackSpace" or "Delete" or "Clear" => "\b",
            _ => null,
        };
    }

    static TextLabel Paragraph(string text, TextStyle style, Color color, int lines)
    {
        var label = Text.Label(text, style, color, maxLines: lines, width: HintWidth);
        label.VerticalAlignment = VerticalAlignment.Top;
        return label;
    }
}
