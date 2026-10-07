package io.github.ozanstn1.pdfswissarmyknife

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * Foreground service (type `dataSync`) that keeps the app process alive while
 * Rust jobs (OCR, compression, conversions) run, so Android does not kill
 * them once the user switches to another app.
 *
 * It does no work itself. [BackgroundWorkPlugin] starts it when the Rust job
 * registry reports its first running job and stops it after the last one
 * ends (src-tauri/src/jobs.rs -> android_background.rs). It is not sticky: if
 * the system kills the process, the jobs are gone too and the next start
 * marks them interrupted.
 */
class BackgroundWorkService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    try {
      createChannel()
      ServiceCompat.startForeground(
        this,
        NOTIFICATION_ID,
        buildNotification(),
        BackgroundWorkPolicy.foregroundServiceType(Build.VERSION.SDK_INT),
      )
    } catch (error: Exception) {
      // For example Android 12+ refuses a foreground start from the background.
      // The jobs keep running for as long as the process lives; nothing else
      // depends on the service.
      Log.w(TAG, "background work: cannot enter the foreground (${error.message})")
      stopSelf()
    }
    return START_NOT_STICKY
  }

  /** Android 15 stops `dataSync` services after six hours in 24; end cleanly instead of crashing. */
  override fun onTimeout(startId: Int, fgsType: Int) {
    Log.w(TAG, "background work: the system time limit for data sync services was reached")
    stopSelf()
  }

  private fun createChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    val channel = NotificationChannel(
      CHANNEL_ID,
      getString(R.string.background_work_channel_name),
      // Low importance: visible in the shade, never a sound or a heads-up.
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = getString(R.string.background_work_channel_description)
      setShowBadge(false)
    }
    manager.createNotificationChannel(channel)
  }

  private fun buildNotification() = NotificationCompat.Builder(this, CHANNEL_ID)
    .setSmallIcon(R.drawable.ic_stat_work)
    .setContentTitle(getString(R.string.background_work_title))
    .setContentText(getString(R.string.background_work_text))
    .setCategory(NotificationCompat.CATEGORY_PROGRESS)
    .setOngoing(true)
    .setOnlyAlertOnce(true)
    .setPriority(NotificationCompat.PRIORITY_LOW)
    .setContentIntent(openAppIntent())
    .build()

  /** Tapping the notification brings the running app back to the front. */
  private fun openAppIntent(): PendingIntent? {
    val launch = packageManager.getLaunchIntentForPackage(packageName) ?: return null
    launch.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    return PendingIntent.getActivity(this, 0, launch, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
  }

  companion object {
    private const val TAG = "PdfSakBackground"
    const val CHANNEL_ID = "background_work"
    const val NOTIFICATION_ID = 4210
  }
}
