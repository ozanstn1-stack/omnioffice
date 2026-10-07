package io.github.ozanstn1.pdfswissarmyknife

import android.app.Activity
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.GeneralSecurityException

@InvokeArg
class SecretArgs {
  /** Standard Base64 (RFC 4648, padded, no line breaks). */
  lateinit var data: String
}

/**
 * Tauri mobile plugin behind `src-tauri/src/android_keystore.rs`.
 *
 * Both commands take and return standard padded Base64 without line breaks
 * (`Base64.NO_WRAP`), the same dialect as the Rust `base64` STANDARD engine.
 * `encrypt` takes the plaintext bytes and returns `iv || ciphertext+tag`;
 * `decrypt` takes that blob and returns the plaintext bytes. Secrets are never
 * logged and never put into an error message.
 *
 * A `decrypt` that cannot succeed because the key is gone, was replaced or
 * cannot be used rejects with the code `key_unavailable`.
 */
@TauriPlugin
class SecretsPlugin(activity: Activity) : Plugin(activity) {
  @Command
  fun encrypt(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SecretArgs::class.java)
      val plaintext = Base64.decode(args.data, Base64.NO_WRAP)
      val blob = SecretCipher.encrypt(KeystoreKey.getOrCreate(), plaintext)
      invoke.resolve(JSObject().put("data", Base64.encodeToString(blob, Base64.NO_WRAP)))
    } catch (error: Exception) {
      invoke.reject("keystore encryption failed", "encrypt_failed")
    }
  }

  @Command
  fun decrypt(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SecretArgs::class.java)
      val blob = Base64.decode(args.data, Base64.NO_WRAP)
      val key = KeystoreKey.find()
      if (key == null) {
        invoke.reject("the keystore key does not exist", "key_unavailable")
        return
      }
      val plaintext = SecretCipher.decrypt(key, blob)
      invoke.resolve(JSObject().put("data", Base64.encodeToString(plaintext, Base64.NO_WRAP)))
    } catch (error: GeneralSecurityException) {
      // Wrong/regenerated key (AEADBadTagException), invalidated or
      // unrecoverable key: the stored value can no longer be read.
      invoke.reject("the stored secret cannot be decrypted", "key_unavailable")
    } catch (error: Exception) {
      invoke.reject("keystore decryption failed", "decrypt_failed")
    }
  }
}
