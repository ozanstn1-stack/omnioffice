package io.github.ozanstn1.pdfswissarmyknife

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import java.io.File
import java.io.FileOutputStream

/**
 * Imports the documents of one "Open with" / share intent into the app.
 *
 * Modern file managers and chat apps send content:// URIs, which the Rust
 * engine cannot read. Each incoming stream is copied into
 * cacheDir/incoming/<uuid>/ under its display name and its absolute path is
 * appended to cacheDir/pending-open.txt, which android_intent.rs drains for the
 * webview. The untrusted-input handling lives in [IncomingFiles], covered by JVM
 * unit tests (src/test/.../IncomingFilesTest.kt).
 *
 * Why this is not inside MainActivity: the read grant the sender attaches to an
 * intent belongs to the activity that received it and is revoked when that
 * activity is destroyed; re-granting it to another activity of the same app is
 * a no-op. OpenWithActivity therefore copies the streams itself, while it still
 * holds the grant, and finishes only after [import] calls back.
 */
object OpenWithImport {
  private const val TAG = "PdfSakIntent"
  const val PENDING_OPEN_FILE = "pending-open.txt"
  private val pendingOpenLock = Any()

  /**
   * Copies every document of [intent] on a background thread and queues it;
   * [done] runs on that thread once all copies finished (also when there was
   * nothing to copy, then synchronously).
   */
  fun import(context: Context, intent: Intent, done: () -> Unit = {}) {
    val cacheDir = context.cacheDir
    // One info-level line per received intent: "I picked OmniOffice and nothing
    // happened" cannot be diagnosed on a device without the action, the type
    // the sender declared and the URI(s) it handed over. `adb logcat -s PdfSakIntent`
    // shows it on a development device; OpenWithLog keeps the same line on the
    // phone for the in-app report (Settings -> Diagnostics).
    val description = "action=${intent.action} type=${intent.type} data=${intent.data} " +
      "flags=0x${Integer.toHexString(intent.flags)} clip=${intent.clipData?.itemCount ?: 0}"
    Log.i(TAG, "intent $description")
    OpenWithLog.record("intent $description")
    val candidates = when (intent.action) {
      // Some senders put the document into the ClipData of a VIEW intent
      // instead of `data`, the same way others do it for ACTION_SEND.
      Intent.ACTION_VIEW -> listOfNotNull(intent.data) + clipUris(intent)
      Intent.ACTION_SEND -> listOfNotNull(streamUri(intent)) + clipUris(intent)
      Intent.ACTION_SEND_MULTIPLE -> streamUris(intent) + clipUris(intent)
      else -> emptyList()
    }
    val uris = candidates
      .filter { uri -> uri.scheme == "content" || uri.scheme == "file" }
      .distinct()
    OpenWithLog.record("candidates=${candidates.size} accepted=${uris.size}")
    if (uris.isEmpty()) {
      if (intent.action == Intent.ACTION_SEND) {
        val sharedText = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        if (!sharedText.isNullOrBlank()) {
          OpenWithLog.record("no uris; saving shared text length=${sharedText.length}")
          val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT)
          Thread {
            try {
              IncomingFiles.saveSharedText(sharedText, subject, cacheDir)?.let { saved ->
                appendPendingOpen(cacheDir, saved.absolutePath)
              }
            } finally {
              done()
            }
          }.start()
          return
        }
        OpenWithLog.record("no candidates: no URI and blank EXTRA_TEXT")
      } else {
        OpenWithLog.record("no candidates: no URI for action=${intent.action}")
      }
      done()
      return
    }
    Thread {
      try {
        for (uri in uris) {
          try {
            copyIncoming(context, uri, intent.type)
          } catch (error: Exception) {
            Log.w(TAG, "open-with: unexpected failure for $uri (${error.message})")
            OpenWithLog.record("copy FAILED uri=$uri error=${error.message}")
          }
        }
      } finally {
        done()
      }
    }.start()
  }

  @Suppress("DEPRECATION")
  private fun streamUri(intent: Intent): Uri? =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
    } else {
      intent.getParcelableExtra(Intent.EXTRA_STREAM) as? Uri
    }

  @Suppress("DEPRECATION")
  private fun streamUris(intent: Intent): List<Uri> {
    val raw = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
    } else {
      intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM)
    }
    return raw?.filterNotNull() ?: emptyList()
  }

  /** Some senders put the documents into the ClipData instead of EXTRA_STREAM. */
  private fun clipUris(intent: Intent): List<Uri> {
    val clip = intent.clipData ?: return emptyList()
    val uris = mutableListOf<Uri>()
    for (index in 0 until clip.itemCount) {
      clip.getItemAt(index).uri?.let(uris::add)
    }
    return uris
  }

  /** Copies one stream and queues its path, or queues the refusal reason. */
  fun copyIncoming(context: Context, uri: Uri, intentType: String?) {
    val cacheDir = context.cacheDir
    OpenWithLog.record("candidate uri=$uri type=$intentType")
    val (destination, result) = IncomingFiles.copyToCache(context.contentResolver, uri, cacheDir, intentType)
    if (result == IncomingFiles.CopyResult.COPIED) {
      val path = destination!!.absolutePath
      OpenWithLog.record("copy COPIED dest=$path")
      appendPendingOpen(cacheDir, path)
      return
    }
    // The user chose OmniOffice for this file, so a silent Log.w looks like
    // "the app did nothing". Queue a token the webview turns into a message;
    // the rejected stream itself is never copied.
    Log.w(TAG, "open-with: $result for $uri")
    val name = IncomingFiles.queryDisplayName(context.contentResolver, uri) ?: "?"
    OpenWithLog.record("copy $result uri=$uri name=$name")
    appendPendingOpen(cacheDir, "__error:${result.name}:$name")
  }

  fun pendingOpenPaths(cacheDir: File): List<String> {
    val file = File(cacheDir, PENDING_OPEN_FILE)
    if (!file.isFile) return emptyList()
    return file.readLines().filter { it.isNotBlank() }
  }

  /** Appends one absolute path per line; the Rust side drains the file. */
  fun appendPendingOpen(cacheDir: File, path: String) {
    // The queue path and the cache directory are the two facts the on-device
    // report needs to tell "the copy never happened" from "the webview never
    // polled", so they are recorded before the write is even attempted.
    OpenWithLog.record("queue path=$path cache=${cacheDir.absolutePath}")
    synchronized(pendingOpenLock) {
      try {
        FileOutputStream(File(cacheDir, PENDING_OPEN_FILE), true).use { stream ->
          stream.write("$path\n".toByteArray(Charsets.UTF_8))
          stream.flush()
        }
      } catch (error: Exception) {
        Log.w(TAG, "open-with: cannot queue $path (${error.message})")
        OpenWithLog.record("queue FAILED path=$path error=${error.message}")
      }
    }
  }
}
