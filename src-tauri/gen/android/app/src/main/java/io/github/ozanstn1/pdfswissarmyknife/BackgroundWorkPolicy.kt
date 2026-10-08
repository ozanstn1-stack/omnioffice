package io.github.ozanstn1.pdfswissarmyknife

/**
 * Pure decisions behind the background-work foreground service, kept free of
 * Android classes so JVM unit tests can cover them.
 *
 * Android freezes or kills the process of an app that is no longer visible,
 * which ends a long OCR / compression / conversion mid-way. A foreground
 * service with a notification keeps the process alive while Rust jobs run.
 */
object BackgroundWorkPolicy {
  /** `Build.VERSION_CODES.TIRAMISU`: notifications became a runtime permission. */
  const val NOTIFICATION_PERMISSION_SDK = 33

  /** `Build.VERSION_CODES.Q`: the first release that knows foreground service types. */
  const val SERVICE_TYPE_SDK = 29

  /** `ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC`; the manifest declares `dataSync`. */
  const val SERVICE_TYPE_DATA_SYNC = 1

  /**
   * Whether to show the system notification prompt now. It is asked once, the
   * first time a job needs the service, and never again after a refusal: the
   * service works without the permission (the notification is only hidden from
   * the shade), so a denial must not turn into repeated prompts.
   */
  fun shouldRequestNotificationPermission(sdkInt: Int, granted: Boolean, alreadyAsked: Boolean): Boolean =
    sdkInt >= NOTIFICATION_PERMISSION_SDK && !granted && !alreadyAsked

  /** The type passed to `startForeground`; releases before Android 10 have none. */
  fun foregroundServiceType(sdkInt: Int): Int = if (sdkInt >= SERVICE_TYPE_SDK) SERVICE_TYPE_DATA_SYNC else 0
}
