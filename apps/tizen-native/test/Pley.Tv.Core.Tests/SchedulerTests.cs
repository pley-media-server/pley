using System;
using System.Threading.Tasks;
using Xunit;

namespace Pley.Tv.Core.Tests;

/// <summary>The TV's timers, on the real clock: the other tests move a fake one by hand.</summary>
public class SchedulerTests
{
    [Fact]
    public async Task A_timer_disposed_before_it_is_due_never_runs()
    {
        var scheduler = new ContextScheduler();
        var cancelledRan = false;
        var later = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        scheduler.After(20, () => cancelledRan = true).Dispose();
        scheduler.After(100, () => later.SetResult(true));
        Assert.True(await later.Task.WaitAsync(TimeSpan.FromSeconds(10)));
        Assert.False(cancelledRan);
    }
}
