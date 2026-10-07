package io.github.ozanstn1.pdfswissarmyknife

import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream

/**
 * On-device instrumentation tests for the open-with / share intent pipeline.
 *
 * They run on an emulator (`./gradlew :app:connectedUniversalDebugAndroidTest`)
 * and exercise the same `ContentResolver` -> [IncomingFiles.copyToCache] path
 * the Activity uses for an incoming intent, without booting the Tauri WebView:
 * display-name sanitization, the extension whitelist, the size cap and the
 * queued cache file are all verified with real Android I/O.
 *
 * Routing: the second half asks the real PackageManager which intents the
 * platform resolver hands to this app (`queryIntentActivities`), with
 * content:// and file:// URIs the way file managers send them. The Tauri
 * generated filters (a bare pathPattern per extension) never matched any of
 * them, so Office documents could not be opened from other apps.
 *
 * SAF note: the picker itself (`ACTION_OPEN_DOCUMENT`) needs the app UI and the
 * in-process provider path is covered by the JVM tests, so a full pick round
 * trip is not automated here; the copy/hand-off contract it feeds is.
 */
@RunWith(AndroidJUnit4::class)
class OpenWithIntentTest {
  private val context = InstrumentationRegistry.getInstrumentation().targetContext
  private val cacheDir: File get() = context.cacheDir
  private val resolver get() = context.contentResolver

  @Before
  fun clearStaging() {
    File(cacheDir, "incoming").deleteRecursively()
    File(cacheDir, "testdocuments").deleteRecursively()
  }

  /**
   * Stages a fixture and returns its `file://` URI. Android's content resolver
   * reads `file://` through the same `openInputStream` path a `content://`
   * provider uses, so the Activity's copy engine is exercised on-device
   * without depending on an in-process DocumentsProvider.
   */
  private fun staged(name: String, body: ByteArray): Uri {
    val directory = File(cacheDir, "testdocuments").apply { mkdirs() }
    val file = File(directory, name)
    FileOutputStream(file).use { it.write(body) }
    return Uri.fromFile(file)
  }

  @Test
  fun openableDocumentIsCopiedIntoTheCache() {
    val body = "hello open-with".toByteArray()
    val uri = staged("report.pdf", body)
    val (file, result) = IncomingFiles.copyToCache(resolver, uri, cacheDir)
    assertEquals(IncomingFiles.CopyResult.COPIED, result)
    assertNotNull(file)
    assertTrue("the cache copy is an absolute path", file!!.isAbsolute)
    assertEquals("the cache copy keeps the display name", "report.pdf", file.name)
    assertTrue("the copy lives under incoming/", file.parentFile!!.canonicalPath.contains("incoming"))
    assertEquals("the copied bytes match the source", body.toList(), file.readBytes().toList())
  }

  @Test
  fun executableExtensionsAreRejected() {
    val uri = staged("installer.exe", "MZ".toByteArray())
    val (file, result) = IncomingFiles.copyToCache(resolver, uri, cacheDir)
    assertNull("an .exe must never be copied", file)
    assertEquals(IncomingFiles.CopyResult.UNSUPPORTED, result)
  }

  @Test
  fun unknownExtensionsAreRejected() {
    val uri = staged("archive.zip", byteArrayOf(1, 2, 3))
    val (file, result) = IncomingFiles.copyToCache(resolver, uri, cacheDir)
    assertNull(file)
    assertEquals(IncomingFiles.CopyResult.UNSUPPORTED, result)
  }

  @Test
  fun missingFileIsReportedNotCopied() {
    val missing = Uri.fromFile(File(cacheDir, "testdocuments/ghost.pdf"))
    val (file, result) = IncomingFiles.copyToCache(resolver, missing, cacheDir)
    assertNull(file)
    assertTrue(result == IncomingFiles.CopyResult.NO_STREAM || result == IncomingFiles.CopyResult.FAILED)
  }

  @Test
  fun oversizedStreamsAreStoppedByTheCopyCap() {
    val limit = 1024L
    val payload = ByteArray((limit + 1).toInt()) { 7 }
    val output = ByteArrayOutputStream()
    assertTrue(
      "the copy cap must stop streams over the limit",
      !IncomingFiles.copyCapped(payload.inputStream(), output, limit),
    )
    assertTrue("nothing past the cap is written", output.size() <= limit)
  }

  @Test
  fun aNormalPdfStillFlowsThroughTheRealCopyPath() {
    val uri = staged("ok.pdf", byteArrayOf(1, 2, 3))
    val (file, result) = IncomingFiles.copyToCache(resolver, uri, cacheDir)
    assertEquals(IncomingFiles.CopyResult.COPIED, result)
    assertNotNull(file)
  }

