//! Android Keystore bridge for the secret store (Android builds only).
//!
//! Bridge choice: a Tauri mobile plugin that lives in the app module
//! (`SecretsPlugin.kt`, registered here with `register_android_plugin` and
//! called with `run_mobile_plugin`) instead of JNI from Rust. Tauri already
//! owns the activity lookup, class loading, thread hand-off and error
//! transport for plugins, and the Tauri consumer ProGuard rules keep
//! `@TauriPlugin` classes in release builds; plain JNI would need a second
//! hand-rolled `ndk-context` + `jni` layer (new crates, manual local-reference
//! and exception handling) for no gain.
//!
//! The key is an AES-256-GCM key generated inside AndroidKeyStore (alias
//! `omnioffice-secrets`, not exportable, randomized IVs, no user-auth
//! requirement so background sync can read secrets). The stored value is
//! `keystore:<base64(iv || ciphertext+tag)>`.
//!
//! Base64: the Rust side uses the standard alphabet with padding; the Kotlin
//! side uses `android.util.Base64` with `NO_WRAP`, which is the same dialect.
//! Plaintext and ciphertext both cross the bridge as Base64 strings so no
//! secret byte depends on a charset. The calls block until Kotlin answers.

use crate::secret::{base64_decode, base64_encode, CipherError, SecretCipher, KEYSTORE_PREFIX};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::plugin::mobile::PluginInvokeError;
use tauri::plugin::{PluginHandle, TauriPlugin};
use tauri::Wry;

const PLUGIN_NAME: &str = "omnioffice-secrets";
const KOTLIN_PACKAGE: &str = "io.github.ozanstn1.pdfswissarmyknife";
const KOTLIN_CLASS: &str = "SecretsPlugin";
/// Error code `SecretsPlugin` rejects with when the Keystore key is gone.
const KEY_UNAVAILABLE: &str = "key_unavailable";

static HANDLE: OnceLock<PluginHandle<Wry>> = OnceLock::new();

#[derive(Serialize)]
struct Payload<'a> {
    data: &'a str,
}

#[derive(Deserialize)]
struct Reply {
    data: String,
}

/// The plugin to register on the app builder. A failed registration is logged
/// and swallowed: the app must still start, and the secret store then keeps
/// working through its plain fallback instead of losing the user's values.
pub fn init() -> TauriPlugin<Wry> {
    tauri::plugin::Builder::<Wry>::new(PLUGIN_NAME)
        .setup(|_app, api| {
            match api.register_android_plugin(KOTLIN_PACKAGE, KOTLIN_CLASS) {
                Ok(handle) => {
                    let _ = HANDLE.set(handle);
                }
                Err(error) => eprintln!("could not register the Android Keystore plugin: {error}"),
            }
            Ok(())
        })
        .build()
}

fn call(command: &str, bytes: &[u8]) -> Result<Vec<u8>, CipherError> {
    let handle = HANDLE.get().ok_or_else(|| CipherError::Failed("the Android Keystore bridge is not ready".into()))?;
    let encoded = base64_encode(bytes);
    match handle.run_mobile_plugin::<Reply>(command, Payload { data: &encoded }) {
        Ok(reply) => base64_decode(&reply.data)
            .ok_or_else(|| CipherError::Failed("the Android Keystore returned invalid data".into())),
        Err(PluginInvokeError::InvokeRejected(response)) if response.code.as_deref() == Some(KEY_UNAVAILABLE) => {
            Err(CipherError::KeyLost)
        }
        Err(error) => Err(CipherError::Failed(format!("Android Keystore {command} failed: {error}"))),
    }
}

/// [`SecretCipher`] backed by the Keystore plugin.
pub struct Keystore;

impl SecretCipher for Keystore {
    fn prefix(&self) -> &'static str {
        KEYSTORE_PREFIX
    }
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        call("encrypt", plaintext)
    }
    fn unprotect(&self, blob: &[u8]) -> Result<Vec<u8>, CipherError> {
        call("decrypt", blob)
    }
    fn migrates_plain(&self) -> bool {
        true
    }
}
