using System;
using System.Collections.Generic;

namespace Pley.Tv.Core.Ui;

public enum Direction
{
    Up,
    Down,
    Left,
    Right,
}

/// <summary>How a group's children are laid out, which says which keys move between them.</summary>
public enum Axis
{
    /// <summary>Left and Right move between children.</summary>
    Row,
    /// <summary>Up and Down move between children.</summary>
    Column,
    /// <summary>A grid of <see cref="FocusGroup.Columns"/> columns, filled row by row.</summary>
    Grid,
    /// <summary>Children on top of each other: no key moves between them (pages of a stack, one shown).</summary>
    Stack,
}

/// <summary>Something focus can be in: an item, or a group of them.</summary>
public abstract class FocusNode
{
    public FocusGroup? Parent { get; internal set; }

    /// <summary>The group it was last taken out of: where focus goes back to if it was here.</summary>
    internal FocusGroup? FormerParent { get; set; }

    /// <summary>Whether focus can land on this node or inside it now.</summary>
    public abstract bool CanTakeFocus { get; }
}

/// <summary>
/// One focusable thing: a tile, a button, a field. What it does with OK, a held OK and the Menu
/// key, and keys of its own (a field typing); <see cref="FocusChanged"/> tells it to draw its
/// focus look.
/// </summary>
public class FocusItem : FocusNode
{
    bool enabled = true;
    bool shown = true;

    public FocusItem(string? name = null) => Name = name;

    /// <summary>For tests and for finding an element again on a page that was rebuilt.</summary>
    public string? Name { get; }

    public bool Enabled
    {
        get => enabled;
        set => enabled = value;
    }

    /// <summary>Whether it is on screen at all (an item scrolled away stays focusable; one hidden doesn't).</summary>
    public bool Shown
    {
        get => shown;
        set => shown = value;
    }

    public override bool CanTakeFocus => enabled && shown;

    public bool Focused { get; private set; }

    public event Action<bool>? FocusChanged;

    /// <summary>OK, let go before a long press.</summary>
    public Action? Selected { get; set; }

    /// <summary>OK held (the item's actions; Android's long click).</summary>
    public Action? LongSelected { get; set; }

    /// <summary>The remote's Menu key: the item's actions too, the way Fire TV apps open them.</summary>
    public Action? Menu { get; set; }

    /// <summary>Keys the item handles before anything else (a field typing): true when handled.</summary>
    public Func<string, bool, bool>? OwnKey { get; set; }

    internal void SetFocused(bool focused)
    {
        if (Focused == focused) return;
        Focused = focused;
        FocusChanged?.Invoke(focused);
    }
}

/// <summary>
/// A group of focus nodes laid out along an <see cref="Axis"/>. It remembers the child focused
/// last, so coming back to it lands there (Android's focusRestorer, D-pad rule 2).
/// </summary>
public class FocusGroup : FocusNode
{
    readonly List<FocusNode> children = new List<FocusNode>();

    public FocusGroup(Axis axis, string? name = null)
    {
        Axis = axis;
        Name = name;
    }

    public Axis Axis { get; }

    public string? Name { get; }

    /// <summary>Columns of a <see cref="Axis.Grid"/>.</summary>
    public int Columns { get; set; } = 1;

    /// <summary>Whether coming back lands on the child focused last (else the first).</summary>
    public bool Remember { get; set; } = true;

    /// <summary>The child focused last, or -1.</summary>
    public int Last { get; internal set; } = -1;

    /// <summary>Forgets the child focused last: coming back lands on the first (a page put back at its top).</summary>
    public void ForgetLast() => Last = -1;

    /// <summary>Coming back lands on the child at <paramref name="index"/> (an item's episode to watch).</summary>
    public void RememberChild(int index)
    {
        if (index >= 0 && index < children.Count) Last = index;
    }

    public IReadOnlyList<FocusNode> Children => children;

    /// <summary>
    /// Focus is about to leave this group: answering true keeps it (Right at a row's end) or says
    /// it was sent elsewhere (Left into the rail, D-pad rule 3); false lets it go on outward.
    /// </summary>
    public Func<Direction, bool>? Exit { get; set; }

    /// <summary>Focus moved to the child at this index, or somewhere inside it.</summary>
    public event Action<int>? ChildFocused;

    public override bool CanTakeFocus
    {
        get
        {
            foreach (var c in children)
                if (c.CanTakeFocus) return true;
            return false;
        }
    }

    public T Add<T>(T node) where T : FocusNode
    {
        Detach(node);
        node.Parent = this;
        children.Add(node);
        return node;
    }

    public T Insert<T>(int index, T node) where T : FocusNode
    {
        Detach(node);
        node.Parent = this;
        children.Insert(index, node);
        if (Last >= index) Last++;
        return node;
    }

    public void Remove(FocusNode node)
    {
        var i = children.IndexOf(node);
        if (i < 0) return;
        children.RemoveAt(i);
        node.Parent = null;
        node.FormerParent = this;
        if (Last == i) Last = -1;
        else if (Last > i) Last--;
    }

    public void Clear()
    {
        foreach (var c in children)
        {
            c.Parent = null;
            c.FormerParent = this;
        }
        children.Clear();
        Last = -1;
    }

    static void Detach(FocusNode node) => node.Parent?.Remove(node);

    internal void Focused(int index)
    {
        Last = index;
        ChildFocused?.Invoke(index);
    }
}

