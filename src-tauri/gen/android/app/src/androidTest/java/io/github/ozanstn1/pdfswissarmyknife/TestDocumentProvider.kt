package io.github.ozanstn1.pdfswissarmyknife

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

/**
 * Minimal in-process document provider used by the instrumentation tests to
 * hand the app a real `content://` stream plus an `OpenableColumns` display
 * name - the same shape Android's SAF providers expose.
 *
 * The provider is declared in the androidTest manifest, so its own `context`
 * is the test app; fixtures staged by `TestDocumentProvider.stage` land in that
 * same cache directory (the tests reach it through the instrumentation
 * context).
 */
class TestDocumentProvider : ContentProvider() {
  companion object {
    const val AUTHORITY = "io.github.ozanstn1.pdfswissarmyknife.testdocuments"

    fun uriFor(name: String): Uri = Uri.parse("content://$AUTHORITY/$name")

    private fun directory(root: File): File = File(root, "testdocuments").apply { mkdirs() }

    /** Writes the fixture a test will stream through the provider. */
    fun stage(root: File, name: String, body: ByteArray) {
      FileOutputStream(File(directory(root), name)).use { it.write(body) }
    }
  }

  private fun fileFor(name: String): File? {
    val root = context?.cacheDir ?: return null
    val file = File(directory(root), name)
    return if (file.isFile) file else null
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
