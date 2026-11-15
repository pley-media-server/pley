package app.pley.android.ui.phone.player

import android.app.AppOpsManager
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Rect
import android.os.Build
import android.os.Process
import android.util.Rational
import android.view.View
import kotlin.math.roundToInt

/** What the video player's and the live player's picture in picture share. */
internal object PictureInPicture {
    /** The widest picture in picture allows, either way up. */
    private const val MAX_ASPECT = 2.39f

    /** Whether the device has picture in picture and Pley may use it (Android's special app access). */
    fun allowed(context: Context): Boolean {
        if (!context.packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return false
        val ops = context.getSystemService(AppOpsManager::class.java) ?: return true
        @Suppress("DEPRECATION")
        val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_PICTURE_IN_PICTURE, Process.myUid(), context.packageName)
        } else {
            ops.checkOpNoThrow(AppOpsManager.OPSTR_PICTURE_IN_PICTURE, Process.myUid(), context.packageName)
        }
        return mode == AppOpsManager.MODE_ALLOWED
    }

    /** The [video]'s shape (width over height; 16:9 while it is 0, not known), within what picture in picture allows. */
    fun aspect(video: Float): Rational {
        val shape = video.takeIf { it > 0f } ?: (16f / 9f)
        val clamped = shape.coerceIn(1f / MAX_ASPECT + 0.001f, MAX_ASPECT - 0.001f)
        return Rational((clamped * 1000).roundToInt(), 1000)
    }

    /**
     * Where picture in picture grows from: the whole picture, fitted to
     * [root] (picture in picture shows all of it, however it was zoomed);
     * `null` before the window is laid out.
     */
    fun hint(root: View, video: Float): Rect? {
        if (root.width == 0 || root.height == 0) return null
        val r = PictureGeometry.place(root.width, root.height, video, 1f, keepBottom = false)
        return Rect(r.left, r.top, r.right, r.bottom).takeIf { !it.isEmpty }
    }

    /** Picture in picture's settings: the [video]'s shape, where it grows from, its buttons, and whether leaving enters it. */
    fun params(video: Float, hint: Rect?, actions: List<RemoteAction>, autoEnter: Boolean): PictureInPictureParams {
        val builder = PictureInPictureParams.Builder()
            .setAspectRatio(aspect(video))
            .setActions(actions)
        hint?.let(builder::setSourceRectHint)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            builder.setAutoEnterEnabled(autoEnter)
            builder.setSeamlessResizeEnabled(false)
        }
        return builder.build()
    }
}
