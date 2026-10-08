package io.github.ozanstn1.pdfswissarmyknife

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** JVM tests for the decisions behind the background-work foreground service. */
class BackgroundWorkPolicyTest {
  @Test
  fun `the notification prompt is asked once on Android 13 and later`() {
    assertTrue(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 33, granted = false, alreadyAsked = false))
    assertTrue(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 36, granted = false, alreadyAsked = false))
  }

  @Test
  fun `a refusal is remembered and never asked again`() {
    assertFalse(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 34, granted = false, alreadyAsked = true))
  }

  @Test
  fun `nothing is asked when the permission is granted or does not exist yet`() {
    assertFalse(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 34, granted = true, alreadyAsked = false))
    // Before Android 13 notifications need no runtime permission.
    assertFalse(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 32, granted = false, alreadyAsked = false))
    assertFalse(BackgroundWorkPolicy.shouldRequestNotificationPermission(sdkInt = 24, granted = false, alreadyAsked = false))
  }

  @Test
  fun `the service type is only passed where Android knows service types`() {
    assertEquals(0, BackgroundWorkPolicy.foregroundServiceType(24))
    assertEquals(0, BackgroundWorkPolicy.foregroundServiceType(28))
    assertEquals(BackgroundWorkPolicy.SERVICE_TYPE_DATA_SYNC, BackgroundWorkPolicy.foregroundServiceType(29))
    assertEquals(BackgroundWorkPolicy.SERVICE_TYPE_DATA_SYNC, BackgroundWorkPolicy.foregroundServiceType(36))
    // ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC is 1 (a compile-time constant of the platform).
    assertEquals(1, BackgroundWorkPolicy.SERVICE_TYPE_DATA_SYNC)
  }

  @Test
  fun `the manifest declares the service and the permissions Android 14 requires`() {
    val manifest = listOf("src/main/AndroidManifest.xml", "app/src/main/AndroidManifest.xml")
      .map(::File).first { it.isFile }.readText()
    for (permission in listOf("FOREGROUND_SERVICE", "FOREGROUND_SERVICE_DATA_SYNC", "POST_NOTIFICATIONS")) {
      assertTrue("$permission is missing", manifest.contains("android.permission.$permission\""))
    }
    val service = Regex("<service[^>]*>", RegexOption.DOT_MATCHES_ALL).find(manifest)?.value.orEmpty()
    assertTrue(service.contains("android:name=\".BackgroundWorkService\""))
    assertTrue("Android 14 rejects a foreground service without its type", service.contains("android:foregroundServiceType=\"dataSync\""))
    assertTrue("the service is internal", service.contains("android:exported=\"false\""))
  }
}
