package io.github.ozanstn1.pdfswissarmyknife

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/**
 * JVM guard for the "Open with" / share intent filters in AndroidManifest.xml.
 *
 * Android only evaluates a `<data android:pathPattern>` when the same filter
 * also declares a scheme and a host, and a filter without any scheme or type
 * only matches intents that carry no data at all. The file associations the
 * Tauri generator used to write were bare path patterns, so no file manager
 * intent ever matched them. These tests read the real manifest and pin the
 * rules that make the filters work; OpenWithIntentTest asks the real
 * PackageManager on a device.
 */
class ManifestIntentFiltersTest {
  private class Filter(
    val actions: Set<String>,
    val schemes: Set<String>,
    val hosts: Set<String>,
    val mimeTypes: Set<String>,
    val pathPatterns: List<String>,
    val pathSuffixes: List<String>,
  ) {
    val hasPath: Boolean get() = pathPatterns.isNotEmpty() || pathSuffixes.isNotEmpty()
  }

  private val androidNs = "http://schemas.android.com/apk/res/android"

  /** Gradle runs unit tests in the module directory; an IDE may start at the repo root. */
  private fun locate(vararg candidates: String): File =
    candidates.map(::File).firstOrNull { it.isFile }
      ?: throw AssertionError("none of ${candidates.toList()} exists (working dir ${File(".").absolutePath})")

  private val manifestFile: File
    get() = locate(
      "src/main/AndroidManifest.xml",
      "app/src/main/AndroidManifest.xml",
      "src-tauri/gen/android/app/src/main/AndroidManifest.xml",
    )

  private fun attr(element: Element, name: String): String? =
    element.getAttributeNS(androidNs, name).takeIf { it.isNotEmpty() }

  private fun mainActivityFilters(): List<Filter> {
    val factory = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
    val document = factory.newDocumentBuilder().parse(manifestFile)
    val activities = document.getElementsByTagName("activity")
    val main = (0 until activities.length)
      .map { activities.item(it) as Element }
      .first { attr(it, "name") == ".MainActivity" }
    val filters = main.getElementsByTagName("intent-filter")
    return (0 until filters.length).map { index ->
      val filter = filters.item(index) as Element
      fun values(tag: String, attribute: String): List<String> {
        val nodes = filter.getElementsByTagName(tag)
        return (0 until nodes.length).mapNotNull { attr(nodes.item(it) as Element, attribute) }
      }
      Filter(
        actions = values("action", "name").toSet(),
        schemes = values("data", "scheme").toSet(),
        hosts = values("data", "host").toSet(),
        mimeTypes = values("data", "mimeType").toSet(),
        // aapt2 turns "\\" in an attribute into "\" when it compiles the manifest;
        // the matcher (and these tests) must see what the device sees.
        pathPatterns = values("data", "pathPattern").map { it.replace("\\\\", "\\") },
        pathSuffixes = values("data", "pathSuffix"),
      )
    }
  }

  private val view = "android.intent.action.VIEW"
  private val send = "android.intent.action.SEND"
  private val sendMultiple = "android.intent.action.SEND_MULTIPLE"

