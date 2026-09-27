package io.github.ozanstn1.pdfswissarmyknife

import android.content.Intent
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Log
import androidx.activity.enableEdgeToEdge
import java.io.File
import java.io.FileOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

class MainActivity : TauriActivity() {
  // The generated TauriActivity disables the WebView back handling so plugins
  // can consume the button. The frontend records one history entry per screen
  // (lib/nav-history.ts); this flag makes the system back button walk that
  // stack and fall through to "finish the activity" once Home is reached.
  override val handleBackNavigation: Boolean = true

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    installTessdata()
    super.onCreate(savedInstanceState)
    importOpenWithIntent(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    // singleTask keeps one activity instance, so later VIEW/SEND intents arrive
    // here; getIntent() must be refreshed for the rest of the process.
    setIntent(intent)
    importOpenWithIntent(intent)
  }

  // ---------------------------------------------------------------------------
  // Open-with / share intents
  //
  // Modern file managers send content:// URIs, which the Rust engine cannot
  // read. Each incoming stream is copied into cacheDir/incoming/<uuid>/ under
  // its display name and its absolute path is appended to
  // cacheDir/pending-open.txt, which android_intent.rs drains for the webview.
  // ---------------------------------------------------------------------------

  /** Extensions the frontend can route. Anything else is logged and skipped. */
  private val openableExtensions = setOf(
    "docx", "odt", "rtf", "txt", "md", "html",
    "xlsx", "ods", "csv", "tsv",
    "pptx", "odp",
    "pdf", "osed", "ospr", "osdt", "oswk",
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff",
  )

  /** A single shared document may not exceed 256 MB. */
  private val maxIncomingBytes = 256L * 1024L * 1024L

