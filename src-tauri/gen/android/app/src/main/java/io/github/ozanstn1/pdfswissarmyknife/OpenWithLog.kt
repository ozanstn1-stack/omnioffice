package io.github.ozanstn1.pdfswissarmyknife

import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Durable, best-effort log of the open-with / share pipeline.
 *
 * Why this exists: the reported bug ("sharing a document opens the app, but the
 * document never arrives") only reproduces on a real phone without adb, so the
 * `Log.i` lines the pipeline already writes are invisible to support. This
 * object keeps the same facts in `cacheDir/open-with.log`, which the Rust
 * `android_open_with_diagnostics` command reads back and Settings shows as
 * copyable text: one reproduction is enough to see the received intent, every
 * candidate URI, the copy outcome and every queue append.
 *
 * Diagnostics must never break the pipeline they observe: every operation here
 * swallows its failures, and the file is capped so a chatty session cannot fill
 * the app cache (the Rust drain appends to the same file and applies the same
 * cap).
 */
object OpenWithLog {
  /** File name inside `cacheDir`; `android_intent.rs` reads the same file. */
  const val FILE_NAME = "open-with.log"

  /** Above this size the log is trimmed, keeping the newest lines. */
  private const val maxBytes = 128 * 1024

  /** What a trim keeps: enough history for one reproduction. */
  private const val keepBytes = 64 * 1024

  /** Serializes appends and trims; the copy thread and intent threads write. */
  private val lock = Any()

  /** Timestamp format with milliseconds and the device's UTC offset. */
  private val timestampFormat = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSSXXX", Locale.US)

  /**
   * Directory holding the log, set once by [MainActivity.onCreate]. Null until
   * then; the JVM tests attach their own temporary directory.
   */
  @Volatile
  private var cacheDir: File? = null

  /** Points the logger at the app cache; called once per process. */
  fun attach(directory: File) {
    cacheDir = directory
  }

  /** Appends one timestamped line, or does nothing when no directory is set. */
  fun record(line: String) {
    val directory = cacheDir ?: return
    try {
      synchronized(lock) {
        val file = File(directory, FILE_NAME)
        // One entry per line: a provider-supplied display name may contain a
        // line break, and a broken report is worse than a flattened one.
        val entry = "${timestamp()}\t${line.replace('\n', ' ').replace('\r', ' ')}\n"
        file.appendText(entry, Charsets.UTF_8)
        trim(file)
      }
    } catch (error: Exception) {
      // A diagnostics failure must never break the intent pipeline.
    }
  }

  /** Returns the log as-is; empty when nothing was recorded or it is unreadable. */
  fun read(): String {
    val directory = cacheDir ?: return ""
    return try {
      val file = File(directory, FILE_NAME)
      if (file.isFile) file.readText(Charsets.UTF_8) else ""
    } catch (error: Exception) {
      ""
    }
  }

  private fun timestamp(): String = timestampFormat.format(Date())

  /**
   * Keeps the newest [keepBytes] when the file grows past [maxBytes], starting
   * at the first complete line so the report never opens mid-entry.
   */
  private fun trim(file: File) {
    if (file.length() <= maxBytes) return
    val bytes = file.readBytes()
    var start = bytes.size - keepBytes
    while (start < bytes.size && bytes[start] != '\n'.code.toByte()) start += 1
    if (start < bytes.size) start += 1
    file.writeBytes(bytes.copyOfRange(start, bytes.size))
  }
}
