package io.github.ozanstn1.pdfswissarmyknife

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
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
class BackgroundWorkArgs {
  /** True while at least one Rust job is running. */
  var active: Boolean = false
}

/**
 * Tauri mobile plugin behind `src-tauri/src/android_background.rs`.
 *
 * `setBackgroundWork(active)` starts or stops [BackgroundWorkService]. Starting
 * and stopping are both idempotent, so the Rust side only has to report the
 * transitions of "any job running". Failures are rejected with the code
 * `background_work_failed`; the app works without the service, the jobs are
 * just no longer protected from the system.
 */
@TauriPlugin
class BackgroundWorkPlugin(private val host: Activity) : Plugin(host) {
  @Command
  fun setBackgroundWork(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(BackgroundWorkArgs::class.java)
      if (args.active) startWork() else stopWork()
      invoke.resolve()
    } catch (error: Exception) {
      Log.w(TAG, "background work: ${error.message}")
      invoke.reject("background work could not be changed", "background_work_failed")
    }
  }

  private fun startWork() {
    askForNotificationPermissionOnce()
    val context = host.applicationContext
    ContextCompat.startForegroundService(context, Intent(context, BackgroundWorkService::class.java))
  }

  private fun stopWork() {
    val context = host.applicationContext
    context.stopService(Intent(context, BackgroundWorkService::class.java))
  }

  /**
   * Android 13+ shows the foreground notification only with the notification
   * permission. The service runs either way, so this never blocks the start:
   * the system prompt is shown once, the first time a job needs the service,
   * and a refusal is simply remembered.
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
        Log.w(TAG, "background work: the notification prompt could not be shown (${error.message})")
      }
    }
  }

  companion object {
    private const val TAG = "PdfSakBackground"
    private const val PREFERENCES = "background_work"
    private const val KEY_PERMISSION_ASKED = "notification_permission_asked"
    private const val PERMISSION_REQUEST_CODE = 4211
  }
}
