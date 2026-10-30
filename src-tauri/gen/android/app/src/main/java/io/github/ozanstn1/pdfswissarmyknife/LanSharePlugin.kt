package io.github.ozanstn1.pdfswissarmyknife

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class LanShareArgs {
  /** True while the Rust LAN host is sharing a folder. */
  var active: Boolean = false
}

/**
 * Tauri mobile plugin behind `src-tauri/src/android_lan.rs`.
 *
 * `setLanSharing(active)` starts or stops [LanShareService] and holds the
 * `WifiManager.MulticastLock` that makes mDNS work on Android (Wi-Fi power
 * saving otherwise drops multicast packets). Both operations are idempotent;
 * a failure is rejected and the Rust host keeps listening for as long as the
 * process lives.
 */
@TauriPlugin
class LanSharePlugin(private val host: Activity) : Plugin(host) {
  private var multicastLock: WifiManager.MulticastLock? = null

  @Command
  fun setLanSharing(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(LanShareArgs::class.java)
      if (args.active) startSharing() else stopSharing()
      invoke.resolve()
    } catch (error: Exception) {
      Log.w(TAG, "LAN share: ${error.message}")
      invoke.reject("LAN sharing could not be changed", "lan_share_failed")
    }
  }

  private fun startSharing() {
    askForNotificationPermissionOnce()
    acquireMulticastLock()
    val context = host.applicationContext
    ContextCompat.startForegroundService(context, Intent(context, LanShareService::class.java))
  }

  private fun stopSharing() {
    releaseMulticastLock()
    val context = host.applicationContext
    context.stopService(Intent(context, LanShareService::class.java))
  }

  /** mDNS responses are multicast; Android drops them without this lock. */
  @Suppress("WakelockTimeout")
  private fun acquireMulticastLock() {
    if (multicastLock != null) return
    try {
      val wifi = host.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return
      val lock = wifi.createMulticastLock(TAG).apply {
        setReferenceCounted(false)
        acquire()
      }
      multicastLock = lock
    } catch (error: Exception) {
      // Without the lock the broadcast fallback still finds nearby hosts.
      Log.w(TAG, "LAN share: multicast lock unavailable (${error.message})")
    }
  }

  private fun releaseMulticastLock() {
    try {
      multicastLock?.let { lock ->
        if (lock.isHeld) lock.release()
      }
    } catch (error: Exception) {
      Log.w(TAG, "LAN share: multicast lock release failed (${error.message})")
    } finally {
      multicastLock = null
    }
  }

  /**
   * Android 13+ shows the foreground notification only with the notification
   * permission; the service runs either way, so this never blocks sharing.
   */
  private fun askForNotificationPermissionOnce() {
    val preferences = host.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
    val granted = Build.VERSION.SDK_INT < BackgroundWorkPolicy.NOTIFICATION_PERMISSION_SDK ||
      ContextCompat.checkSelfPermission(host, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    val asked = preferences.getBoolean(KEY_PERMISSION_ASKED, false)
    if (!BackgroundWorkPolicy.shouldRequestNotificationPermission(Build.VERSION.SDK_INT, granted, asked)) return
    preferences.edit().putBoolean(KEY_PERMISSION_ASKED, true).apply()
    host.runOnUiThread {
      try {
        ActivityCompat.requestPermissions(host, arrayOf(Manifest.permission.POST_NOTIFICATIONS), PERMISSION_REQUEST_CODE)
      } catch (error: Exception) {
        Log.w(TAG, "LAN share: the notification prompt could not be shown (${error.message})")
      }
    }
  }

  companion object {
    private const val TAG = "PdfSakLanShare"
    private const val PREFERENCES = "lan_share"
    private const val KEY_PERMISSION_ASKED = "notification_permission_asked"
    private const val PERMISSION_REQUEST_CODE = 4221
  }
}
