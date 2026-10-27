package io.github.ozanstn1.pdfswissarmyknife

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/**
 * JVM unit tests for the open-with intent pipeline's untrusted-input handling.
 * They run on every Android build (./gradlew :app:testDebugUnitTest) and cover
 * the logic that used to be untestable inside the Activity.
 *
 * The acceptance path has three stages - provider display name, MIME type,
 * copied bytes - so the tests cover the MIME table, the provisional cache
 * name and every sniffing branch with synthetic files (no emulator needed).
 */
class IncomingFilesTest {
  @Test
  fun `sanitize keeps a plain document name`() {
    assertEquals("report.pdf", IncomingFiles.sanitizeDisplayName("report.pdf"))
    assertEquals("spaced name.pdf", IncomingFiles.sanitizeDisplayName("  spaced name.pdf  "))
  }

  @Test
  fun `sanitize strips path separators and traversal`() {
    assertEquals("passwd", IncomingFiles.sanitizeDisplayName("../../etc/passwd"))
    assertEquals("evil.pdf", IncomingFiles.sanitizeDisplayName("C:\\Users\\a\\..\\evil.pdf"))
    assertEquals("evil.pdf", IncomingFiles.sanitizeDisplayName("..\\..\\evil.pdf"))
    assertEquals("_hidden.pdf", IncomingFiles.sanitizeDisplayName("..hidden.pdf"))
  }

  @Test
  fun `sanitize replaces windows reserved characters`() {
    assertEquals("a_b_c.pdf", IncomingFiles.sanitizeDisplayName("a:b*c.pdf"))
    assertEquals("weird_.pdf", IncomingFiles.sanitizeDisplayName("weird?.pdf"))
  }

  @Test
  fun `sanitize rejects empty and dot-only names`() {
    assertNull(IncomingFiles.sanitizeDisplayName(null))
    assertNull(IncomingFiles.sanitizeDisplayName(""))
    assertNull(IncomingFiles.sanitizeDisplayName("   "))
    assertNull(IncomingFiles.sanitizeDisplayName("."))
    assertNull(IncomingFiles.sanitizeDisplayName("/"))
    // Traversal runs are neutralized rather than kept as a hidden file name.
    assertEquals("_", IncomingFiles.sanitizeDisplayName(".."))
    assertEquals("_.", IncomingFiles.sanitizeDisplayName("..."))
  }

  @Test
  fun `openable extensions accept routable documents case-insensitively`() {
    assertTrue(IncomingFiles.isOpenableName("report.pdf"))
    assertTrue(IncomingFiles.isOpenableName("REPORT.PDF"))
    assertTrue(IncomingFiles.isOpenableName("notes.OSWK"))
    assertTrue(IncomingFiles.isOpenableName("photo.jpeg"))
    assertTrue(IncomingFiles.isOpenableName("sheet.xlsx"))
    assertTrue(IncomingFiles.isOpenableName("macro.xlsm"))
    assertTrue(IncomingFiles.isOpenableName("legacy.xls"))
    assertTrue(IncomingFiles.isOpenableName("invoice.xml"))
    assertTrue(IncomingFiles.isOpenableName("mail.eml"))
    assertTrue(IncomingFiles.isOpenableName("archive.zip"))
    // Legacy/Word-template formats the office backend reads.
    assertTrue(IncomingFiles.isOpenableName("old.doc"))
    assertTrue(IncomingFiles.isOpenableName("template.dotx"))
    assertTrue(IncomingFiles.isOpenableName("macro.docm"))
    assertTrue(IncomingFiles.isOpenableName("slides.pptm"))
    assertTrue(IncomingFiles.isOpenableName("old.ppt"))
  }

  @Test
  fun `sanitize strips control characters that would split the open queue`() {
    // The pending-open queue is one path per line; a provider name with an
    // inner newline used to inject a second, bogus entry.
    assertEquals("report_.docx", IncomingFiles.sanitizeDisplayName("report\n.docx"))
    assertEquals("a_b.pdf", IncomingFiles.sanitizeDisplayName("a\u0007b.pdf"))
    assertEquals("tab_.pdf", IncomingFiles.sanitizeDisplayName("tab\t.pdf"))
  }

  @Test
  fun `openable extensions reject executables and unknown types`() {
    assertFalse(IncomingFiles.isOpenableName("installer.exe"))
    assertFalse(IncomingFiles.isOpenableName("app.apk"))
    assertFalse(IncomingFiles.isOpenableName("script.sh"))
    assertFalse(IncomingFiles.isOpenableName("noextension"))
    assertFalse(IncomingFiles.isOpenableName(""))
  }

