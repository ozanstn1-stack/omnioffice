package io.github.ozanstn1.pdfswissarmyknife

import android.content.ContentResolver
import android.database.Cursor
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID
import java.util.zip.ZipEntry
import java.util.zip.ZipFile

/**
 * Pure helpers for the open-with/share intent pipeline.
 *
 * They live in their own object (instead of inside [MainActivity]) so the JVM
 * unit tests can exercise the untrusted-input handling without an emulator:
 * provider-supplied file names are sanitized, a shared document is accepted by
 * display name, then by MIME type, then by sniffing the copied bytes, and the
 * cache copy is size-capped.
 */
object IncomingFiles {
  // The same tag as [MainActivity]: docs/android.md documents
  // `adb logcat -s PdfSakIntent`, and that one filter must show both the
  // received intent and the copy outcome.
  private const val TAG = "PdfSakIntent"

  /** Extensions the frontend can route. Anything else is logged and skipped. */
  val openableExtensions: Set<String> = setOf(
    "docx", "docm", "dotx", "doc", "dot", "odt", "rtf", "txt", "md", "html",
    "xlsx", "xlsm", "xls", "ods", "csv", "tsv",
    "pptx", "pptm", "ppt", "odp",
    "pdf", "osed", "ospr", "osdt", "oswk",
    "eml", "xml", "zip",
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff",
  )

  /** A single shared document may not exceed 256 MB. */
  const val maxIncomingBytes: Long = 256L * 1024L * 1024L

  /** Bytes read before the content sniffing starts; enough for every magic. */
  private const val sniffHeaderBytes = 16 * 1024

  /** Bytes of a copied file the text/CSV heuristic inspects. */
  private const val sniffTextBytes = 64 * 1024

  /** The ODF media type is ~45 bytes; this window is all the sniff reads. */
  private const val mimetypeReadBytes = 256

  /** A declared `mimetype` size above this is hostile by definition. */
  private const val mimetypeDeclaredLimit = 1024L

  /**
   * Fallback extension per MIME type, used to give the provisional cache file
   * a plausible identity before the content sniff runs.
   *
   * Chat apps hand over opaque `content://` URIs with no display name, so the
   * MIME type is the only cheap hint. It is never trusted on its own:
   * [sniffExtension] has the last word, and `application/octet-stream` is
   * deliberately absent because it says nothing about the content.
   */
  private val extensionByMime: Map<String, String> = mapOf(
    "application/pdf" to "pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document" to "docx",
    "application/vnd.ms-word.document.macroenabled.12" to "docm",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template" to "dotx",
    "application/msword" to "doc",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" to "xlsx",
    "application/vnd.ms-excel.sheet.macroenabled.12" to "xlsm",
    "application/vnd.ms-excel" to "xls",
    "application/msexcel" to "xls",
    "application/x-msexcel" to "xls",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation" to "pptx",
    "application/vnd.ms-powerpoint.presentation.macroenabled.12" to "pptm",
    "application/vnd.ms-powerpoint" to "ppt",
    "application/vnd.oasis.opendocument.text" to "odt",
    "application/vnd.oasis.opendocument.spreadsheet" to "ods",
    "application/vnd.oasis.opendocument.presentation" to "odp",
    "application/rtf" to "rtf",
    "text/rtf" to "rtf",
    "text/csv" to "csv",
    "text/tab-separated-values" to "tsv",
    "message/rfc822" to "eml",
    "application/xml" to "xml",
    "text/xml" to "xml",
    "text/html" to "html",
    "text/plain" to "txt",
    "text/markdown" to "md",
    "text/x-markdown" to "md",
    "image/png" to "png",
    "image/jpeg" to "jpg",
    "image/webp" to "webp",
    "image/bmp" to "bmp",
    "image/x-ms-bmp" to "bmp",
    "image/gif" to "gif",
    "image/tiff" to "tif",
  )

  private val pdfMagic = "%PDF".toByteArray(Charsets.US_ASCII)
  private val zipMagic = byteArrayOf(0x50, 0x4B, 0x03, 0x04)
  private val compoundFileMagic = byteArrayOf(
    0xD0.toByte(), 0xCF.toByte(), 0x11, 0xE0.toByte(), 0xA1.toByte(), 0xB1.toByte(), 0x1A, 0xE1.toByte(),
  )
  private val rtfMagic = "{\\rtf".toByteArray(Charsets.US_ASCII)
  private val executableMagic = "MZ".toByteArray(Charsets.US_ASCII)

