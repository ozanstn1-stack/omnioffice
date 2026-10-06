package io.github.ozanstn1.pdfswissarmyknife

import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Launch smoke test: boots the real app on the emulator - the Rust library,
 * the Tauri WebView and the embedded frontend - and waits until the Home
 * screen has rendered its tool grid.
 *
 * The other on-device suite exercises the intent pipeline without the UI; this
 * one catches what only shows up at startup: a native library that fails to
 * load, a crash in onCreate, or a frontend that never renders (blank WebView).
 */
@RunWith(AndroidJUnit4::class)
class AppLaunchSmokeTest {
  @Test
  fun appStartsAndRendersTheHomeToolGrid() {
    ActivityScenario.launch(MainActivity::class.java).use { scenario ->
      val deadline = SystemClock.uptimeMillis() + TimeUnit.SECONDS.toMillis(120)
      var cards = 0
      var lastState = "not checked"
      while (SystemClock.uptimeMillis() < deadline && cards == 0) {
        val answered = CountDownLatch(1)
        scenario.onActivity { activity ->
          val webView = findWebView(activity.window.decorView)
          if (webView == null) {
            lastState = "no WebView yet"
            answered.countDown()
          } else {
            webView.evaluateJavascript("document.querySelectorAll('.tool-card').length") { value ->
              cards = value?.trim('"')?.toIntOrNull() ?: 0
              lastState = "url=${webView.url} cards=$value"
              answered.countDown()
            }
          }
        }
        answered.await(15, TimeUnit.SECONDS)
        if (cards == 0) Thread.sleep(1000)
      }
      assertTrue("the Home tool grid did not render within 120 s ($lastState)", cards > 0)
    }
  }

  private fun findWebView(view: View): WebView? {
    if (view is WebView) return view
    if (view is ViewGroup) {
      for (index in 0 until view.childCount) {
        findWebView(view.getChildAt(index))?.let { return it }
      }
    }
    return null
  }
}
