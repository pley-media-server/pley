using System;
using System.Collections.Generic;
using Pley.Tv.Core.Ui;
using Tizen.NUI;
using Tizen.NUI.BaseComponents;

namespace Pley.Tv.Tizen.Kit;

/// <summary>
/// What the remote drives: a page, or a dialog over it. Each has its own focus, so focus in a
/// dialog never reaches the page behind it, and comes back where it was when the dialog goes.
/// </summary>
class Sheet
{
    public Sheet(string name, Axis axis = Axis.Column)
    {
        View = new View { WidthResizePolicy = ResizePolicyType.FillToParent, HeightResizePolicy = ResizePolicyType.FillToParent };
        Root = new FocusGroup(axis, name);
        Tree = new FocusTree(Root);
    }

    public View View { get; }
    public FocusGroup Root { get; }
    public FocusTree Tree { get; }

    /// <summary>
    /// Drawn over nothing: what is under it goes while it shows, and the window is clear there, so
    /// the TV's video plane under the window shows through (the player).
    /// </summary>
    public bool Covers { get; init; }

    /// <summary>Back: true when the sheet took it (closed a pad, went back a step).</summary>
    public Func<bool>? Back { get; set; }

    /// <summary>The sheet was taken off the screen: timers and requests stop.</summary>
    public event Action? Gone;

    internal void Leave() => Gone?.Invoke();
}

/// <summary>
/// The screen's sheets: one page, and dialogs over it. Back goes to the top one; a dialog that
/// doesn't take it closes, and a page that doesn't leaves the app.
/// </summary>
sealed class Shell
{
    readonly Window window;
    readonly List<Sheet> sheets = new List<Sheet>();

    public Shell(Window window, KeyRepeatGate gate, Action exit)
    {
        this.window = window;
        Input = new Input(window, () => Top?.Tree, gate) { Back = () => OnBack(exit) };
    }

    public Input Input { get; }

    public Sheet? Top => sheets.Count > 0 ? sheets[sheets.Count - 1] : null;

    /// <summary>A covering sheet (a player) is on screen, under a panel or a list of its own or not.</summary>
    public bool Covered => sheets.Exists(s => s.Covers);

    /// <summary>The app went to the background (false) or came back in front (true): what plays lets go, then starts again.</summary>
    public event Action<bool>? Shown;

    /// <summary>The app says it is hidden or shown again.</summary>
    public void Visible(bool shown) => Shown?.Invoke(shown);

    /// <summary>Shows <paramref name="page"/> in place of everything on screen.</summary>
    public void Show(Sheet page)
    {
        foreach (var l in sheets) Remove(l);
        sheets.Clear();
        sheets.Add(page);
        window.Add(page.View);
        Uncovered();
    }

    /// <summary>Puts <paramref name="sheet"/> (a dialog) over what is on screen.</summary>
    public void Push(Sheet sheet)
    {
        sheets.Add(sheet);
        window.Add(sheet.View);
        if (sheet.Covers) Uncovered();
    }

    /// <summary>Takes <paramref name="sheet"/> off the screen; focus is again where it was under it.</summary>
    public void Pop(Sheet sheet)
    {
        if (!sheets.Remove(sheet)) return;
        if (!sheet.Covers)
        {
            Remove(sheet);
            return;
        }
        // A player lets go of the TV's player as it goes, which holds the UI thread while the video
        // plane empties. Done before the window is opaque again, the last frame drawn is the clear
        // one, and what is under the app (the TV's menu) shows for that moment. So the pages come
        // back and are drawn first, and the player goes a few frames later.
        sheet.View.Hide();
        sheet.Tree.Blur();
        Uncovered();
        var later = new Timer(LetGoMs);
        later.Tick += (s, e) =>
        {
            leaving.Remove(later);
            Remove(sheet);
            return false;
        };
        leaving.Add(later);
        later.Start();
    }

    /// <summary>Time for a frame or two (at 60 Hz) to be drawn before a covering sheet lets go.</summary>
    const uint LetGoMs = 80;

    /// <summary>Timers for sheets on their way out, held so they aren't collected before they fire.</summary>
    readonly List<Timer> leaving = new List<Timer>();

    /// <summary>Sheets under the top covering one go; the window is clear while one shows.</summary>
    void Uncovered()
    {
        var cover = sheets.FindLastIndex(s => s.Covers);
        for (var i = 0; i < sheets.Count; i++)
        {
            if (i < cover) sheets[i].View.Hide();
            else sheets[i].View.Show();
        }
        // The window has an alpha while a covering sheet shows, and none otherwise, set each time.
        // Set once and kept, only its colour changing, every second player showed the TV's menu
        // where its picture was, with its sound; and an opaque window is less for the TV to draw.
        var clear = cover >= 0;
        if (clear != transparent)
        {
            window.SetTransparency(clear);
            transparent = clear;
        }
        window.BackgroundColor = clear ? Color.Transparent : PleyColors.Bg;
    }

    bool transparent;

    static void Remove(Sheet sheet)
    {
        sheet.Tree.Blur();
        sheet.View.Unparent();
        sheet.Leave();
        sheet.View.Dispose();
    }

    void OnBack(Action exit)
    {
        var top = Top;
        if (top == null)
        {
            exit();
            return;
        }
        if (top.Back?.Invoke() == true) return;
        if (sheets.Count > 1) Pop(top);
        else AskExit(exit);
    }

    /// <summary>
    /// Back on the first screen asks "Exit Pley?" first, as Samsung's rule for apps has it and the
    /// web Samsung app does: Exit (focused) leaves, Cancel or Back stays.
    /// </summary>
    void AskExit(Action exit)
    {
        // A short question: a narrow card, its title and two buttons.
        var dialog = new Dialog(this, "Exit Pley?", "", width: 360);
        dialog.Button("Cancel", dialog.Close);
        dialog.Button("Exit", exit, initial: true);
        dialog.Show();
    }
}