  /** Names an RFC 822 header line may carry; also rejects prose before `eml`. */
  private val headerName = Regex("[a-z0-9][a-z0-9-]*")

  /** Header names that make a line count towards the `eml` heuristic. */
  private val emlHeaders = setOf(
    "from", "to", "cc", "bcc", "subject", "date", "mime-version", "received",
    "return-path", "message-id", "content-type",
  )

  /** True when [extension] (without the dot) is one the frontend can route. */
  fun isOpenableExtension(extension: String): Boolean = extension.lowercase() in openableExtensions

  /**
   * Resolves the extension a MIME type implies, or null when the type is
   * unknown or says nothing (`application/octet-stream`).
   */
  fun extensionForMime(mime: String?): String? {
    val normalized = mime?.substringBefore(';')?.trim()?.lowercase().orEmpty()
    if (normalized.isEmpty()) return null
    return extensionByMime[normalized]
  }

  /**
   * Step 1-2 of the pipeline: reuse the provider name when it already carries
   * a routable extension, otherwise derive one from [mime] so the copied bytes
   * can be sniffed. `.bin` means "the type is unknown, decide from content".
   */
  fun provisionalName(displayName: String?, mime: String?): String {
    if (displayName != null && isOpenableName(displayName)) return displayName
    val extension = extensionForMime(mime) ?: "bin"
    return "shared-${UUID.randomUUID()}.$extension"
  }