  @Test
  fun `copyCapped copies a small stream`() {
    val payload = ByteArray(1024) { (it % 251).toByte() }
    val output = ByteArrayOutputStream()
    assertTrue(IncomingFiles.copyCapped(ByteArrayInputStream(payload), output, 2048))
    assertTrue(payload.contentEquals(output.toByteArray()))
  }

  @Test
  fun `copyCapped refuses a stream over the limit and never writes past it`() {
    val payload = ByteArray(4096) { 1 }
    val output = ByteArrayOutputStream()
    assertFalse(IncomingFiles.copyCapped(ByteArrayInputStream(payload), output, 1024))
    assertTrue(output.size() <= 1024)
  }

  @Test
  fun `saveSharedText writes plain text into incoming cache directory`() {
    withTempDir { root ->
      val saved = IncomingFiles.saveSharedText("Merhaba Dünya", "Not Başlığı", root)
      assertTrue(saved != null && saved.isFile)
      assertEquals("Not Başlığı.txt", saved!!.name)
      assertEquals("Merhaba Dünya", saved.readText(Charsets.UTF_8))
      assertNull(IncomingFiles.saveSharedText("   ", null, root))
    }
  }

  // ---------------------------------------------------------------------------
  // MIME -> extension (stage 2 of the acceptance order)
  // ---------------------------------------------------------------------------

  @Test
  fun `every supported mime type maps to a routable extension`() {
    val expected = mapOf(
      "application/pdf" to "pdf",
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document" to "docx",
      "application/vnd.ms-word.document.macroEnabled.12" to "docm",
      "application/vnd.openxmlformats-officedocument.wordprocessingml.template" to "dotx",
      "application/msword" to "doc",
      "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" to "xlsx",
      "application/vnd.ms-excel.sheet.macroEnabled.12" to "xlsm",
      "application/vnd.ms-excel" to "xls",
      "application/vnd.openxmlformats-officedocument.presentationml.presentation" to "pptx",
      "application/vnd.ms-powerpoint.presentation.macroEnabled.12" to "pptm",
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
      "image/png" to "png",
      "image/jpeg" to "jpg",
      "image/webp" to "webp",
      "image/bmp" to "bmp",
      "image/gif" to "gif",
      "image/tiff" to "tif",
    )
    for ((mime, extension) in expected) {
      assertEquals(mime, extension, IncomingFiles.extensionForMime(mime))
    }
  }

  @Test
  fun `mime lookup ignores case and parameters`() {
    assertEquals("pdf", IncomingFiles.extensionForMime("APPLICATION/PDF"))
    assertEquals("pdf", IncomingFiles.extensionForMime("application/pdf; charset=binary"))
    assertEquals("docm", IncomingFiles.extensionForMime("application/vnd.ms-word.document.macroEnabled.12"))
  }

  @Test
  fun `octet-stream and unknown mime types need content sniffing`() {
    // WhatsApp and download managers frequently declare nothing useful; the
    // pipeline must not treat that as "unsupported" before reading the bytes.
    assertNull(IncomingFiles.extensionForMime("application/octet-stream"))
    assertNull(IncomingFiles.extensionForMime("application/x-unknown-container"))
    assertNull(IncomingFiles.extensionForMime(null))
    assertNull(IncomingFiles.extensionForMime(""))
    assertNull(IncomingFiles.extensionForMime("   "))
  }

  @Test
  fun `provisional names reuse a routable provider name`() {
    assertEquals("report.pdf", IncomingFiles.provisionalName("report.pdf", "application/pdf"))
    // The name wins even when the sender declares a generic type.
    assertEquals("sheet.xls", IncomingFiles.provisionalName("sheet.xls", "application/octet-stream"))
  }

  @Test
  fun `provisional names fall back to the mime extension`() {
    val pdf = IncomingFiles.provisionalName("IMG-0001", "application/pdf")
    assertTrue("a mime named copy is marked provisional", pdf.startsWith("shared-"))
    assertTrue("the mime gives the provisional extension", pdf.endsWith(".pdf"))
    assertTrue(IncomingFiles.isOpenableName(pdf))

    assertTrue(IncomingFiles.provisionalName(null, "application/vnd.ms-excel").endsWith(".xls"))
    assertTrue(IncomingFiles.provisionalName("scan", "image/jpeg").endsWith(".jpg"))
  }

