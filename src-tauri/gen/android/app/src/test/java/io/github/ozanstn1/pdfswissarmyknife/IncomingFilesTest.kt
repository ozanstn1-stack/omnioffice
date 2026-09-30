package io.github.ozanstn1.pdfswissarmyknife

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream

/**
 * JVM unit tests for the open-with intent pipeline's untrusted-input handling.
 * They run on every Android build (./gradlew :app:testDebugUnitTest) and cover
 * the logic that used to be untestable inside the Activity.
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
}
