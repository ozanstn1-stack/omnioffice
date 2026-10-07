package io.github.ozanstn1.pdfswissarmyknife

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.security.GeneralSecurityException
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * JVM tests for the AES-GCM blob format. They use a software key; the
 * Keystore-backed key generation is covered by device/emulator runs only.
 */
class SecretCipherTest {
  private fun newKey(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

  @Test
  fun `round trip returns the plaintext`() {
    val key = newKey()
    val plaintext = "sk-test-0123456789 çğış".toByteArray(Charsets.UTF_8)
    val blob = SecretCipher.encrypt(key, plaintext)
    assertArrayEquals(plaintext, SecretCipher.decrypt(key, blob))
  }

  @Test
  fun `blob layout is iv then ciphertext then tag`() {
    val key = newKey()
    val blob = SecretCipher.encrypt(key, ByteArray(10) { it.toByte() })
    assertEquals(SecretCipher.IV_BYTES + 10 + SecretCipher.TAG_BITS / 8, blob.size)
  }

  @Test
  fun `every encryption uses a fresh iv`() {
    val key = newKey()
    val first = SecretCipher.encrypt(key, "same".toByteArray())
    val second = SecretCipher.encrypt(key, "same".toByteArray())
    val firstIv = first.copyOfRange(0, SecretCipher.IV_BYTES)
    val secondIv = second.copyOfRange(0, SecretCipher.IV_BYTES)
    assertFalse(firstIv.contentEquals(secondIv))
  }

  @Test(expected = GeneralSecurityException::class)
  fun `a modified blob is rejected`() {
    val key = newKey()
    val blob = SecretCipher.encrypt(key, "secret".toByteArray())
    blob[blob.size - 1] = (blob[blob.size - 1].toInt() xor 1).toByte()
    SecretCipher.decrypt(key, blob)
  }

  @Test(expected = GeneralSecurityException::class)
  fun `a different key is rejected`() {
    val blob = SecretCipher.encrypt(newKey(), "secret".toByteArray())
    SecretCipher.decrypt(newKey(), blob)
  }

  @Test(expected = IllegalArgumentException::class)
  fun `a truncated blob is rejected`() {
    SecretCipher.decrypt(newKey(), ByteArray(SecretCipher.MIN_BLOB_BYTES - 1))
  }
}
