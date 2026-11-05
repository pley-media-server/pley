package app.pley.android.downloads

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import app.pley.android.common.AppLanguage
import app.pley.android.common.Language
import app.pley.android.common.formatWords
import app.pley.android.data.DownloadState
import app.pley.android.model.format.formatBytes
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Fetches downloads in the foreground (type dataSync) while there are
 * some, with a notification that says which and how far, then stops.
 * The app starts it when downloads are asked for and when it opens.
 */
class DownloadService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private var run: Job? = null

    /** What the notification shows now, while the downloads run. */
    private var shown: Shown? = null

    // The notification's words in the language shown.
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(AppLanguage.wrap(base))
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // Android 13 and later put a language chosen into the app's words now: the notification says it.
        shown?.let { manager().notify(NOTIFICATION_ID, notification(it)) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val downloads = (application as? DownloadsOwner)?.appDownloads
        if (downloads == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        channel()
        try {
            ServiceCompat.startForeground(this, NOTIFICATION_ID, notification(shownOf(downloads.all.value, AppLanguage.shown.value)), foregroundType())
        } catch (_: Exception) {
            // Not allowed now (the app went to the background first): the app starts it again when it opens.
            stopSelf()
            return START_NOT_STICKY
        }
        if (run?.isActive != true) {
            run = scope.launch {
                val showing = launch {
                    combine(downloads.all, AppLanguage.shown, ::shownOf)
                        .distinctUntilChanged()
                        .collectLatest {
                            shown = it
                            manager().notify(NOTIFICATION_ID, notification(it))
                        }
                }
                // Off the main thread: the index is written on every change.
                withContext(Dispatchers.IO) { downloads.runQueue() }
                showing.cancel()
                shown = null
                ServiceCompat.stopForeground(this@DownloadService, ServiceCompat.STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_NOT_STICKY
    }

    /** Android 15 ends a dataSync service after its daily time: what is left goes on when the app opens. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        run?.cancel()
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }

    private fun foregroundType(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0

    private fun manager(): NotificationManager = getSystemService(NotificationManager::class.java)

    /** The channel, named in the language shown: the system's settings show it. */
    private fun channel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val words = AppLanguage.resources(this)
        val channel = NotificationChannel(CHANNEL, words.getString(R.string.downloads_channel), NotificationManager.IMPORTANCE_LOW).apply {
            description = words.getString(R.string.downloads_channel_description)
            setShowBadge(false)
        }
        manager().createNotificationChannel(channel)
    }

    /** What the notification shows: it changes only when one of these does (a whole percent, the language). */
    private data class Shown(
        val title: String?,
        val doneBytes: Long,
        val sizeBytes: Long,
        val percent: Int,
        val more: Int,
        val waiting: Boolean,
        /** What waits does so for the server, not for Wi-Fi. */
        val away: Boolean = false,
        /** Nothing downloads, but the server makes one: its title, and how far (percent). */
        val converting: Pair<String, Int>? = null,
        /** The language it is said in. */
        val language: Language,
    )

    private fun shownOf(all: List<Saved>, language: Language): Shown {
        val active = all.firstOrNull { it.state == DownloadState.Downloading }
        val making = all.firstOrNull { it.converting != null && it.mediaUrl == null && it.state == DownloadState.Queued }
        val left = all.count { it.state == DownloadState.Queued || it.state == DownloadState.Waiting || it.state == DownloadState.Downloading }
        val percent = if (active != null && active.sizeBytes > 0) (active.downloadedBytes * 100 / active.sizeBytes).toInt().coerceIn(0, 100) else 0
        // Sizes move with the percent: a step of 1 % keeps the text from changing more often than the bar.
        val done = if (active != null && active.sizeBytes > 0) active.sizeBytes * percent / 100 else 0
        return Shown(
            title = active?.let { a -> a.showTitle?.let { "$it · ${a.title}" } ?: a.title },
            doneBytes = done,
            sizeBytes = active?.sizeBytes ?: 0,
            percent = percent,
            more = (left - 1).coerceAtLeast(0),
            waiting = active == null && all.any { it.state == DownloadState.Waiting },
            away = all.filter { it.state == DownloadState.Waiting }.let { w -> w.isNotEmpty() && w.all { it.unreachable } },
            converting = making?.takeIf { active == null }?.let { m ->
                (m.showTitle?.let { "$it · ${m.title}" } ?: m.title) to ((m.converting ?: 0f) * 100).toInt()
            },
            language = language,
        )
    }

    /** "Downloading The Distant Harbor", "1.2 GB of 4.6 GB · 2 more", its bar; or waiting for Wi-Fi or the server. */
    private fun notification(shown: Shown): Notification {
        // In the language shown now, which may have changed since the service started.
        val words = AppLanguage.resources(this)
        val open = packageManager.getLaunchIntentForPackage(packageName)?.let {
            PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val builder = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_download_notification)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setCategory(NotificationCompat.CATEGORY_PROGRESS)
            .setContentIntent(open)
        when {
            shown.title != null -> {
                val sizes = formatWords(words)
                val size = formatBytes(shown.sizeBytes, sizes)
                val done = formatBytes(shown.doneBytes, sizes)
                builder.setContentTitle(words.getString(R.string.downloads_notification_downloading, shown.title))
                    .setContentText(
                        if (shown.more > 0) words.getString(R.string.downloads_notification_progress_more, done, size, shown.more)
                        else words.getString(R.string.downloads_notification_progress, done, size),
                    )
                    .setProgress(100, shown.percent, shown.sizeBytes <= 0)
            }
            shown.converting != null -> builder.setContentTitle(words.getString(R.string.downloads_notification_converting, shown.converting.first))
                .setContentText(words.getString(R.string.downloads_notification_converting_text, shown.converting.second))
                .setProgress(100, shown.converting.second, false)
            shown.waiting && shown.away -> builder.setContentTitle(words.getString(R.string.downloads_notification_waiting_server))
                .setContentText(words.getString(R.string.downloads_notification_waiting_server_text))
            shown.waiting -> builder.setContentTitle(words.getString(R.string.downloads_notification_waiting))
                .setContentText(words.getString(R.string.downloads_notification_waiting_text))
            else -> builder.setContentTitle(words.getString(R.string.downloads_notification_starting)).setProgress(0, 0, true)
        }
        return builder.build()
    }

    private companion object {
        const val CHANNEL = "downloads"
        const val NOTIFICATION_ID = 7_700
    }
}
