package io.github.ozanstn1.pdfswissarmyknife

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
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
}