  /** Extension -> the MIME types senders use for it. */
  private val typesByExtension = mapOf(
    "docx" to listOf("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    "odt" to listOf("application/vnd.oasis.opendocument.text"),
    "rtf" to listOf("application/rtf", "text/rtf"),
    "xlsx" to listOf("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    "ods" to listOf("application/vnd.oasis.opendocument.spreadsheet"),
    "pptx" to listOf("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
    "odp" to listOf("application/vnd.oasis.opendocument.presentation"),
    "pdf" to listOf("application/pdf"),
    "oswk" to listOf("application/x-oswk"),
  )

  @Test
  fun `a path is never declared without a scheme and a host`() {
    val offenders = mainActivityFilters().filter { it.hasPath && (it.schemes.isEmpty() || it.hosts.isEmpty()) }
    assertTrue("Android ignores paths without a scheme and a host: ${offenders.size} filter(s)", offenders.isEmpty())
  }

  @Test
  fun `a specific type is never combined with a path`() {
    // Type and path must both match, but content:// paths usually have no
    // extension, so only the catch-all type may sit next to a path.
    val offenders = mainActivityFilters().filter { it.hasPath && (it.mimeTypes - "*/*").isNotEmpty() }
    assertTrue("specific types next to paths: ${offenders.size} filter(s)", offenders.isEmpty())
  }

  @Test
  fun `share filters carry no scheme because a share intent has no data uri`() {
    val shareFilters = mainActivityFilters().filter { send in it.actions || sendMultiple in it.actions }
    assertTrue("there must be a share filter", shareFilters.isNotEmpty())
    assertTrue(shareFilters.all { it.schemes.isEmpty() && !it.hasPath })
  }

  @Test
  fun `every supported type opens and shares through a typed filter`() {
    val typed = mainActivityFilters().filter { view in it.actions && send in it.actions && sendMultiple in it.actions }
    assertEquals("one typed open/share filter", 1, typed.size)
    for ((extension, types) in typesByExtension) {
      for (type in types) {
        assertTrue("$extension: $type is missing from the typed filter", type in typed.single().mimeTypes)
      }
    }
  }

  @Test
  fun `the name based fallbacks exist for content and file uris with and without a type`() {
    val fallbacks = mainActivityFilters().filter { it.hasPath }
    assertEquals("one fallback with a generic type and one without a type", 2, fallbacks.size)
    for (filter in fallbacks) {
      assertEquals(setOf(view), filter.actions)
      assertEquals(setOf("content", "file"), filter.schemes)
      assertEquals(setOf("*"), filter.hosts)
    }
    assertEquals(setOf(setOf("*/*"), emptySet<String>()), fallbacks.map { it.mimeTypes }.toSet())
  }

  @Test
  fun `every extension has an exact suffix and a pattern per dot count in both fallbacks`() {
    for (filter in mainActivityFilters().filter { it.hasPath }) {
      for (extension in typesByExtension.keys) {
        assertTrue("$extension: pathSuffix", ".$extension" in filter.pathSuffixes)
        val own = filter.pathPatterns.filter { it.endsWith("\\.$extension") }
        assertEquals("$extension: one pattern per dot count", 4, own.size)
      }
      assertEquals(
        "no path for an unsupported extension",
        typesByExtension.keys.map { ".$it" }.toSet(),
        filter.pathSuffixes.toSet(),
      )
    }
  }

  @Test
  fun `the patterns match dotted names, nothing else and are case sensitive`() {
    val filter = mainActivityFilters().first { it.hasPath }
    fun matches(path: String) = filter.pathPatterns.any { AndroidGlob.matches(it, path) }
    // The plain pattern alone fails as soon as the name or a folder has a dot.
    assertTrue(AndroidGlob.matches(".*\\.docx", "/document/primary:Download/report.docx"))
    assertFalse(AndroidGlob.matches(".*\\.docx", "/document/primary:Download/My.Report.docx"))
    val wanted = listOf(
      "/document/primary:Download/report.docx",
      "/document/primary:Download/My.Report.docx",
      "/document/primary:Download/My.Report.v2.xlsx",
      "/storage/emulated/0/Download/v1.2/My.Report.v2.pptx",
      "/a.odt",
      "/files/notes.oswk",
      "/files/scan.pdf",
    )
    for (path in wanted) assertTrue("$path should match", matches(path))
    val unwanted = listOf(
      "/document/msf:1234",
      "/storage/emulated/0/Download/report.docx.exe",
      "/storage/emulated/0/Download/setup.apk",
      "/storage/emulated/0/Download/docx",
      "/storage/emulated/0/Download/REPORT.DOCX",
    )
    for (path in unwanted) assertFalse("$path should not match", matches(path))
  }

  @Test
  fun `every associated extension is accepted by the copy pipeline`() {
    val missing = typesByExtension.keys - IncomingFiles.openableExtensions
    assertTrue("IncomingFiles would reject: $missing", missing.isEmpty())
  }

  @Test
  fun `the generator no longer owns the file associations`() {
    // tauri-build rewrites the block between these markers on every Android
    // build unless bundle.fileAssociations is empty for Android.
    assertFalse(manifestFile.readText().contains("tauri-file-associations"))
    val config = locate(
      "../../tauri.android.conf.json",
      "src-tauri/tauri.android.conf.json",
      "../../../tauri.android.conf.json",
    ).readText()
    assertTrue(Regex("\"fileAssociations\"\\s*:\\s*\\[\\s*\\]").containsMatchIn(config))
  }
}

/**
 * Port of android.os.PatternMatcher.matchGlobPattern (PATTERN_SIMPLE_GLOB, the
 * semantics of android:pathPattern), so the JVM test can check the patterns
 * without a device. Notably `.*` consumes up to the FIRST occurrence of the
 * next pattern character and never backtracks.
 */
internal object AndroidGlob {
  fun matches(pattern: String, match: String): Boolean {
    val np = pattern.length
    val nm = match.length
    if (np <= 0) return nm <= 0
    var ip = 0
    var im = 0
    var nextChar = pattern[0]
    while (ip < np && im < nm) {
      var c = nextChar
      ip++
      nextChar = if (ip < np) pattern[ip] else '\u0000'
      val escaped = c == '\\'
      if (escaped) {
        c = nextChar
        ip++
        nextChar = if (ip < np) pattern[ip] else '\u0000'
      }
      if (nextChar == '*') {
        if (!escaped && c == '.') {
          // ".*" at the end of the pattern matches whatever is left.
          if (ip >= np - 1) return true
          ip++
          nextChar = pattern[ip]
          // Consume everything until the next pattern character is found.
          if (nextChar == '\\') {
            ip++
            nextChar = if (ip < np) pattern[ip] else '\u0000'
          }
          while (true) {
            if (match[im] == nextChar) break
            im++
            if (im >= nm) break
          }
          if (im == nm) return false
          ip++
          nextChar = if (ip < np) pattern[ip] else '\u0000'
          im++
        } else {
          // Consume only characters equal to the one before '*'.
          while (true) {
            if (match[im] != c) break
            im++
            if (im >= nm) break
          }
          ip++
          nextChar = if (ip < np) pattern[ip] else '\u0000'
        }
      } else {
        if (c != '.' && match[im] != c) return false
        im++
      }
    }
    if (ip >= np && im >= nm) return true
    // A trailing ".*" still matches when the text ran out first.
    return ip == np - 2 && pattern[ip] == '.' && pattern[ip + 1] == '*'
  }
}