  private fun importOpenWithIntent(intent: Intent?) {
    if (intent == null) return
    val candidates = when (intent.action) {
      Intent.ACTION_VIEW -> listOfNotNull(intent.data)
      Intent.ACTION_SEND -> listOfNotNull(streamUri(intent)) + clipUris(intent)
      Intent.ACTION_SEND_MULTIPLE -> streamUris(intent) + clipUris(intent)
      else -> emptyList()
    }
    val uris = candidates.filter { uri -> uri.scheme == "content" || uri.scheme == "file" }
    if (uris.isEmpty()) return
    Thread {
      for (uri in uris) {
        try {
          copyIncoming(uri)
        } catch (error: Exception) {
          Log.w(TAG, "open-with: unexpected failure for $uri (${error.message})")
        }
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

  private fun copyIncoming(uri: Uri) {
    val displayName = queryDisplayName(uri)
    if (displayName == null) {
      Log.w(TAG, "open-with: no usable file name for $uri")
      return
    }
    val extension = displayName.substringAfterLast('.', "").lowercase()
    if (extension.isEmpty() || extension !in openableExtensions) {
      Log.w(TAG, "open-with: rejected '.$extension' for $uri")
      return
    }
    val directory = File(File(cacheDir, "incoming"), UUID.randomUUID().toString())
    if (!directory.mkdirs()) {
      Log.w(TAG, "open-with: cannot create ${directory.absolutePath}")
      return
    }
    val destination = File(directory, displayName)
    try {
      contentResolver.openInputStream(uri).use { input ->
        if (input == null) {
          Log.w(TAG, "open-with: provider returned no stream for $uri")
          directory.delete()
          return
        }
        destination.outputStream().use { output ->
          if (!copyCapped(input, output, maxIncomingBytes)) {
            Log.w(TAG, "open-with: $displayName exceeds the 256 MB limit")
            destination.delete()
            directory.delete()
            return
          }
        }
      }
    } catch (error: Exception) {
      Log.w(TAG, "open-with: copy failed for $uri (${error.message})")
      destination.delete()
      directory.delete()
      return
    }
    appendPendingOpen(destination.absolutePath)
  }

  /** Resolves the provider's display name (OpenableColumns), sanitized. */
  private fun queryDisplayName(uri: Uri): String? {
    if (uri.scheme == "file") return sanitizeDisplayName(uri.lastPathSegment)
    var cursor: Cursor? = null
    try {
      cursor = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
      if (cursor != null && cursor.moveToFirst()) {
        val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (index >= 0) {
          val name = sanitizeDisplayName(cursor.getString(index))
          if (name != null) return name
        }
      }
    } catch (error: Exception) {
      Log.w(TAG, "open-with: cannot resolve a name for $uri (${error.message})")
    } finally {
      cursor?.close()
    }
    // Some providers only expose a document id as the last path segment; use it
    // when it at least looks like a file name.
    val fallback = sanitizeDisplayName(uri.lastPathSegment)
    return if (fallback != null && fallback.contains('.')) fallback else null
  }

  /**
   * Provider names are untrusted: keep the base name only, never path
   * separators or "..", so the cache copy can never escape its directory.
   */
  private fun sanitizeDisplayName(raw: String?): String? {
    var name = raw?.trim().orEmpty()
    if (name.isEmpty()) return null
    name = name.substringAfterLast('/').substringAfterLast('\\')
    name = name.replace("..", "_")
    name = name.replace(Regex("[\\\\/:*?\"<>|]"), "_")
    name = name.trim().trimStart('.')
    return name.ifEmpty { null }
  }

  /** Copies at most `limit` bytes; returns false when the stream is larger. */
  private fun copyCapped(input: InputStream, output: OutputStream, limit: Long): Boolean {
    val buffer = ByteArray(64 * 1024)
    var total = 0L
    while (true) {
      val read = input.read(buffer)
      if (read < 0) break
      total += read
      if (total > limit) return false
      output.write(buffer, 0, read)
    }
    return true
  }

  /** Appends one absolute path per line; the Rust side drains the file. */
  private fun appendPendingOpen(path: String) {
    synchronized(pendingOpenLock) {
      try {
        FileOutputStream(File(cacheDir, PENDING_OPEN_FILE), true).use { stream ->
          stream.write("$path\n".toByteArray(Charsets.UTF_8))
          stream.flush()
        }
      } catch (error: Exception) {
        Log.w(TAG, "open-with: cannot queue $path (${error.message})")
      }
    }
  }

  /**
   * Copies the OCR language models that ship as APK assets into the app's
   * private files directory. The bundled Tesseract executable reads them from
   * the filesystem (TESSDATA_PREFIX), so they cannot stay inside the APK.
   * The copy runs once per app version and is skipped afterwards.
   */
  private fun installTessdata() {
    val target = File(filesDir, "tessdata")
    val marker = File(target, ".installed")
    val stamp = "${packageInfoVersion()}-${assetStamp()}"
    if (marker.isFile && marker.readText().trim() == stamp) return
    if (!target.isDirectory && !target.mkdirs()) return
    val entries = try {
      assets.list("tessdata")
    } catch (error: Exception) {
      null
    } ?: return
    for (name in entries) {
      val assetPath = "tessdata/$name"
      val destination = File(target, name)
      if (hasChildren(assetPath)) {
        copyDirectory(assetPath, destination)
      } else {
        copyAsset(assetPath, destination)
      }
    }
    marker.writeText(stamp)
  }

  private fun assetStamp(): String = try {
    val entries = assets.list("tessdata") ?: emptyArray()
    entries.size.toString()
  } catch (error: Exception) {
    "0"
  }

  private fun packageInfoVersion(): String = try {
    packageManager.getPackageInfo(packageName, 0).versionName ?: "0"
  } catch (error: Exception) {
    "0"
  }

  private fun hasChildren(assetPath: String): Boolean = try {
    val children = assets.list(assetPath)
    children != null && children.isNotEmpty()
  } catch (error: Exception) {
    false
  }

  private fun copyDirectory(assetPath: String, destination: File) {
    if (!destination.isDirectory && !destination.mkdirs()) return
    val children = try {
      assets.list(assetPath)
    } catch (error: Exception) {
      null
    } ?: return
    for (name in children) {
      val childAsset = "$assetPath/$name"
      val childDestination = File(destination, name)
      if (hasChildren(childAsset)) {
        copyDirectory(childAsset, childDestination)
      } else {
        copyAsset(childAsset, childDestination)
      }
    }
  }

  private fun copyAsset(assetPath: String, destination: File) {
    try {
      assets.open(assetPath).use { input ->
        destination.outputStream().use { output -> input.copyTo(output) }
      }
    } catch (error: Exception) {
      // A missing optional model must not stop the app from starting.
    }
  }

  companion object {
    private const val TAG = "PdfSakIntent"
    private const val PENDING_OPEN_FILE = "pending-open.txt"
    private val pendingOpenLock = Any()
  }
}
