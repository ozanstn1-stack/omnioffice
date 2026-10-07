package io.github.ozanstn1.pdfswissarmyknife

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * AES-256-GCM helpers for the secrets bridge ([SecretsPlugin]).
 *
 * The cipher functions take the [SecretKey] as a parameter so the JVM unit
 * tests can run them with a software key; only [KeystoreKey] touches the
 * Android Keystore. The wire format is `iv(12) || ciphertext || tag(16)`.
 */
object SecretCipher {
  private const val TRANSFORMATION = "AES/GCM/NoPadding"
  const val IV_BYTES = 12
  const val TAG_BITS = 128

  /** Smallest valid blob: an IV and a tag around an empty plaintext. */
  const val MIN_BLOB_BYTES = IV_BYTES + TAG_BITS / 8

  /**
   * Encrypts [plaintext]. The IV is chosen by the cipher (the Keystore forces
   * a fresh random one per call) and prepended to the output.
   */
  fun encrypt(key: SecretKey, plaintext: ByteArray): ByteArray {
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.ENCRYPT_MODE, key)
    val iv = cipher.iv
    check(iv != null && iv.size == IV_BYTES) { "unexpected GCM IV length" }
    return iv + cipher.doFinal(plaintext)
  }

  /**
   * Decrypts a blob produced by [encrypt]. A wrong key or a modified blob
   * throws a [java.security.GeneralSecurityException] (AEADBadTagException).
   */
  fun decrypt(key: SecretKey, blob: ByteArray): ByteArray {
    require(blob.size >= MIN_BLOB_BYTES) { "ciphertext is too short" }
    val cipher = Cipher.getInstance(TRANSFORMATION)
    cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, blob, 0, IV_BYTES))
    return cipher.doFinal(blob, IV_BYTES, blob.size - IV_BYTES)
  }
}

/**
 * The app's single Keystore key. It is generated inside AndroidKeyStore, so
 * the key material is not exportable. No user authentication is required (the
 * secrets are also read in the background, e.g. by sync). The key never
 * survives a backup restore or a re-install, which is why the Rust side treats
 * a decryption failure as "enter the secret again".
 */
object KeystoreKey {
  const val ALIAS = "omnioffice-secrets"
  private const val PROVIDER = "AndroidKeyStore"

  private fun keyStore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

  /** The existing key, or null when none was generated yet (or it was wiped). */
  fun find(): SecretKey? = keyStore().getKey(ALIAS, null) as? SecretKey

  /** The existing key, generating it on first use. */
  @Synchronized
  fun getOrCreate(): SecretKey {
    find()?.let { return it }
    val spec = KeyGenParameterSpec.Builder(
      ALIAS,
      KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
    )
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setKeySize(256)
      .setRandomizedEncryptionRequired(true)
      .build()
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
    generator.init(spec)
    return generator.generateKey()
  }
}