  @Test
  fun `provisional names end in bin when the type says nothing`() {
    val unknown = IncomingFiles.provisionalName(null, "application/octet-stream")
    assertTrue(unknown.startsWith("shared-"))
    assertTrue(unknown.endsWith(".bin"))
    // `.bin` is not routable: the copy exists only so it can be sniffed.
    assertFalse(IncomingFiles.isOpenableName(unknown))

    // A known-bad extension does not stop the copy either; the bytes decide.
    val exe = IncomingFiles.provisionalName("installer.exe", "application/octet-stream")
    assertTrue(exe.startsWith("shared-"))
    assertTrue(exe.endsWith(".bin"))
  }

  @Test
  fun `accepted extension prefers content and falls back to the mime type`() {
    // The bytes win: a mislabelled DOCX is routed as a DOCX, not as a PDF.
    assertEquals("xlsx", IncomingFiles.acceptedExtension("xlsx", "application/pdf"))
    assertEquals("pdf", IncomingFiles.acceptedExtension("pdf", "image/jpeg"))
    // Images are not sniffed; their sender-declared type is the signal.
    assertEquals("jpg", IncomingFiles.acceptedExtension(null, "image/jpeg"))
    assertEquals("png", IncomingFiles.acceptedExtension(null, "image/png"))
    // Neither the content nor the declared type identifies the file.
    assertNull(IncomingFiles.acceptedExtension(null, "application/octet-stream"))
    assertNull(IncomingFiles.acceptedExtension(null, null))
    assertNull(IncomingFiles.acceptedExtension(null, "application/x-unknown"))
    // A non-routable sniff result never hides a usable MIME type.
    assertEquals("pdf", IncomingFiles.acceptedExtension("bin", "application/pdf"))
  }

  // ---------------------------------------------------------------------------
  // Content sniffing (stage 3 of the acceptance order)
  // ---------------------------------------------------------------------------

  @Test
  fun `sniff recognises a pdf by its magic`() {
    withTempDir { root ->
      val file = writeTemp(root, "shared", "%PDF-1.7\n1 0 obj\n<<>>\nendobj\n".toByteArray())
      assertEquals("pdf", IncomingFiles.sniffExtension(file))
    }
  }

  @Test
  fun `sniff recognises ooxml zips by their part layout`() {
    withTempDir { root ->
      assertEquals("docx", IncomingFiles.sniffExtension(zipFile(root, "word.docx", mapOf("word/document.xml" to "<w/>"))))
      assertEquals(
        "xlsx",
        IncomingFiles.sniffExtension(zipFile(root, "sheet.xlsx", mapOf("xl/workbook.xml" to "<w/>"))),
      )
      assertEquals(
        "pptx",
        IncomingFiles.sniffExtension(zipFile(root, "slides.pptx", mapOf("ppt/presentation.xml" to "<p/>"))),
      )
    }
  }

  @Test
  fun `sniff recognises odf documents by their mimetype entry`() {
    withTempDir { root ->
      val odt = zipFile(
        root,
        "text.odt",
        mapOf(
          "mimetype" to "application/vnd.oasis.opendocument.text",
          "content.xml" to "<office/>",
        ),
      )
      val ods = zipFile(
        root,
        "sheet.ods",
        mapOf(
          "mimetype" to "application/vnd.oasis.opendocument.spreadsheet",
          "content.xml" to "<office/>",
        ),
      )
      val odp = zipFile(
        root,
        "slides.odp",
        mapOf(
          "mimetype" to "application/vnd.oasis.opendocument.presentation",
          "content.xml" to "<office/>",
        ),
      )
      assertEquals("odt", IncomingFiles.sniffExtension(odt))
      assertEquals("ods", IncomingFiles.sniffExtension(ods))
      assertEquals("odp", IncomingFiles.sniffExtension(odp))
    }
  }

  @Test
  fun `sniff recognises legacy compound documents by their stream markers`() {
    withTempDir { root ->
      assertEquals("doc", IncomingFiles.sniffExtension(compoundFile(root, "word.doc", "WordDocument")))
      assertEquals("xls", IncomingFiles.sniffExtension(compoundFile(root, "biff8.xls", "Workbook")))
      assertEquals("xls", IncomingFiles.sniffExtension(compoundFile(root, "biff5.xls", "Book")))
      assertEquals("ppt", IncomingFiles.sniffExtension(compoundFile(root, "slides.ppt", "PowerPoint Document")))
    }
  }

  @Test
  fun `sniff keeps xls accepted as an openable legacy document`() {
    withTempDir { root ->
      val xls = compoundFile(root, "legacy.xls", "Workbook")
      assertEquals("xls", IncomingFiles.sniffExtension(xls))
      assertTrue("the sniffed extension is routable", IncomingFiles.isOpenableExtension("xls"))
      assertTrue(IncomingFiles.isOpenableName("copy.xls"))
    }
  }

