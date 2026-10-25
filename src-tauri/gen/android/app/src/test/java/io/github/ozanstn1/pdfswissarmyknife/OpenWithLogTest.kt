package io.github.ozanstn1.pdfswissarmyknife

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * JVM tests for the durable open-with log.
 *
 * The log is the only diagnostic a user without adb can send us, so it has to
 * append complete timestamped lines and keep the newest ones when the file is
 * trimmed. These tests use a temporary directory; the activity only calls
 * [OpenWithLog.attach] with its real cacheDir.
 */
class OpenWithLogTest {
  private lateinit var directory: File

  @Before
  fun setUp() {
    directory = File.createTempFile("open-with-log", "").let { file ->
      file.delete()
      file.mkdirs()
      file
    }
    OpenWithLog.attach(directory)
  }

  @After
  fun tearDown() {
    directory.deleteRecursively()
  }

  private fun logFile(): File = File(directory, OpenWithLog.FILE_NAME)

  @Test
  fun `record appends one timestamped line`() {
    OpenWithLog.record("intent action=android.intent.action.VIEW type=application/pdf")
    val text = OpenWithLog.read()
    assertTrue(text, Regex("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}").containsMatchIn(text))
    assertTrue(text.contains("intent action=android.intent.action.VIEW type=application/pdf"))
  }

  @Test
  fun `records pile up in order`() {
    OpenWithLog.record("first")
    OpenWithLog.record("second")
    val lines = OpenWithLog.read().trimEnd().lines()
    assertEquals(2, lines.size)
    assertTrue(lines[0].endsWith("\tfirst"))
    assertTrue(lines[1].endsWith("\tsecond"))
  }

  @Test
  fun `line breaks inside a recorded value stay on one line`() {
    OpenWithLog.record("name=report\n.pdf")
    val lines = OpenWithLog.read().trimEnd().lines()
    assertEquals(1, lines.size)
    assertTrue(lines[0].endsWith("\tname=report .pdf"))
  }

  @Test
  fun `read is empty before anything is recorded`() {
    assertEquals("", OpenWithLog.read())
  }

  @Test
  fun `the file is trimmed to complete recent lines`() {
    // 300 * ~1 KB crosses the 128 KB cap several times over.
    val payload = "x".repeat(1000)
    repeat(300) { index -> OpenWithLog.record("$index-$payload") }
    val text = OpenWithLog.read()
    // A trim keeps 64 KB; the log may then grow back towards the 128 KB cap.
    assertTrue("log is ${text.length} chars", text.length <= 130 * 1024)
    assertTrue("the oldest entries were dropped", !text.contains("\t0-$payload"))
    // The first kept line must be complete: a timestamp, not a cut-off tail.
    assertTrue(text, Regex("^\\d{4}-\\d{2}-\\d{2}T").containsMatchIn(text))
    val last = text.trimEnd().lines().last()
    assertTrue(last, last.endsWith("\t299-$payload"))
  }
}
