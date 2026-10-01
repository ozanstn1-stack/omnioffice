package io.github.ozanstn1.pdfswissarmyknife

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

/**
 * On-device instrumentation tests for the open-with / share intent pipeline.
 *
 * They exercise the real `content://` path on an emulator
 * (`./gradlew :app:connectedUniversalDebugAndroidTest`): [TestDocumentProvider]
 * serves a real stream and [IncomingFiles.copyToCache] performs the copy the
 * Activity runs for an incoming intent. The display-name/whitelist/size rules
 * and the queued cache file are all verified end to end, without booting the
 * Tauri WebView.
 *
 * SAF note: the picker itself (`ACTION_OPEN_DOCUMENT`) still needs the app UI,
 * so a full pick round trip is not automated here; the copy/hand-off contract
 * the SAF result flows through is.
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

  @Test
  fun openableDocumentIsCopiedIntoTheCache() {
    val body = "hello open-with".toByteArray()
    TestDocumentProvider.stage(cacheDir, "report.pdf", body)
    val (file, result) = IncomingFiles.copyToCache(resolver, TestDocumentProvider.uriFor("report.pdf"), cacheDir)
    assertEquals(IncomingFiles.CopyResult.COPIED, result)
    assertNotNull(file)
    assertTrue("the cache copy is an absolute path", file!!.isAbsolute)
    assertEquals("the cache copy keeps the display name", "report.pdf", file.name)
    assertTrue("the copy lives under incoming/", file.parentFile!!.canonicalPath.contains("incoming"))
    assertEquals("the copied bytes match the source", body.toList(), file.readBytes().toList())
  }

  @Test
  fun executableExtensionsAreRejected() {
    TestDocumentProvider.stage(cacheDir, "installer.exe", "MZ".toByteArray())
    val (file, result) = IncomingFiles.copyToCache(resolver, TestDocumentProvider.uriFor("installer.exe"), cacheDir)
    assertNull("an .exe must never be copied", file)
    assertEquals(IncomingFiles.CopyResult.UNSUPPORTED, result)
  }

  @Test
  fun unknownExtensionsAreRejected() {
    TestDocumentProvider.stage(cacheDir, "archive.zip", byteArrayOf(1, 2, 3))
    val (file, result) = IncomingFiles.copyToCache(resolver, TestDocumentProvider.uriFor("archive.zip"), cacheDir)
    assertNull(file)
    assertEquals(IncomingFiles.CopyResult.UNSUPPORTED, result)
  }

  @Test
  fun missingProviderStreamIsReportedNotCopied() {
    // No fixture staged for this name: openInputStream must fail cleanly.
    val (file, result) = IncomingFiles.copyToCache(resolver, TestDocumentProvider.uriFor("ghost.pdf"), cacheDir)
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
    TestDocumentProvider.stage(cacheDir, "ok.pdf", byteArrayOf(1, 2, 3))
    val (file, result) = IncomingFiles.copyToCache(resolver, TestDocumentProvider.uriFor("ok.pdf"), cacheDir)
    assertEquals(IncomingFiles.CopyResult.COPIED, result)
    assertNotNull(file)
  }
}