  @Test
  fun `sniff reads only a bounded prefix of a padded mimetype entry`() {
    withTempDir { root ->
      // 645 bytes: past the 256-byte read window but inside the declared-size
      // guard. The entry is never materialised, yet the media type prefix
      // still identifies the document.
      val mimetype = "application/vnd.oasis.opendocument.text" + "x".repeat(600)
      val odt = zipFile(
        root,
        "padded.odt",
        mapOf("mimetype" to mimetype, "content.xml" to "<office/>"),
      )
      assertEquals("odt", IncomingFiles.sniffExtension(odt))
    }
  }

  @Test
  fun `sniff rejects a huge mimetype entry with no other markers`() {
    withTempDir { root ->
      // A hostile archive can declare a huge `mimetype` entry; the copy cap
      // only bounds the source stream, so the sniff must never materialise the
      // decompressed entry. Unknown content is rejected, not thrown.
      val hostile = zipFile(root, "hostile.odt", mapOf("mimetype" to "x".repeat(4096)))
      assertNull(IncomingFiles.sniffExtension(hostile))
    }
  }

  @Test
  fun `sniff skips mimetype entries above the declared-size guard`() {
    withTempDir { root ->
      // A conformant ODF mimetype is ~45 bytes; a 4 KB one is not conformant
      // and is dropped before decompression, even when its prefix looks right.
      // The hard allocation bound is the fixed read window below the guard.
      val oversized = zipFile(
        root,
        "oversized.odt",
        mapOf(
          "mimetype" to "application/vnd.oasis.opendocument.text" + "x".repeat(4096),
          "content.xml" to "<office/>",
        ),
      )
      assertNull(IncomingFiles.sniffExtension(oversized))
    }
  }