  // ---------------------------------------------------------------------------
  // Intent routing (AndroidManifest.xml intent filters)
  // ---------------------------------------------------------------------------

  /** Extension -> the MIME types senders use for it (see the typed filter). */
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

  /** URIs in the shapes the system file picker, mail apps and old file managers produce. */
  private fun documentUris(extension: String): List<String> = listOf(
    "content://com.android.providers.downloads.documents/document/msf%3A42",
    "content://com.android.externalstorage.documents/document/primary%3ADownload%2FMy.Report.v2.$extension",
    "content://com.example.mail.attachments/message/7/report.$extension",
    "file:///storage/emulated/0/Download/report.$extension",
  )

  /** Names that carry the extension in the path, with and without extra dots. */
  private fun namedUris(extension: String): List<String> = listOf(
    "content://com.example.docs/files/report.$extension",
    "content://com.example.docs/files/My.Report.v2.$extension",
    "content://com.android.externalstorage.documents/document/primary%3ADownload%2Fv1.2%2FMy.Report.$extension",
    "file:///storage/emulated/0/Download/report.$extension",
    "file:///storage/emulated/0/Download/My.Report.v2.$extension",
  )

  private fun viewIntent(uri: String, type: String?): Intent {
    val intent = Intent(Intent.ACTION_VIEW)
    if (type == null) intent.data = Uri.parse(uri) else intent.setDataAndType(Uri.parse(uri), type)
    return intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
  }

  /** True when the platform resolver offers MainActivity for [intent]. */
  @Suppress("DEPRECATION")
  private fun handledByApp(intent: Intent): Boolean {
    val matches = context.packageManager.queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
    return matches.any { it.activityInfo.packageName == context.packageName && it.activityInfo.name.endsWith(".MainActivity") }
  }

  @Test
  fun typedViewIntentsForEveryFormatReachTheApp() {
    for ((extension, types) in typesByExtension) {
      for (type in types) {
        for (uri in documentUris(extension)) {
          assertTrue("VIEW $uri as $type is not routed to the app", handledByApp(viewIntent(uri, type)))
        }
      }
    }
  }

  @Test
  fun shareIntentsForEveryFormatReachTheApp() {
    for ((extension, types) in typesByExtension) {
      for (type in types) {
        val stream = Uri.parse("content://com.example.mail.attachments/message/7/report.$extension")
        val send = Intent(Intent.ACTION_SEND).setType(type).putExtra(Intent.EXTRA_STREAM, stream)
        assertTrue("SEND $type is not routed to the app", handledByApp(send))
        val many = Intent(Intent.ACTION_SEND_MULTIPLE).setType(type)
          .putParcelableArrayListExtra(Intent.EXTRA_STREAM, arrayListOf(stream, stream))
        assertTrue("SEND_MULTIPLE $type is not routed to the app", handledByApp(many))
      }
    }
  }

  @Test
  fun genericallyTypedFilesAreMatchedByTheirName() {
    // Many managers send application/octet-stream for formats they do not know,
    // and .oswk has no registered type at all.
    for (extension in typesByExtension.keys) {
      for (uri in namedUris(extension)) {
        assertTrue("octet-stream $uri is not routed to the app", handledByApp(viewIntent(uri, "application/octet-stream")))
      }
    }
  }

  @Test
  fun untypedFilesAreMatchedByTheirName() {
    // No type: the provider could not tell, or the sender used a bare file:// URI.
    for (extension in typesByExtension.keys) {
      for (uri in namedUris(extension)) {
        assertTrue("untyped $uri is not routed to the app", handledByApp(viewIntent(uri, null)))
      }
    }
  }

  @Test
  fun unrelatedFilesAreNotClaimed() {
    val unrelated = listOf(
      Triple("content://com.example.docs/files/setup.exe", "application/octet-stream", "an executable"),
      Triple("content://com.example.docs/files/app.apk", "application/vnd.android.package-archive", "an apk"),
      Triple("content://com.example.docs/files/archive.zip", "application/zip", "a zip"),
      Triple("content://com.example.docs/files/report.docx.exe", "application/octet-stream", "a double extension"),
      Triple("file:///storage/emulated/0/Download/notes.zip", null, "an untyped zip"),
    )
    for ((uri, type, what) in unrelated) {
      assertFalse("$what ($uri) must not be routed to the app", handledByApp(viewIntent(uri, type)))
    }
  }
}
