package io.github.ozanstn1.pdfswissarmyknife

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.io.FileOutputStream

/**
 * Minimal in-process document provider used by the instrumentation tests to
 * hand [MainActivity] a real `content://` stream plus an `OpenableColumns`
 * display name - the same shape Android's SAF providers expose.
 *
 * The provider instance is created by the system, so test data is staged in
 * (and read from) the target app's cache directory via the instrumentation
 * registry rather than the provider's own `context`.
 */
class TestDocumentProvider : ContentProvider() {
  companion object {
    const val AUTHORITY = "io.github.ozanstn1.pdfswissarmyknife.testdocuments"

    fun uriFor(name: String): Uri = Uri.parse("content://$AUTHORITY/$name")

    private fun candidateRoots(): List<File> {
      val instrumentation = InstrumentationRegistry.getInstrumentation()
      return listOf(
        File(instrumentation.context.cacheDir, "testdocuments"),
        File(instrumentation.targetContext.cacheDir, "testdocuments"),
      )
    }

    /** Writes the fixture into every candidate root a provider might read. */
    fun stage(name: String, body: ByteArray) {
      val instrumentation = InstrumentationRegistry.getInstrumentation()
      val roots = listOf(
        File(instrumentation.context.cacheDir, "testdocuments"),
        File(instrumentation.targetContext.cacheDir, "testdocuments"),
      )
      for (directory in roots) {
        directory.mkdirs()
        FileOutputStream(File(directory, name)).use { it.write(body) }
      }
    }

    private fun fileFor(name: String): File? =
      candidateRoots().map { File(it, name) }.firstOrNull { it.isFile }
  }

  override fun onCreate(): Boolean = true

  override fun query(
    uri: Uri,
    projection: Array<out String>?,
    selection: String?,
    selectionArgs: Array<out String>?,
    sortOrder: String?,
  ): Cursor {
    val name = uri.lastPathSegment ?: ""
    val file = fileFor(name)
    val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
    val cursor = MatrixCursor(columns)
    cursor.addRow(columns.map { column ->
      when (column) {
        OpenableColumns.DISPLAY_NAME -> name
        OpenableColumns.SIZE -> file?.length() ?: 0L
        else -> null
      }
    })
    return cursor
  }

  override fun getType(uri: Uri): String = "application/octet-stream"

  override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor? {
    val name = uri.lastPathSegment ?: return null
    val file = fileFor(name) ?: return null
    return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
  }

  override fun insert(uri: Uri, values: ContentValues?): Uri? = null

  override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

  override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0
}
