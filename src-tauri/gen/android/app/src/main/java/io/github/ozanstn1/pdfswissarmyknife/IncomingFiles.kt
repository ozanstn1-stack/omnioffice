package io.github.ozanstn1.pdfswissarmyknife

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

/**
 * Pure helpers for the open-with/share intent pipeline.
 *
 * They live in their own object (instead of inside [MainActivity]) so the JVM
 * unit tests can exercise the untrusted-input handling without an emulator:
 * provider-supplied file names are sanitized, only routable extensions are
 * accepted and the cache copy is size-capped.
 */
object IncomingFiles {
  /** Extensions the frontend can route. Anything else is logged and skipped. */
  val openableExtensions: Set<String> = setOf(
    "docx", "odt", "rtf", "txt", "md", "html",
    "xlsx", "ods", "csv", "tsv",
    "pptx", "odp",
    "pdf", "osed", "ospr", "osdt", "oswk",
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff",
  )

  /** A single shared document may not exceed 256 MB. */
  const val maxIncomingBytes: Long = 256L * 1024L * 1024L

  /**
   * Provider names are untrusted: keep the base name only, never path
   * separators or "..", so the cache copy can never escape its directory.
   */
  fun sanitizeDisplayName(raw: String?): String? {
    var name = raw?.trim().orEmpty()
    if (name.isEmpty()) return null
    name = name.substringAfterLast('/').substringAfterLast('\\')
    name = name.replace("..", "_")
    name = name.replace(Regex("[\\\\/:*?\"<>|]"), "_")
    name = name.trim().trimStart('.')
    return name.ifEmpty { null }
  }

  /** True when the (already sanitized) name has an extension the app can open. */
  fun isOpenableName(name: String): Boolean {
    val extension = name.substringAfterLast('.', "").lowercase()
    return extension.isNotEmpty() && extension in openableExtensions
  }

  /** Copies at most [limit] bytes; returns false when the stream is larger. */
  fun copyCapped(input: InputStream, output: OutputStream, limit: Long = maxIncomingBytes): Boolean {
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

  /**
   * Resolves the provider's display name (OpenableColumns), sanitized. Returns
   * null when the provider exposes no usable name.
   */
  fun queryDisplayName(resolver: ContentResolver, uri: Uri): String? {
    if (uri.scheme == "file") return sanitizeDisplayName(uri.lastPathSegment)
    var cursor: Cursor? = null
    try {
      cursor = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
      if (cursor != null && cursor.moveToFirst()) {
        val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (index >= 0) {
          val name = sanitizeDisplayName(cursor.getString(index))
          if (name != null) return name
        }
      }
    } catch (error: Exception) {
      // Fall through to the last-path-segment fallback below.
    } finally {
      cursor?.close()
    }
    val fallback = sanitizeDisplayName(uri.lastPathSegment)
    return if (fallback != null && fallback.contains('.')) fallback else null
  }

  /** Outcome of a copy attempt, so callers can log the precise reason. */
  enum class CopyResult { COPIED, NO_NAME, UNSUPPORTED, NO_STREAM, TOO_LARGE, FAILED }

  /**
   * Copies a `content://` (or `file://`) document into `cacheDir/incoming/`
   * under its sanitized display name and returns the cached [File] plus the
   * outcome. This is the whole open-with pipeline minus the activity plumbing,
   * so it can run under instrumentation without a WebView.
   */
  fun copyToCache(
    resolver: ContentResolver,
    uri: Uri,
    cacheDir: File,
    limit: Long = maxIncomingBytes,
  ): Pair<File?, CopyResult> {
    val displayName = queryDisplayName(resolver, uri) ?: return null to CopyResult.NO_NAME
    if (!isOpenableName(displayName)) return null to CopyResult.UNSUPPORTED
    val directory = File(File(cacheDir, "incoming"), UUID.randomUUID().toString())
    if (!directory.mkdirs()) return null to CopyResult.FAILED
    val destination = File(directory, displayName)
    try {
      resolver.openInputStream(uri).use { input ->
        if (input == null) {
          directory.delete()
          return null to CopyResult.NO_STREAM
        }
        destination.outputStream().use { output ->
          if (!copyCapped(input, output, limit)) {
            destination.delete()
            directory.delete()
            return null to CopyResult.TOO_LARGE
          }
        }
      }
    } catch (error: Exception) {
      destination.delete()
      directory.delete()
      return null to CopyResult.FAILED
    }
    return destination to CopyResult.COPIED
  }
}
