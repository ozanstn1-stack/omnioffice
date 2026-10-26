package io.github.ozanstn1.pdfswissarmyknife

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.annotation.VisibleForTesting
import java.io.File

class MainActivity : TauriActivity() {
  // The generated TauriActivity disables the WebView back handling so plugins
  // can consume the button. The frontend records one history entry per screen
  // (lib/nav-history.ts); this flag makes the system back button walk that
  // stack and fall through to "finish the activity" once Home is reached.
  override val handleBackNavigation: Boolean = true

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    installTessdata()
    super.onCreate(savedInstanceState)
    // The durable open-with log must exist before the first intent is imported;
    // Settings reads it back through android_open_with_diagnostics.
    OpenWithLog.attach(cacheDir, packageInfoVersion())
    importOpenWithIntent(intent)
    // Old open-with and share copies; off the UI thread, failures are ignored.
    Thread {
      try {
        IncomingFiles.pruneDocumentCache(cacheDir, System.currentTimeMillis())
      } catch (error: Exception) {
        // A cache sweep must never stop the app from starting.
      }
    }.start()
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    // singleTask keeps one activity instance, so later VIEW/SEND intents arrive
    // here; getIntent() must be refreshed for the rest of the process.
    setIntent(intent)
    importOpenWithIntent(intent)
  }

  // ---------------------------------------------------------------------------
  // Open-with / share intents
  //
  // Documents from other apps normally arrive through OpenWithActivity, which
  // copies them while it holds the sender's read grant and then wakes this
  // activity with ACTION_OPEN_WITH_WAKE (nothing to import, the webview drains
  // the queue). Launcher shortcuts and intents addressed to MainActivity
  // directly still go through [OpenWithImport] here.
  // ---------------------------------------------------------------------------

  private fun importOpenWithIntent(intent: Intent?) {
    if (intent == null) return
    when (intent.action) {
      ACTION_OPEN_WITH_WAKE -> return
      ACTION_SHORTCUT_SCAN -> {
        OpenWithImport.appendPendingOpen(cacheDir, "__action:scan")
        return
      }
      ACTION_SHORTCUT_NEW_DOCUMENT -> {
        OpenWithImport.appendPendingOpen(cacheDir, "__action:new_document")
        return
      }
      ACTION_SHORTCUT_RECENT_FILES -> {
        OpenWithImport.appendPendingOpen(cacheDir, "__action:recent_files")
        return
      }
    }
    OpenWithImport.import(this, intent)
  }

  /**
   * Test-only entry point for the open-with pipeline: runs the same copy the
   * intent handler does and returns the queued path (null when the file was
   * rejected). Instrumented tests drive it with a real `content://` provider.
   */
  @VisibleForTesting
  internal fun importIncomingUriForTest(uri: Uri, intentType: String? = null): String? {
    val before = OpenWithImport.pendingOpenPaths(cacheDir)
    OpenWithImport.copyIncoming(this, uri, intentType)
    val after = OpenWithImport.pendingOpenPaths(cacheDir)
    return after.lastOrNull { it !in before }
  }

  /**
   * Copies the OCR language models that ship as APK assets into the app's
   * private files directory. The bundled Tesseract executable reads them from
   * the filesystem (TESSDATA_PREFIX), so they cannot stay inside the APK.
   * The copy runs once per app version and is skipped afterwards.
   */
  private fun installTessdata() {
    val target = File(filesDir, "tessdata")
    val marker = File(target, ".installed")
    val stamp = "${packageInfoVersion()}-${assetStamp()}"
    if (marker.isFile && marker.readText().trim() == stamp) return
    if (!target.isDirectory && !target.mkdirs()) return
    val entries = try {
      assets.list("tessdata")
    } catch (error: Exception) {
      null
    } ?: return
    for (name in entries) {
      val assetPath = "tessdata/$name"
      val destination = File(target, name)
      if (hasChildren(assetPath)) {
        copyDirectory(assetPath, destination)
      } else {
        copyAsset(assetPath, destination)
      }
    }
    marker.writeText(stamp)
  }

  private fun assetStamp(): String = try {
    val entries = assets.list("tessdata") ?: emptyArray()
    entries.size.toString()
  } catch (error: Exception) {
    "0"
  }

  private fun packageInfoVersion(): String = try {
    packageManager.getPackageInfo(packageName, 0).versionName ?: "0"
  } catch (error: Exception) {
    "0"
  }

  private fun hasChildren(assetPath: String): Boolean = try {
    val children = assets.list(assetPath)
    children != null && children.isNotEmpty()
  } catch (error: Exception) {
    false
  }

  private fun copyDirectory(assetPath: String, destination: File) {
    if (!destination.isDirectory && !destination.mkdirs()) return
    val children = try {
      assets.list(assetPath)
    } catch (error: Exception) {
      null
    } ?: return
    for (name in children) {
      val childAsset = "$assetPath/$name"
      val childDestination = File(destination, name)
      if (hasChildren(childAsset)) {
        copyDirectory(childAsset, childDestination)
      } else {
        copyAsset(childAsset, childDestination)
      }
    }
  }

  private fun copyAsset(assetPath: String, destination: File) {
    try {
      assets.open(assetPath).use { input ->
        destination.outputStream().use { output -> input.copyTo(output) }
      }
    } catch (error: Exception) {
      // A missing optional model must not stop the app from starting.
    }
  }

  companion object {
    /** Sent by OpenWithActivity after it queued the documents: bring the app forward, import nothing. */
    const val ACTION_OPEN_WITH_WAKE = "io.github.ozanstn1.pdfswissarmyknife.OPEN_WITH_WAKE"
    private const val ACTION_SHORTCUT_SCAN = "io.github.ozanstn1.pdfswissarmyknife.SCAN"
    private const val ACTION_SHORTCUT_NEW_DOCUMENT = "io.github.ozanstn1.pdfswissarmyknife.NEW_DOCUMENT"
    private const val ACTION_SHORTCUT_RECENT_FILES = "io.github.ozanstn1.pdfswissarmyknife.RECENT_FILES"
  }
}
