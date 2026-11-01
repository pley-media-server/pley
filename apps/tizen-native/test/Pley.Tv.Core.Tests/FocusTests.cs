using System.Collections.Generic;
using System.Linq;
using Pley.Tv.Core.Ui;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The D-pad rules, on a shell like the TV app's: the rail, then a page of rows.</summary>
public class FocusTests
{
    sealed class Shell
    {
        public readonly FocusGroup Root = new FocusGroup(Axis.Row, "root");
        public readonly FocusGroup Rail;
        public readonly FocusGroup Page;
        public readonly FocusTree Tree;

        public Shell(int rows, int tiles)
        {
            Rail = Root.Add(new FocusGroup(Axis.Column, "rail"));
            foreach (var name in new[] { "search", "home", "movies" }) Rail.Add(new FocusItem(name));
            Page = Root.Add(new FocusGroup(Axis.Column, "page"));
            Tree = new FocusTree(Root);
            for (var r = 0; r < rows; r++)
            {
                var row = Page.Add(new FocusGroup(Axis.Row, $"row{r}"));
                // Right stops at a row's end; Left from its first tile goes to the rail's current item.
                row.Exit = d => d == Direction.Right || (d == Direction.Left && Tree.Enter(Rail));
                for (var t = 0; t < tiles; t++) row.Add(new FocusItem($"{r}.{t}"));
            }
        }

        public string? At => Tree.Current?.Name;

        public Shell Keys(params Direction[] keys)
        {
            foreach (var k in keys) Tree.Move(k);
            return this;
        }
    }

    [Fact]
    public void Focus_on_an_item_drawn_away_comes_back_to_its_row_on_the_next_key()
    {
        var s = new Shell(2, 3);
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Right);
        var row = s.Page.Children[0] as FocusGroup;
        Assert.Equal("0.1", s.At);
        // The row drawn again, its items new, focus left on the one that went.
        row!.Clear();
        foreach (var name in new[] { "a", "b", "c" }) row.Add(new FocusItem(name));
        Assert.True(s.Tree.Lost);
        Assert.True(s.Tree.Move(Direction.Down));
        Assert.Equal("a", s.At);
        Assert.False(s.Tree.Lost);
        s.Keys(Direction.Down);
        Assert.Equal("1.0", s.At);
    }

    [Fact]
    public void Focus_on_an_item_whose_row_went_too_goes_to_the_nearest_group_still_there()
    {
        var s = new Shell(2, 3);
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Down);
        Assert.Equal("1.0", s.At);
        s.Page.Remove(s.Page.Children[1]);
        Assert.True(s.Tree.Move(Direction.Up));
        Assert.Equal("0.0", s.At);
    }

    [Fact]
    public void Rows_move_left_and_right_and_stop_at_their_end()
    {
        var s = new Shell(2, 3);
        s.Tree.Enter(s.Page);
        Assert.Equal("0.0", s.At);
        Assert.Equal("0.2", s.Keys(Direction.Right, Direction.Right, Direction.Right, Direction.Right).At);
    }

    [Fact]
    public void Coming_back_to_a_row_lands_on_the_tile_it_had()
    {
        var s = new Shell(3, 5);
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Right, Direction.Right, Direction.Down);
        Assert.Equal("1.0", s.At);
        s.Keys(Direction.Right, Direction.Up);
        Assert.Equal("0.2", s.At);
        Assert.Equal("1.1", s.Keys(Direction.Down).At);
    }

    [Fact]
    public void Left_from_a_rows_first_tile_goes_to_the_rails_current_item_and_Right_comes_back()
    {
        var s = new Shell(2, 4);
        s.Tree.Focus((FocusItem)s.Rail.Children[2]);
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Down, Direction.Right, Direction.Left, Direction.Left);
        Assert.Equal("movies", s.At);
        Assert.Equal("1.0", s.Keys(Direction.Right).At);
    }

    [Fact]
    public void Disabled_and_hidden_items_are_passed_over()
    {
        var s = new Shell(1, 4);
        var row = (FocusGroup)s.Page.Children[0];
        ((FocusItem)row.Children[1]).Enabled = false;
        ((FocusItem)row.Children[2]).Shown = false;
        s.Tree.Enter(s.Page);
        Assert.Equal("0.3", s.Keys(Direction.Right).At);
    }

    [Fact]
    public void A_grid_moves_by_rows_and_columns_and_down_reaches_a_shorter_last_row()
    {
        var root = new FocusGroup(Axis.Grid) { Columns = 3 };
        for (var i = 0; i < 7; i++) root.Add(new FocusItem(i.ToString()));
        var tree = new FocusTree(root);
        tree.Enter(root);
        string? At() => tree.Current?.Name;
        tree.Move(Direction.Right);
        tree.Move(Direction.Right);
        Assert.Equal("2", At());
        tree.Move(Direction.Right);
        Assert.Equal("2", At());
        tree.Move(Direction.Down);
        Assert.Equal("5", At());
        tree.Move(Direction.Down);
        Assert.Equal("6", At());
        tree.Move(Direction.Up);
        Assert.Equal("3", At());
        tree.Move(Direction.Left);
        Assert.Equal("3", At());
    }

    [Fact]
    public void Tells_items_and_groups_where_focus_is()
    {
        var s = new Shell(2, 2);
        var looks = new List<string>();
        foreach (var row in s.Page.Children.Cast<FocusGroup>())
            foreach (var item in row.Children.Cast<FocusItem>())
                item.FocusChanged += on => looks.Add($"{item.Name}:{on}");
        var rowsFocused = new List<int>();
        s.Page.ChildFocused += rowsFocused.Add;
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Down);
        Assert.Equal(new[] { "0.0:True", "0.0:False", "1.0:True" }, looks);
        Assert.Equal(new[] { 0, 1 }, rowsFocused);
    }

    [Fact]
    public void A_removed_remembered_child_is_forgotten()
    {
        var s = new Shell(1, 4);
        var row = (FocusGroup)s.Page.Children[0];
        s.Tree.Enter(s.Page);
        s.Keys(Direction.Right, Direction.Right);
        s.Tree.Enter(s.Rail);
        row.Remove(row.Children[2]);
        s.Tree.Enter(s.Page);
        Assert.Equal("0.0", s.At);
    }
}
