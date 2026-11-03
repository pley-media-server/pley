package app.pley.android.data

import app.pley.android.common.DeviceProfile
import app.pley.android.testing.data.TestData
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher

/** Signs in to the test's server, opens it, and lets the session read who it is; on a phone unless [profile] says otherwise. */
@OptIn(ExperimentalCoroutinesApi::class)
internal suspend fun TestScope.signedIn(
    dir: File,
    scope: CoroutineScope = backgroundScope,
    profile: DeviceProfile = TestData.PHONE,
    prepare: TestData.() -> Unit = {},
): TestData {
    val data = TestData(this, dir, profile = profile, scope = scope)
    data.prepare()
    data.signIn()
    data.useServer()
    data.graph.session
    testScheduler.runCurrent()
    return data
}

/** Everything [flow] gives while the test runs, in order. */
@OptIn(ExperimentalCoroutinesApi::class)
internal fun <T> TestScope.collect(flow: Flow<T>, scope: CoroutineScope = backgroundScope): MutableList<T> {
    val seen = java.util.Collections.synchronizedList(mutableListOf<T>())
    scope.launch(UnconfinedTestDispatcher(testScheduler)) { flow.collect { seen += it } }
    return seen
}

/** A scope for one run of the app on a folder, ended by cancelling it before the next run. */
internal fun TestScope.appRun(): CoroutineScope =
    CoroutineScope(backgroundScope.coroutineContext + Job(backgroundScope.coroutineContext[Job]))

/** Where TestData keeps [scope]'s snapshot of [key]. */
internal fun snapshotFile(dir: File, scope: ProfileScope, key: String): File =
    File(dir, "files/snapshots/${scope.server}/${scope.userId}/${FileSnapshotStore.fileName(key)}")

/** The values of Ready and Failed states, Loading as null. */
internal fun <T> List<Loadable<T>>.values(): List<T?> = map { it.valueOrNull }