/// <summary>
/// Where focus is, and how keys move it (the D-pad rules of the Android app's TV design): along
/// each group's axis, outward through groups when a group has nothing further that way, into a
/// group at the child it remembers.
/// </summary>
public sealed class FocusTree
{
    public FocusTree(FocusGroup root) => Root = root;

    public FocusGroup Root { get; }

    public FocusItem? Current { get; private set; }

    /// <summary>Focus moved: to this item, or nowhere.</summary>
    public event Action<FocusItem?>? Moved;

    /// <summary>Puts focus on <paramref name="item"/>; false when it can't take it.</summary>
    public bool Focus(FocusItem item)
    {
        if (!item.CanTakeFocus || !InTree(item)) return false;
        if (Current == item) return true;
        var before = Current;
        Current = item;
        before?.SetFocused(false);
        // Each group on the way up remembers which of its children holds focus now.
        FocusNode child = item;
        for (var g = item.Parent; g != null; child = g, g = g.Parent)
        {
            var index = IndexOf(g, child);
            if (index >= 0) g.Focused(index);
        }
        item.SetFocused(true);
        Moved?.Invoke(item);
        return true;
    }

    /// <summary>Focus into <paramref name="node"/>: an item itself, or the child a group remembers (else its first).</summary>
    public bool Enter(FocusNode node)
    {
        var item = Landing(node);
        return item != null && Focus(item);
    }

    /// <summary>Clears focus: nothing on screen has it (a page going away).</summary>
    public void Blur()
    {
        if (Current == null) return;
        var before = Current;
        Current = null;
        before.SetFocused(false);
        Moved?.Invoke(null);
    }

    /// <summary>Moves focus one step <paramref name="direction"/>; false when nothing is that way.</summary>
    public bool Move(Direction direction)
    {
        if (Current == null) return Enter(Root);
        if (Lost) return Recover();
        FocusNode child = Current;
        for (var g = Current.Parent; g != null; child = g, g = g.Parent)
        {
            var target = Step(g, IndexOf(g, child), direction);
            if (target != null)
            {
                var item = Landing(target);
                if (item != null) return Focus(item);
            }
            if (g.Exit?.Invoke(direction) == true) return true;
        }
        return false;
    }

    /// <summary>
    /// Focus is on an item taken out of the tree: a page drew a row again under it (a library's
    /// chips as a sort or filter changes their labels) without putting focus back. No key moves
    /// from there, as it has no groups to go out through.
    /// </summary>
    public bool Lost => Current != null && !InTree(Current);

    /// <summary>
    /// Focus goes back into the group the lost item was taken out of, or the nearest one still in
    /// the tree, or the root's: at the child it remembers, else its first.
    /// </summary>
    public bool Recover()
    {
        for (FocusNode? n = Current?.Parent ?? Current?.FormerParent; n != null; n = n.Parent ?? n.FormerParent)
            if (InTree(n) && Enter(n)) return true;
        if (Enter(Root)) return true;
        Blur();
        return false;
    }

    bool InTree(FocusNode node)
    {
        var n = node;
        while (n.Parent != null) n = n.Parent;
        return n == Root;
    }

    static int IndexOf(FocusGroup g, FocusNode child)
    {
        var list = g.Children;
        for (var i = 0; i < list.Count; i++)
            if (list[i] == child) return i;
        return -1;
    }

    /// <summary>The item focus lands on when it enters <paramref name="node"/>.</summary>
    static FocusItem? Landing(FocusNode node)
    {
        if (node is FocusItem item) return item.CanTakeFocus ? item : null;
        var g = (FocusGroup)node;
        if (g.Remember && g.Last >= 0 && g.Last < g.Children.Count)
        {
            var remembered = Landing(g.Children[g.Last]);
            if (remembered != null) return remembered;
        }
        foreach (var c in g.Children)
        {
            var first = Landing(c);
            if (first != null) return first;
        }
        return null;
    }

    /// <summary>The child of <paramref name="g"/> a step <paramref name="d"/> from the one at <paramref name="index"/> that can take focus; null when none.</summary>
    static FocusNode? Step(FocusGroup g, int index, Direction d)
    {
        var list = g.Children;
        if (index < 0) return null;
        switch (g.Axis)
        {
            case Axis.Row when d is Direction.Left or Direction.Right:
            case Axis.Column when d is Direction.Up or Direction.Down:
            {
                var step = d is Direction.Left or Direction.Up ? -1 : 1;
                for (var i = index + step; i >= 0 && i < list.Count; i += step)
                    if (list[i].CanTakeFocus) return list[i];
                return null;
            }
            case Axis.Grid:
            {
                var columns = Math.Max(1, g.Columns);
                var column = index % columns;
                int target;
                switch (d)
                {
                    case Direction.Left:
                        if (column == 0) return null;
                        target = index - 1;
                        break;
                    case Direction.Right:
                        if (column == columns - 1 || index + 1 >= list.Count) return null;
                        target = index + 1;
                        break;
                    case Direction.Up:
                        target = index - columns;
                        break;
                    default:
                        target = index + columns;
                        // A shorter last row: Down from above its end lands on its last tile.
                        if (target >= list.Count && index / columns < (list.Count - 1) / columns) target = list.Count - 1;
                        break;
                }
                return target >= 0 && target < list.Count && list[target].CanTakeFocus ? list[target] : null;
            }
            default:
                return null;
        }
    }
}
