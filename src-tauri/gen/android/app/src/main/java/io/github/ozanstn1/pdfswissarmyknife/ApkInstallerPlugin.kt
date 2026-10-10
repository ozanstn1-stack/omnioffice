package io.github.ozanstn1.pdfswissarmyknife

import android.app.Activity
import android.content.Intent
import android.util.Log
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ApkInstallerArgs {
  /** Absolute path of the APK; the Rust side only accepts APKs under cacheDir/updates. */
  var path: String = ""
}

/**
 * Tauri mobile plugin behind `src-tauri/src/android_update.rs`.
 *
 * `installApk(path)` opens the system package installer for a downloaded
 * update. The app-private file is shared through the existing FileProvider
 * (`${applicationId}.fileprovider`, see AndroidManifest.xml and
 * res/xml/file_paths.xml) with a one-shot read grant, so the installer can
 * read it without the app exposing any storage. The user still has to confirm
 * the update in the system UI; failures are rejected with the code
 * `apk_install_failed` and the browser fallback of the update check remains.
 */
@TauriPlugin
class ApkInstallerPlugin(private val host: Activity) : Plugin(host) {
  @Command
  fun installApk(invoke: Invoke) {
    val file = try {
      File(invoke.parseArgs(ApkInstallerArgs::class.java).path)
    } catch (error: Exception) {
      Log.w(TAG, "apk install: bad arguments (${error.message})")
      invoke.reject("the system installer could not be opened", "apk_install_failed")
      return
    }
    if (!file.isFile) {
      invoke.reject("the downloaded update is gone", "apk_missing")
      return
    }
    // Rust may call this from a worker thread; Activity/intent launches belong
    // on the UI thread. run_mobile_plugin waits for resolve/reject, so every
    // path has to answer.
    host.runOnUiThread {
      try {
        val uri = FileProvider.getUriForFile(host, "${host.packageName}.fileprovider", file)
        val intent = Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(uri, APK_MIME)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        host.startActivity(intent)
        invoke.resolve()
      } catch (error: Exception) {
        Log.w(TAG, "apk install: ${error.message}")
        invoke.reject("the system installer could not be opened", "apk_install_failed")
      }
    }
  }

  companion object {
    private const val TAG = "PdfSakUpdate"
    private const val APK_MIME = "application/vnd.android.package-archive"
  }
}
