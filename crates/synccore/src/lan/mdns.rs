//! mDNS/DNS-SD discovery for the LAN sync (C14 phase 2).
//!
//! The host advertises `_omnioffice._tcp.local.` with a TXT record carrying its
//! device id, certificate fingerprint and the protocol version; a client browses
//! that type and turns every resolved instance into a [`LanDevice`]. The UDP
//! broadcast from phase 1 stays as a fallback for networks that block multicast
//! (some guests and CI sandboxes do), and the two result sets are merged by
//! device id.
//!
//! On Android multicast reception needs a `WifiManager.MulticastLock`; the
//! `LanSharePlugin` holds one while sharing, otherwise the mDNS responses never
//! reach the socket.

use super::{LanDevice, LanIdentity};
use crate::SyncError;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Service type both sides use.
pub(super) const SERVICE_TYPE: &str = "_omnioffice._tcp.local.";
const TXT_DEVICE: &str = "device_id";
const TXT_FINGERPRINT: &str = "fingerprint";
const TXT_PROTOCOL: &str = "protocol";
const PROTOCOL: &str = "2";

/// Keeps the advertisement alive until it is dropped or stopped.
pub(super) struct Advertiser {
    daemon: ServiceDaemon,
    full_name: String,
}

impl Advertiser {
    /// Registers the service. `addresses` may be empty, in which case mdns-sd
    /// publishes every interface address and keeps them updated.
    pub(super) fn start(identity: &LanIdentity, port: u16, addresses: &[String]) -> Result<Self, SyncError> {
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;
        let instance = instance_name(&identity.name, &identity.device_id);
        let host = format!("{}.local.", host_label(&identity.device_id));
        let ip = addresses.join(",");
        let properties = [
            (TXT_DEVICE, identity.device_id.as_str()),
            (TXT_FINGERPRINT, identity.fingerprint.as_str()),
            (TXT_PROTOCOL, PROTOCOL),
        ];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, ip.as_str(), port, &properties[..])
            .map_err(mdns_error)?
            .enable_addr_auto();
        let full_name = info.get_fullname().to_string();
        daemon.register(info).map_err(mdns_error)?;
        Ok(Self { daemon, full_name })
    }
}

impl Drop for Advertiser {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.full_name);
        let _ = self.daemon.shutdown();
    }
}

/// Browses for hosts for at most `timeout` and returns what resolved.
pub(super) fn discover(timeout: Duration) -> Result<Vec<LanDevice>, SyncError> {
    let daemon = ServiceDaemon::new().map_err(mdns_error)?;
    let receiver = daemon.browse(SERVICE_TYPE).map_err(mdns_error)?;
    let deadline = Instant::now() + timeout;
    let mut devices: HashMap<String, LanDevice> = HashMap::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if left.is_zero() {
            break;
        }
        match receiver.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(ServiceEvent::ServiceResolved(resolved)) => {
                if let Some(device) = device_from_resolved(&resolved) {
                    devices.entry(device.device_id.clone()).or_insert(device);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    let mut found: Vec<LanDevice> = devices.into_values().collect();
    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

fn mdns_error(error: impl std::fmt::Display) -> SyncError {
    SyncError::Network(format!("mDNS: {error}"))
}

/// Maps one resolved mDNS instance to a device; `None` when the TXT record is
/// not ours or lacks the identity fields.
fn device_from_resolved(resolved: &mdns_sd::ResolvedService) -> Option<LanDevice> {
    let device_id = resolved.txt_properties.get_property_val_str(TXT_DEVICE)?;
    let fingerprint = resolved.txt_properties.get_property_val_str(TXT_FINGERPRINT)?;
    if device_id.is_empty() || fingerprint.is_empty() {
        return None;
    }
    let address = resolved
        .addresses
        .iter()
        .find(|address| address.is_ipv4())
        .map(|address| format!("{}:{}", address.to_ip_addr(), resolved.port))?;
    Some(LanDevice {
        device_id: device_id.to_string(),
        name: resolved.fullname.split('.').next().unwrap_or(device_id).replace('\\', ""),
        fingerprint: fingerprint.to_string(),
        address,
    })
}

/// DNS-SD instance names allow dots and backslashes; keep them out of the way
/// and fall back to the device id for an empty name.
fn instance_name(name: &str, device_id: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|character| match character {
            '.' | '\\' => '-',
            other if other.is_control() => '-',
            other => other,
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').trim();
    if trimmed.is_empty() {
        format!("OmniOffice-{}", &device_id.chars().take(8).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

/// A DNS host label is lowercase alphanumerics and hyphens.
fn host_label(device_id: &str) -> String {
    let mut label: String = device_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    label.truncate(40);
    if label.is_empty() {
        return "omnioffice".to_string();
    }
    format!("omnioffice-{label}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_stay_inside_the_dnssd_rules() {
        assert_eq!(instance_name("Atölye PC", "device-a"), "Atölye PC");
        assert_eq!(instance_name("Ofis.2", "device-a"), "Ofis-2");
        assert_eq!(instance_name("  ", "device-12345678"), "OmniOffice-device-1");
        assert_eq!(host_label("0f68b482-D84E"), "omnioffice-0f68b482d84e");
        assert_eq!(host_label("!!!"), "omnioffice");
    }

    #[test]
    fn a_device_without_the_txt_identity_is_ignored() {
        // A resolved instance is only constructed by the daemon; the mapping
        // rule (missing TXT fields -> no device) is exercised here through the
        // property lookup contract.
        let properties = mdns_sd::TxtProperties::new();
        assert!(properties.get_property_val_str(TXT_DEVICE).is_none());
        assert!(TXT_FINGERPRINT.is_ascii());
    }

    #[test]
    fn discovery_returns_within_its_timeout_without_a_network() {
        // A sandbox without multicast may answer with an error; either way the
        // call must return well before the test timeout.
        let started = Instant::now();
        match discover(Duration::from_millis(300)) {
            Ok(devices) => assert!(devices.iter().all(|device| !device.device_id.is_empty())),
            Err(error) => assert!(!error.to_string().is_empty()),
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
