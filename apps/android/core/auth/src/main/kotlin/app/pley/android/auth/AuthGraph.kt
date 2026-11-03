package app.pley.android.auth

import android.app.Activity
import android.content.Context
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import okhttp3.OkHttpClient

/**
 * Signing in, for the rest of the app: the tokens of each server, the
 * household's profiles, and the TV's sign-in with a code. One per process.
 */
interface AuthGraph {
    /**
     * The tokens of [issuer] (a server's origin), shared by every client.
     * Made without opening the Keystore, so any thread may ask; the first
     * request opens them.
     */
    fun tokens(issuer: String): Tokens

    /**
     * Whether the app holds tokens for [issuer], read without the Keystore,
     * so the start screen can be chosen before the first frame.
     */
    fun hasTokens(issuer: String): Boolean

    /** The profiles of [issuer]'s household, over [api] (a client with its tokens). */
    fun profiles(issuer: String, api: OkHttpClient): Profiles

    /** Signing in with a code shown on screen (TVs, and phones without a browser). */
    val deviceSignIn: DeviceSignIn

    /**
     * Signs in to [server] from [activity] (phones) in the browser, on the
     * server's own page, in a private tab when [privateTab]. How it ends
     * comes in [signInState].
     */
    suspend fun signInWithBrowser(activity: Activity, server: String, privateTab: Boolean = true)

    /** The last browser sign-in's progress: Finishing, then SignedIn or Failed. */
    val signInState: StateFlow<SignInState>

    /** Sign-ins that ended without the person asking: show sign-in for that server. */
    val events: SharedFlow<AuthEvent>

    companion object {
        /**
         * The sign-in module over the app's [root] client (its clients
         * derive from it, sharing its connections), introducing itself as
         * [userAgent].
         */
        fun create(context: Context, root: OkHttpClient, userAgent: String): AuthGraph {
            Auth.use(root, userAgent)
            ServerProofs.followNetworks(context.applicationContext)
            return AppAuthGraph(context.applicationContext)
        }
    }
}

/** The graph over the process's one [Auth]. */
private class AppAuthGraph(private val context: Context) : AuthGraph {
    override fun tokens(issuer: String): Tokens = Auth.tokens(context, issuer)

    override fun hasTokens(issuer: String): Boolean = Auth.hasTokens(context, issuer)

    override fun profiles(issuer: String, api: OkHttpClient): Profiles = Profiles(api, tokens(issuer))

    override val deviceSignIn: DeviceSignIn
        get() = Auth.deviceSignIn(context)

    override suspend fun signInWithBrowser(activity: Activity, server: String, privateTab: Boolean) =
        Auth.begin(activity, server, privateTab)

    override val signInState: StateFlow<SignInState>
        get() = Auth.state

    override val events: SharedFlow<AuthEvent>
        get() = Auth.events
}