  /**
   * The extension the provisional file is accepted with.
   *
   * Content wins when [sniffed] identifies the file, so a provider that
   * mislabels a DOCX as `application/pdf` still routes correctly. Otherwise a
   * routable MIME type is accepted as-is: images are deliberately not sniffed
   * (their magic adds nothing the MIME type does not already say), and a
   * sender-declared type is the same level of trust as a display name.
   * `null` means neither the declared type nor the bytes identify the file -
   * the caller deletes the copy and reports `UNKNOWN_TYPE`.
   */
  fun acceptedExtension(sniffed: String?, mime: String?): String? {
    if (sniffed != null && isOpenableExtension(sniffed)) return sniffed
    val fromMime = extensionForMime(mime)
    return if (fromMime != null && isOpenableExtension(fromMime)) fromMime else null
  }

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
    // The pending-open queue is one path per line: an inner line break in a
    // provider-supplied name would inject a bogus entry (and a newline in a
    // cache file name is legal on Linux). These are never meaningful in a
    // display name.
    name = name.replace(Regex("[\\u0000-\\u001f\\u007f]"), "_")
    name = name.trim().trimStart('.')
    return name.ifEmpty { null }
  }

  /** True when the (already sanitized) name has an extension the app can open. */
  fun isOpenableName(name: String): Boolean {
    val extension = name.substringAfterLast('.', "").lowercase()
    return extension.isNotEmpty() && isOpenableExtension(extension)
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

  /**
   * Outcome of a copy attempt, so callers can log the precise reason.
   *
   * [NO_NAME] and [UNSUPPORTED] are no longer produced by [copyToCache]: the
   * copy is always attempted and the bytes decide. They stay in the enum (and
   * in the `__error:<reason>:<name>` token contract) because a token queued by
   * an older install can still be drained by this webview, and the UI
   * localises every reason.
   */
  enum class CopyResult { COPIED, NO_NAME, UNSUPPORTED, NO_STREAM, TOO_LARGE, FAILED, NO_ACCESS, UNKNOWN_TYPE }

  /**
   * Copies a `content://` (or `file://`) document into `cacheDir/incoming/`
   * and returns the cached [File] plus the outcome. This is the whole open-with
   * pipeline minus the activity plumbing, so it can run under instrumentation
   * without a WebView.
   *
   * Acceptance order: a display name with a routable extension is trusted as
   * the user-visible identity and copied as-is (the app also opens its own
   * .oswk/.ospr/.osed/.osdt/.zip containers, which content sniffing does not
   * recognise). Every other URI - chat apps hand out opaque `content://` URIs
   * with no name, or a name without an extension - gets a provisional name
   * from [extensionForMime]; the copied bytes may then refine the type
   * ([sniffExtension]) and otherwise the routable MIME type is accepted. When
   * neither the declared type nor the bytes identify the file, the copy is
   * deleted and the result is [CopyResult.UNKNOWN_TYPE]: unsupported content is
   * never queued.
   */
  fun copyToCache(
    resolver: ContentResolver,
    uri: Uri,
    cacheDir: File,
    intentType: String? = null,
    limit: Long = maxIncomingBytes,
  ): Pair<File?, CopyResult> {
    val outcome = copyIncoming(resolver, uri, cacheDir, intentType, limit)
    Log.i(
      TAG,
      "incoming: $uri -> ${outcome.second}" + (outcome.first?.let { " (${it.name})" } ?: ""),
    )
    return outcome
  }

  private fun copyIncoming(
    resolver: ContentResolver,
    uri: Uri,
    cacheDir: File,
    intentType: String?,
    limit: Long,
  ): Pair<File?, CopyResult> {
    val displayName = queryDisplayName(resolver, uri)
    val mime = resolveMime(resolver, uri, intentType)
    val namedByProvider = displayName != null && isOpenableName(displayName)
    val cachedName = provisionalName(displayName, mime)
    Log.i(
      TAG,
      "incoming: uri=$uri mime=${mime ?: "unknown"} name=${displayName ?: "unknown"} provisional=$cachedName",
    )
    val directory = File(File(cacheDir, "incoming"), UUID.randomUUID().toString())
    if (!directory.mkdirs()) return null to CopyResult.FAILED
    val destination = File(directory, cachedName)
    val input = try {
      resolver.openInputStream(uri)
    } catch (error: SecurityException) {
      // The sender granted the URI but the provider refuses to open it (a
      // revoked or wrong grant); that is a permission problem, not a bad file
      // and deserves its own message.
      Log.w(TAG, "incoming: no read access to $uri (${error.message})")
      discard(directory)
      return null to CopyResult.NO_ACCESS
    } catch (error: Exception) {
      Log.w(TAG, "incoming: cannot open $uri (${error.message})")
      discard(directory)
      return null to CopyResult.FAILED
    }
    if (input == null) {
      discard(directory)
      return null to CopyResult.NO_STREAM
    }
    try {
      input.use { stream ->
        destination.outputStream().use { output ->
          if (!copyCapped(stream, output, limit)) {
            discard(directory)
            return null to CopyResult.TOO_LARGE
          }
        }
      }
    } catch (error: Exception) {
      Log.w(TAG, "incoming: copy failed for $uri (${error.message})")
      discard(directory)
      return null to CopyResult.FAILED
    }

    // A provider name with a routable extension already identifies the file;
    // sniffing it would only re-read bytes the frontend does not need.
    if (namedByProvider) return destination to CopyResult.COPIED

    val sniffed = sniffExtension(destination)
    val extension = acceptedExtension(sniffed, mime)
    Log.i(TAG, "incoming: sniffed=${sniffed ?: "unknown"} accepted=${extension ?: "unknown"} for $uri")
    if (extension == null) {
      discard(directory)
      return null to CopyResult.UNKNOWN_TYPE
    }
    val renamed = File(directory, renamedName(displayName, cachedName, extension))
    if (!destination.renameTo(renamed)) {
      Log.w(TAG, "incoming: cannot rename the sniffed copy for $uri")
      discard(directory)
      return null to CopyResult.FAILED
    }
    return renamed to CopyResult.COPIED
  }

  /**
   * The name a sniffed copy is renamed to: the provider's stem when there is
   * one, so `Invoice` with PDF bytes becomes `Invoice.pdf` instead of a random
   * cache name, else the provisional stem.
   */
  private fun renamedName(displayName: String?, provisionalName: String, extension: String): String {
    val fromProvider = displayName?.substringBeforeLast('.', displayName)?.let(::sanitizeDisplayName)
    val stem = fromProvider ?: provisionalName.substringBeforeLast('.', provisionalName)
    return "${stem.ifEmpty { "shared-document" }}.$extension"
  }

  /** Deletes a half-written cache directory; a failed file must never linger. */
  private fun discard(directory: File) {
    directory.deleteRecursively()
  }

  /**
   * The MIME type used for the provisional extension: the provider's answer
   * first (it knows what its URI really holds), then the intent type (file
   * managers often set it correctly even when the provider exposes nothing).
   * Both are hints, never proof: the sniff decides.
   */
  private fun resolveMime(resolver: ContentResolver, uri: Uri, intentType: String?): String? {
    val fromProvider = try {
      resolver.getType(uri)
    } catch (error: Exception) {
      null
    }
    return fromProvider ?: intentType
  }

  /**
   * Identifies the content of a copied file from its bytes, because chat apps
   * and download managers may hand over a URI with no usable display name.
   * Returns a routable extension or null when the content is unknown.
   *
   * The checks are deliberately shallow - a magic number or a container
   * marker, never a full parse: a corrupt document must fail while it is
   * opened with a real error, not be misclassified here.
   */
  fun sniffExtension(file: File): String? {
    val header = readHeader(file, sniffHeaderBytes) ?: return null
    return when {
      startsWith(header, pdfMagic) -> "pdf"
      startsWith(header, zipMagic) -> sniffZip(file)
      startsWith(header, compoundFileMagic) -> sniffCompoundFile(file)
      startsWith(header, rtfMagic) -> "rtf"
      looksLikeEml(header) -> "eml"
      looksLikeXml(header) -> "xml"
      // DOS/PE executables start with printable "MZ" bytes, which the text
      // heuristic below would otherwise call a text document. They are never
      // routed, so stop here instead of copying them as `.txt`.
      startsWith(header, executableMagic) -> null
      else -> sniffText(file)
    }
  }

  /**
   * OOXML and ODF are ZIP containers; the part layout (or the `mimetype` entry
   * ODF requires first) identifies the family without parsing the document.
   * Any other ZIP stays unknown: a generic archive gives the reader no type.
   */
  private fun sniffZip(file: File): String? = try {
    ZipFile(file).use { zip ->
      var word = false
      var spreadsheet = false
      var presentation = false
      var openDocument: String? = null
      val entries = zip.entries()
      while (entries.hasMoreElements() && openDocument == null) {
        val entry = entries.nextElement()
        if (entry.isDirectory) continue
        val path = entry.name
        when {
          path.startsWith("word/") -> word = true
          path.startsWith("xl/") -> spreadsheet = true
          path.startsWith("ppt/") -> presentation = true
          path == "mimetype" -> openDocument = openDocumentExtension(zip, entry)
        }
      }
      openDocument ?: when {
        word -> "docx"
        spreadsheet -> "xlsx"
        presentation -> "pptx"
        else -> null
      }
    }
  } catch (error: Exception) {
    null
  }

  /**
   * The ODF media type carried by the `mimetype` entry, read through a fixed
   * window instead of materialising the entry: the 256 MB copy cap bounds only
   * the source stream, not a decompressed ZIP entry, so a hostile archive could
   * otherwise declare a huge `mimetype` and exhaust the process. The declared
   * size is skipped as a cheap first guard (a conformant ODF type is ~45 bytes);
   * the bounded read is what actually protects the process if the declared size
   * lies.
   */
  private fun openDocumentExtension(zip: ZipFile, entry: ZipEntry): String? {
    if (entry.size > mimetypeDeclaredLimit) return null
    val bytes = ByteArray(mimetypeReadBytes)
    val read = try {
      zip.getInputStream(entry).use { input ->
        var total = 0
        while (total < bytes.size) {
          val chunk = input.read(bytes, total, bytes.size - total)
          if (chunk <= 0) break
          total += chunk
        }
        total
      }
    } catch (error: Exception) {
      return null
    }
    if (read <= 0) return null
    val mime = String(bytes, 0, read, Charsets.US_ASCII)
    return when {
      mime.contains("opendocument.text") -> "odt"
      mime.contains("opendocument.spreadsheet") -> "ods"
      mime.contains("opendocument.presentation") -> "odp"
      else -> null
    }
  }

  /**
   * Legacy Office files are OLE compound documents. The stream names live as
   * UTF-16LE directory entries, and the stream that defines the application
   * (`WordDocument`, `Workbook`/`Book`, `PowerPoint Document`) chooses the
   * extension. Scanning is enough: a real CFB parser would buy nothing here
   * because a corrupt file surfaces its own error when opened.
   */
  private fun sniffCompoundFile(file: File): String? = when {
    containsUtf16Le(file, "WordDocument") -> "doc"
    containsUtf16Le(file, "Workbook") || containsUtf16Le(file, "Book") -> "xls"
    containsUtf16Le(file, "PowerPoint Document") -> "ppt"
    else -> null
  }

  private fun containsUtf16Le(file: File, marker: String): Boolean {
    val needle = marker.toByteArray(Charsets.UTF_16LE)
    if (needle.isEmpty()) return false
    return try {
      file.inputStream().use { input -> scanForMarker(input, needle) }
    } catch (error: Exception) {
      false
    }
  }

  /**
   * Streams [input] looking for [needle], keeping a one-marker overlap so a
   * match split across two reads is still found. Directory sectors can sit
   * anywhere in a compound file, so the whole file is scanned; a bounded
   * window keeps a 256 MB share from being allocated twice.
   */
  private fun scanForMarker(input: InputStream, needle: ByteArray): Boolean {
    val keep = needle.size - 1
    val buffer = ByteArray(64 * 1024 + keep)
    var filled = 0
    while (true) {
      val read = input.read(buffer, filled, buffer.size - filled)
      if (read < 0) return false
      filled += read
      if (indexOfBytes(buffer, needle, filled) >= 0) return true
      if (keep > 0) {
        System.arraycopy(buffer, filled - keep, buffer, 0, keep)
        filled = keep
      } else {
        filled = 0
      }
    }
  }

  /** Index of [needle] within the first [length] bytes of [haystack], or -1. */
  private fun indexOfBytes(haystack: ByteArray, needle: ByteArray, length: Int): Int {
    val lastStart = length - needle.size
    for (start in 0..lastStart) {
      var matched = true
      for (offset in needle.indices) {
        if (haystack[start + offset] != needle[offset]) {
          matched = false
          break
        }
      }
      if (matched) return start
    }
    return -1
  }

  /**
   * A mail message carried as a file starts with an RFC 822 header block:
   * at least two `Name: value` lines plus MIME-Version, or From together with
   * one of Subject/To/Date/Received. ISO-8859-1 keeps arbitrary header bytes
   * from throwing; folded continuation lines are skipped.
   */
  private fun looksLikeEml(header: ByteArray): Boolean {
    var headerLines = 0
    var mimeVersion = false
    var from = false
    var subject = false
    var toOrDate = false
    for (raw in String(header, Charsets.ISO_8859_1).lineSequence().take(40)) {
      val line = raw.trimEnd('\r')
      if (line.isEmpty()) break
      if (line[0] == ' ' || line[0] == '\t') continue
      val colon = line.indexOf(':')
      if (colon <= 0) break
      val name = line.substring(0, colon).trim().lowercase()
      if (!headerName.matches(name)) break
      headerLines++
      when (name) {
        "mime-version" -> mimeVersion = true
        "from" -> from = true
        "subject" -> subject = true
        "to", "date", "received" -> toOrDate = true
      }
      if (!emlHeaders.contains(name)) continue
    }
    return headerLines >= 2 && (mimeVersion || (from && (subject || toOrDate)))
  }

  /** `<?xml` after an optional UTF-8 BOM and leading whitespace. */
  private fun looksLikeXml(header: ByteArray): Boolean {
    var index = if (
      header.size >= 3 && header[0] == 0xEF.toByte() && header[1] == 0xBB.toByte() && header[2] == 0xBF.toByte()
    ) {
      3
    } else {
      0
    }
    while (index < header.size) {
      val value = header[index].toInt() and 0xFF
      if (value == 0x20 || value == 0x09 || value == 0x0A || value == 0x0D) {
        index++
      } else {
        break
      }
    }
    if (header.size - index < 5) return false
    return String(header, index, 5, Charsets.US_ASCII) == "<?xml"
  }

  /**
   * Text/CSV fallback: mostly printable bytes without NULs. A consistent
   * delimiter shape over at least three lines means CSV/TSV; anything else
   * that reads as text is a `.txt`, which is still openable.
   */
  private fun sniffText(file: File): String? {
    val sample = readHeader(file, sniffTextBytes) ?: return null
    if (sample.isEmpty()) return null
    var printable = 0
    for (byte in sample) {
      val value = byte.toInt() and 0xFF
      when {
        value == 0 -> return null
        value == 0x09 || value == 0x0A || value == 0x0D -> printable++
        value in 0x20..0x7E -> printable++
        value >= 0x80 -> printable++
        else -> return null
      }
    }
    if (printable * 100 < sample.size * 95) return null
    return delimitedExtension(String(sample, Charsets.UTF_8)) ?: "txt"
  }

  /**
   * A conservative delimiter guess: three or more lines must all carry the
   * same positive number of a single delimiter. Requiring a consistent shape
   * keeps prose that happens to contain commas classified as plain text.
   */
  private fun delimitedExtension(text: String): String? {
    val lines = text.lineSequence().map { it.trimEnd('\r') }.filter { it.isNotBlank() }.take(20).toList()
    if (lines.size < 3) return null
    for ((delimiter, extension) in listOf('\t' to "tsv", ',' to "csv", ';' to "csv")) {
      val counts = lines.map { line -> line.count { it == delimiter } }
      if (counts.first() > 0 && counts.all { it == counts.first() }) return extension
    }
    return null
  }

  /** Reads at most [count] bytes; a short read is fine for every check here. */
  private fun readHeader(file: File, count: Int): ByteArray? = try {
    file.inputStream().use { input ->
      val output = ByteArrayOutputStream()
      val buffer = ByteArray(8 * 1024)
      var remaining = count
      while (remaining > 0) {
        val read = input.read(buffer, 0, minOf(buffer.size, remaining))
        if (read <= 0) break
        output.write(buffer, 0, read)
        remaining -= read
      }
      output.toByteArray()
    }
  } catch (error: Exception) {
    null
  }

  private fun startsWith(bytes: ByteArray, prefix: ByteArray): Boolean {
    if (bytes.size < prefix.size) return false
    for (index in prefix.indices) {
      if (bytes[index] != prefix[index]) return false
    }
    return true
  }

  /**
   * Saves plain text shared via `Intent.EXTRA_TEXT` (when no `EXTRA_STREAM` URI
   * is present) into `cacheDir/incoming/<uuid>/<subject>.txt` so the frontend
   * can open it as a document.
   */
  fun saveSharedText(text: String, subject: String?, cacheDir: File): File? {
    val trimmed = text.trim()
    if (trimmed.isEmpty()) return null
    val bytes = trimmed.toByteArray(Charsets.UTF_8)
    if (bytes.size > 4 * 1024 * 1024) return null
    val stem = sanitizeDisplayName(subject)?.removeSuffix(".txt")?.take(48)?.ifEmpty { null } ?: "shared-note"
    val directory = File(File(cacheDir, "incoming"), UUID.randomUUID().toString())
    if (!directory.mkdirs()) return null
    val destination = File(directory, "$stem.txt")
    return try {
      destination.writeBytes(bytes)
      destination
    } catch (error: Exception) {
      destination.delete()
      directory.delete()
      null
    }
  }

  /** How long a copied or shared document stays in the cache. */
  const val CACHE_MAX_AGE_MS: Long = 14L * 24 * 60 * 60 * 1000

  /**
   * Deletes the per-document folders under `cacheDir/incoming` and
   * `cacheDir/share` whose newest file is older than [maxAgeMs].
   *
   * Every document opened from another app and every share copy got its own
   * `<uuid>` folder and nothing removed them, so the cache only grew. Tabs do
   * not survive a restart, so a folder that old belongs to no open document;
   * the age is taken from the newest entry so a document still being re-shared
   * stays. Returns the number of folders removed; failures are skipped.
   */
  fun pruneDocumentCache(cacheDir: File, nowMs: Long, maxAgeMs: Long = CACHE_MAX_AGE_MS): Int {
    var removed = 0
    for (root in listOf(File(cacheDir, "incoming"), File(cacheDir, "share"))) {
      val folders = root.listFiles() ?: continue
      for (folder in folders) {
        if (!folder.isDirectory) continue
        val newest = folder.walkTopDown().maxOfOrNull { it.lastModified() } ?: folder.lastModified()
        if (nowMs - newest > maxAgeMs && folder.deleteRecursively()) removed += 1
      }
    }
    return removed
  }
}
