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
 * Foreground service (type `dataSync`) that keeps the LAN host alive while the
 * user shares a folder, so other paired devices can still connect after the
 * app goes to the background.
 *
 * It does no transfer itself: the Rust host owns the TCP listener. The service
 * only keeps the process alive and shows what is happening;
 * [LanSharePlugin] starts and stops it together with the multicast lock the
 * mDNS responder needs.
 */
class LanShareService : Service() {
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
      // Android 12+ can refuse a foreground start from the background; sharing
      // then simply lives as long as the process does.
      Log.w(TAG, "LAN share: cannot enter the foreground (${error.message})")
      stopSelf()
    }
    return START_NOT_STICKY
  }

  /** Android 15 stops `dataSync` services after six hours in 24; end cleanly. */
  override fun onTimeout(startId: Int, fgsType: Int) {
    Log.w(TAG, "LAN share: the system time limit for data sync services was reached")
    stopSelf()
  }

  private fun createChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    val channel = NotificationChannel(
      CHANNEL_ID,
      getString(R.string.lan_share_channel_name),
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = getString(R.string.lan_share_channel_description)
      setShowBadge(false)
    }
    manager.createNotificationChannel(channel)
  }

  private fun buildNotification() = NotificationCompat.Builder(this, CHANNEL_ID)
    .setSmallIcon(R.drawable.ic_stat_work)
    .setContentTitle(getString(R.string.lan_share_title))
    .setContentText(getString(R.string.lan_share_text))
    .setCategory(NotificationCompat.CATEGORY_SERVICE)
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
    private const val TAG = "PdfSakLanShare"
    const val CHANNEL_ID = "lan_share"
    const val NOTIFICATION_ID = 4220
  }
}
