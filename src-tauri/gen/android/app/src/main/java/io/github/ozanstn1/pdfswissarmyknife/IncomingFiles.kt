package io.github.ozanstn1.pdfswissarmyknife

import java.io.InputStream
import java.io.OutputStream

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
}
