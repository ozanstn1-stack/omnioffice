package io.github.ozanstn1.pdfswissarmyknife

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.util.Log

/**
 * Entry point for "Open with" and share intents (VIEW / SEND / SEND_MULTIPLE).
 *
 * Why a trampoline: the senders decide the launch flags, and some (seen from
 * the system UI on a Pixel, and possible from chat and mail apps) add
 * FLAG_ACTIVITY_CLEAR_TASK. Delivered to MainActivity, that flag makes Android
 * create a new MainActivity and destroy the running one - and the generated
 * WryActivity.onDestroy calls Rust.onDestroy, which ends the whole process.
 * The document was copied, the app vanished, and the next launch opened the
 * leftover queue entry as an extra tab (reproduced with adb on v6.2.9).
 *
 * This activity has no visible UI and its own task (`taskAffinity=""`), so a
 * CLEAR_TASK from the sender only clears this throwaway task. It first wakes
 * MainActivity with [wakeFlags] (a running app comes forward, a cold start
 * creates it), then copies the documents itself: the sender's read grant
 * belongs to this activity and is revoked when it is destroyed, so the copy
 * must finish before [finish]. MainActivity's webview drains the queue.
 */
class OpenWithActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    OpenWithLog.attach(cacheDir)
    val incoming = intent
    OpenWithLog.record("trampoline action=${incoming?.action} flags=0x${Integer.toHexString(incoming?.flags ?: 0)}")
    try {
      startActivity(
        Intent(this, MainActivity::class.java)
          .setAction(MainActivity.ACTION_OPEN_WITH_WAKE)
          .addFlags(wakeFlags()),
      )
    } catch (error: Exception) {
      Log.w(TAG, "open-with: cannot wake MainActivity (${error.message})")
      OpenWithLog.record("trampoline wake FAILED error=${error.message}")
    }
    if (incoming == null) {
      finish()
      return
    }
    OpenWithImport.import(this, incoming) { runOnUiThread { finish() } }
  }

  companion object {
    private const val TAG = "PdfSakIntent"

    /**
     * Flags for waking MainActivity: NEW_TASK only. CLEAR_TASK, MULTIPLE_TASK,
     * NO_HISTORY and the other task flags are the sender's choice for its own
     * navigation and must never reach the singleTask activity that hosts the
     * Tauri runtime.
     */
    @JvmStatic
    fun wakeFlags(): Int = Intent.FLAG_ACTIVITY_NEW_TASK
  }
}
