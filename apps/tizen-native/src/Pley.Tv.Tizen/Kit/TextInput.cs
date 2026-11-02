using System;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// Text entry on TV (D-pad rule 7, design spec §2.11; the Android app's TvTextField), only for search
/// and server addresses. Focus lands on the field without opening the keyboard, so D-pad users pass
/// through it; OK opens the TV's keyboard (or <see cref="Start"/>, on a screen that only asks for
/// words), and Back, the keyboard's own action or its closing ends typing, leaving focus on the field.
/// The remote's digit keys type into it too (<see cref="Type"/>).
///
/// 44 dp, Surface2, radius 4, Body. At rest a 1 dp Control edge; focused or typing a 2 dp
/// AccentBright edge, without scale (fields are wide). With an icon (search's glyph), the icon
/// comes first, Muted at rest and Fg focused, 12 dp before the text.
/// </summary>
sealed class TextInput : View
{
    public const float Height = 44;

    readonly Input input;
    readonly TextField field;
    readonly ImageView? icon;
    IDisposable? keys;
    bool editing;

    public TextInput(Input input, float width, string placeholder, InputMethod.PanelLayoutType layout = InputMethod.PanelLayoutType.Normal,
        InputMethod.ActionButtonTitleType action = InputMethod.ActionButtonTitleType.Search, string? iconName = null)
    {
        this.input = input;
        Size = new Size(Ui.Dp(width), Ui.Dp(Height));
        CornerRadius = Ui.Dp(PleyShape.FieldRadius);
        BackgroundColor = PleyColors.Surface2;
        BorderlineOffset = -1;
        var left = 16f;
        if (iconName != null)
        {
            icon = Icons.Icon(iconName, PleyTv.IconSize, PleyColors.Muted);
            icon.Position = new Position(Ui.Dp(left), Ui.Dp((Height - PleyTv.IconSize) / 2));
            Add(icon);
            left += PleyTv.IconSize + 12;
        }
        field = new TextField
        {
            Size = new Size(Ui.Dp(width - left - 16), Ui.Dp(Height)),
            Position = new Position(Ui.Dp(left), 0),
            PixelSize = Ui.Dp(PleyType.Body.Size),
            TextColor = PleyColors.Fg,
            PlaceholderText = placeholder,
            PlaceholderTextColor = PleyColors.Muted,
            PrimaryCursorColor = PleyColors.AccentBright,
            FontFamily = PleyType.Roboto,
            FontStyle = Text.Weight(PleyType.Regular),
            VerticalAlignment = VerticalAlignment.Center,
            EnableSelection = false,
            Focusable = true,
        };
        var method = new InputMethod { PanelLayout = layout, ActionButton = action };
        field.InputMethodSettings = method.OutputMap;
        field.TextChanged += (s, e) => Changed?.Invoke(field.Text);
        field.KeyEvent += OnFieldKey;
        // The keyboard closed by its own key (Back on it, its close key): typing ends.
        field.GetInputMethodContext().StatusChanged += (s, e) =>
        {
            if (!e.StatusChanged && editing) Stop();
        };
        Add(field);
        Item = new FocusItem("field") { Selected = Start };
        Item.FocusChanged += _ => Look();
        // Taken off the screen while typing (a page rebuilt round it): the TV closes its keyboard, so
        // typing ends here too, or the field would go on taking every key the remote sends.
        RemovedFromWindow += (s, e) => Stop();
        Look();
    }

    public FocusItem Item { get; }

    /// <summary>What is typed changed, by the keyboard or the remote.</summary>
    public event Action<string>? Changed;

    /// <summary>The keyboard's action key (Go, Search) was pressed.</summary>
    public Action? Submitted { get; set; }

    public string Value
    {
        get => field.Text ?? "";
        set => field.Text = value;
    }

    /// <summary>Adds <paramref name="text"/> at the end, as the remote's digit keys type it.</summary>
    public void Type(string text) => field.Text = Value + text;

    /// <summary>Deletes the last character, as the remote's Delete does.</summary>
    public void DeleteLast()
    {
        var v = Value;
        if (v.Length > 0) field.Text = v.Substring(0, v.Length - 1);
    }

    /// <summary>Opens the TV's keyboard on the field.</summary>
    public void Start()
    {
        if (editing) return;
        editing = true;
        // The remote's keys go to the keyboard and the field; what they leave (Up, Down) moves nothing.
        keys = input.Push((key, down) =>
        {
            if (down && key is "XF86Back" or "Escape") Stop();
            return true;
        });
        FocusManager.Instance.SetCurrentFocusView(field);
        field.GetInputMethodContext().Activate();
        field.GetInputMethodContext().ShowInputPanel();
        Look();
    }

    void Stop()
    {
        if (!editing) return;
        editing = false;
        keys?.Dispose();
        keys = null;
        var context = field.GetInputMethodContext();
        context.HideInputPanel();
        context.Deactivate();
        FocusManager.Instance.ClearFocus();
        Look();
    }

    bool OnFieldKey(object source, KeyEventArgs e)
    {
        if (!editing || e.Key.State != Key.StateType.Down) return false;
        switch (e.Key.KeyPressedName)
        {
            case "Return":
            case "KP_Enter":
            case "Select":
                Stop();
                Submitted?.Invoke();
                return true;
            case "XF86Back":
            case "Escape":
                Stop();
                return true;
            default:
                return false;
        }
    }

    void Look()
    {
        // Let go already: a focus change can still reach it on its way to the view in its place.
        if (Disposed) return;
        var ring = editing || Item.Focused;
        BorderlineWidth = Ui.Dp(ring ? 2 : 1);
        BorderlineColor = ring ? PleyColors.AccentBright : PleyColors.Control;
        if (icon != null) icon.Color = ring ? PleyColors.Fg : PleyColors.Muted;
    }

    protected override void Dispose(global::Tizen.NUI.DisposeTypes type)
    {
        if (Disposed) return;
        if (editing) Stop();
        base.Dispose(type);
    }
}
