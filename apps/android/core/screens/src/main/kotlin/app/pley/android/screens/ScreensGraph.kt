package app.pley.android.screens

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.auth.AuthGraph
import app.pley.android.caps.CapsGraph
import app.pley.android.common.DeviceProfile
import app.pley.android.data.DataGraph
import app.pley.android.data.Downloads
import app.pley.android.data.drive.Drives
import app.pley.android.network.NetworkGraph
import app.pley.android.player.PlayerGraph
import app.pley.android.screens.remote.CastReceiver

/**
 * What a screen's ViewModel may use, handed over by the app. Each part is
 * made the first time it is asked for, so a screen pays only for what it
 * uses.
 */
interface ScreensGraph {
    val device: DeviceProfile
    val data: DataGraph
    val player: PlayerGraph
    val caps: CapsGraph
    val auth: AuthGraph
    val playerLauncher: PlayerLauncher

    /** Downloads for watching away from the server: phones and tablets; none on TVs and in tests. */
    val downloads: Downloads
        get() = Downloads.None

    /** Drives plugged into the device, whose videos play without a server: TVs; none on phones and in tests. */
    val drives: Drives
        get() = Drives.None

    /** Plays what the profile's remotes on its other devices cast here; none in tests unless they make one. */
    val casts: CastReceiver
        get() = CastReceiver.None

    /**
     * Finding servers and checking addresses (discovery, the network
     * sweep, the probe, local network access), and the signed-in clients
     * the sign-in module's profiles go through. Getting in uses it; graphs
     * made for other screens' tests needn't have one.
     */
    val network: NetworkGraph
        get() = throw UnsupportedOperationException("this graph has no network parts")
}

/** The Application, which holds the app's one graph. */
interface PleyGraphOwner {
    val screensGraph: ScreensGraph
}

/**
 * The graph, in a ViewModel factory's initializer:
 * `viewModelFactory { initializer { HomeViewModel(screensGraph()) } }`.
 */
fun CreationExtras.screensGraph(): ScreensGraph {
    val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
        ?: error("ViewModels are made with the Application in their CreationExtras")
    return (app as PleyGraphOwner).screensGraph
}
