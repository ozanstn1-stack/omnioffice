package io.github.ozanstn1.pdfswissarmyknife

import android.app.Activity
import android.content.ClipData
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
class ShareFileArgs {
  /** Absolute path of the document; the Rust side only accepts app-private files. */
  var path: String = ""
  /** MIME type the receiving apps are filtered by (WhatsApp, Gmail, Drive, ...). */
  var mimeType: String = "application/octet-stream"
  /** Title of the system chooser. */
  var title: String = ""
}

/**
 * Tauri mobile plugin behind `src-tauri/src/android_share.rs`.
 *
 * `shareFile(path, mimeType, title)` opens the system share sheet for a
 * document the app holds in its private storage (a file opened from another
 * app, an office document written for sharing, a tool result). The file goes
 * out through the existing FileProvider (`${applicationId}.fileprovider`, see
 * AndroidManifest.xml and res/xml/file_paths.xml) with a read grant for the
 * receiving app only, so nothing is copied to shared storage first. Failures
 * are rejected with the code `share_failed`; the frontend then falls back to
 * publishing the file to Downloads and sharing that copy.
 */
@TauriPlugin
class ShareFilePlugin(private val host: Activity) : Plugin(host) {
  @Command
  fun shareFile(invoke: Invoke) {
    val args = try {
      invoke.parseArgs(ShareFileArgs::class.java)
    } catch (error: Exception) {
      Log.w(TAG, "share: bad arguments (${error.message})")
      invoke.reject("the share sheet could not be opened", "share_failed")
      return
    }
    val file = File(args.path)
    if (!file.isFile) {
      invoke.reject("the document to share is gone", "share_missing")
      return
    }
    // Rust may call this from a worker thread; activity launches belong on the
    // UI thread. run_mobile_plugin waits for resolve/reject, so every path has
    // to answer.
    host.runOnUiThread {
      try {
        val uri = FileProvider.getUriForFile(host, "${host.packageName}.fileprovider", file)
        val mimeType = args.mimeType.ifBlank { "application/octet-stream" }
        val send = Intent(Intent.ACTION_SEND).apply {
          type = mimeType
          putExtra(Intent.EXTRA_STREAM, uri)
          putExtra(Intent.EXTRA_TITLE, file.name)
          // ClipData carries the grant to the chosen app on Android 10+; the
          // EXTRA_STREAM alone is not enough for every receiver.
          clipData = ClipData.newUri(host.contentResolver, file.name, uri)
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val chooser = Intent.createChooser(send, args.title.ifBlank { null }).apply {
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        host.startActivity(chooser)
        Log.i(TAG, "share: opened the chooser for ${file.name} ($mimeType)")
        invoke.resolve()
      } catch (error: Exception) {
        Log.w(TAG, "share: ${error.message}")
        invoke.reject("the share sheet could not be opened", "share_failed")
      }
    }
  }

  companion object {
    private const val TAG = "PdfSakShare"
  }
}