  @Test
  fun `sniff recognises rtf, eml, xml and plain text`() {
    withTempDir { root ->
      val rtf = writeTemp(root, "note.rtf", "{\\rtf1\\ansi Merhaba}".toByteArray())
      val eml = writeTemp(
        root,
        "mail.eml",
        (
          "From: sender@example.com\r\nTo: reader@example.com\r\nSubject: Merhaba\r\n" +
            "MIME-Version: 1.0\r\n\r\nBody text\r\n"
          ).toByteArray(),
      )
      val emlWithoutMimeVersion = writeTemp(
        root,
        "mail2.eml",
        "From: sender@example.com\nSubject: Merhaba\n\nBody\n".toByteArray(),
      )
      val xml = writeTemp(root, "feed.xml", "<?xml version=\"1.0\"?><root/>".toByteArray())
      val xmlWithBom = writeTemp(
        root,
        "bom.xml",
        byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()) + "<?xml version=\"1.0\"?><root/>".toByteArray(),
      )
      val text = writeTemp(root, "note.txt", "Merhaba dünya\nİkinci satır\n".toByteArray())
      assertEquals("rtf", IncomingFiles.sniffExtension(rtf))
      assertEquals("eml", IncomingFiles.sniffExtension(eml))
      assertEquals("eml", IncomingFiles.sniffExtension(emlWithoutMimeVersion))
      assertEquals("xml", IncomingFiles.sniffExtension(xml))
      assertEquals("xml", IncomingFiles.sniffExtension(xmlWithBom))
      assertEquals("txt", IncomingFiles.sniffExtension(text))
    }
  }

  @Test
  fun `sniff recognises delimited text only with a consistent shape`() {
    withTempDir { root ->
      val csv = writeTemp(root, "table.csv", "name,age\nAda,36\nGrace,85\n".toByteArray())
      val tsv = writeTemp(root, "table.tsv", "name\tage\nAda\t36\nGrace\t85\n".toByteArray())
      // Prose that happens to contain commas is a text document, not a table:
      // the line shape is not consistent.
      val prose = writeTemp(
        root,
        "prose.txt",
        "Merhaba, dünya\nBugün nasılsın, dostum?\nİyiyim\n".toByteArray(),
      )
      assertEquals("csv", IncomingFiles.sniffExtension(csv))
      assertEquals("tsv", IncomingFiles.sniffExtension(tsv))
      assertEquals("txt", IncomingFiles.sniffExtension(prose))
    }
  }

  @Test
  fun `sniff rejects unknown binary and executable content`() {
    withTempDir { root ->
      val binary = writeTemp(root, "blob", byteArrayOf(0, 1, 2, 3, 4, 0, 5, 6, 7))
      val executable = writeTemp(
        root,
        "setup",
        "MZ\u0090\u0000\u0003\u0000\u0000\u0000".toByteArray(Charsets.ISO_8859_1),
      )
      val empty = writeTemp(root, "empty", ByteArray(0))
      assertNull("binary bytes cannot be routed", IncomingFiles.sniffExtension(binary))
      assertNull("an executable must not be sniffed as text", IncomingFiles.sniffExtension(executable))
      assertNull("an empty file has no type", IncomingFiles.sniffExtension(empty))
    }
  }

  @Test
  fun `sniff cannot identify a generic zip container`() {
    withTempDir { root ->
      // A plain archive is not a document; without a display name or a mime
      // type there is no tool to route it to.
      val zip = zipFile(root, "archive.zip", mapOf("readme.txt" to "hello"))
      assertNull(IncomingFiles.sniffExtension(zip))
    }
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  private fun withTempDir(block: (File) -> Unit) {
    val root = java.nio.file.Files.createTempDirectory("pdfsak-incoming").toFile()
    try {
      block(root)
    } finally {
      root.deleteRecursively()
    }
  }

  private fun writeTemp(root: File, name: String, body: ByteArray): File =
    File(root, name).apply { writeBytes(body) }

  private fun zipFile(root: File, name: String, entries: Map<String, String>): File {
    val output = ByteArrayOutputStream()
    ZipOutputStream(output).use { zip ->
      for ((entryName, content) in entries) {
        zip.putNextEntry(ZipEntry(entryName))
        zip.write(content.toByteArray(Charsets.UTF_8))
        zip.closeEntry()
      }
    }
    return writeTemp(root, name, output.toByteArray())
  }

  /** A minimal OLE/CFB header plus one UTF-16LE directory entry name. */
  private fun compoundFile(root: File, name: String, marker: String): File {
    val output = ByteArrayOutputStream()
    output.write(
      byteArrayOf(
        0xD0.toByte(), 0xCF.toByte(), 0x11, 0xE0.toByte(), 0xA1.toByte(), 0xB1.toByte(), 0x1A, 0xE1.toByte(),
      ),
    )
    output.write(ByteArray(64))
    output.write(marker.toByteArray(Charsets.UTF_16LE))
    output.write(ByteArray(64))
    return writeTemp(root, name, output.toByteArray())
  }

  @Test
  fun `the document cache drops folders older than the limit and keeps the rest`() {
    val cache = kotlin.io.path.createTempDirectory("pdfsak-prune").toFile()
    val now = 1_000_000_000_000L
    val day = 24L * 60 * 60 * 1000
    fun folder(root: String, name: String, ageDays: Long): File {
      val dir = File(File(cache, root), name).apply { mkdirs() }
      File(dir, "doc.pdf").apply { writeText("%PDF") }.setLastModified(now - ageDays * day)
      dir.setLastModified(now - ageDays * day)
      return dir
    }
    val oldIncoming = folder("incoming", "old", 20)
    val freshIncoming = folder("incoming", "fresh", 2)
    val oldShare = folder("share", "old", 30)
    val oldScan = folder("scans", "old", 15)
    val queue = File(cache, "pending-open.txt").apply { writeText("") }

    assertEquals(3, IncomingFiles.pruneDocumentCache(cache, now, 14 * day))
    assertFalse(oldIncoming.exists())
    assertFalse(oldShare.exists())
    assertFalse("scanner page photos are swept too", oldScan.exists())
    assertTrue(freshIncoming.exists())
    assertTrue("files next to the folders are not touched", queue.exists())
    assertEquals(0, IncomingFiles.pruneDocumentCache(File(cache, "missing"), now))
    cache.deleteRecursively()
  }

  @Test
  fun `old camera captures are removed, other pictures stay`() {
    // The WebView's capture chooser writes every photo to
    // getExternalFilesDir(Pictures)/JPEG_<time>_<n>.jpg and never deletes it;
    // the scanner keeps its own copy in cacheDir/scans.
    val pictures = kotlin.io.path.createTempDirectory("pdfsak-pictures").toFile()
    val now = 1_000_000_000_000L
    val day = 24L * 60 * 60 * 1000
    fun file(name: String, ageDays: Long) =
      File(pictures, name).apply { writeText("x") }.also { it.setLastModified(now - ageDays * day) }
    val oldCapture = file("JPEG_20261027_101010_123.jpg", 3)
    val freshCapture = file("JPEG_20261030_101010_456.jpg", 0)
    val other = file("signature.png", 30)

    assertEquals(1, IncomingFiles.pruneCapturePhotos(pictures, now))
    assertFalse(oldCapture.exists())
    assertTrue("a capture still being staged is kept", freshCapture.exists())
    assertTrue(other.exists())
    assertEquals(0, IncomingFiles.pruneCapturePhotos(null, now))
    pictures.deleteRecursively()
  }
}
