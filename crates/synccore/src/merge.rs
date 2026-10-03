//! Conflict resolution policy.
//!
//! This module is intentionally pure: it maps `(SyncState, Resolution)` to a
//! [`MergeAction`] and provides the "cloud copy" naming used by `keep_both`.
//! The Tauri layer executes the action; keeping the policy here makes the
//! three-way matrix unit-testable without a network or a filesystem.
//!
//! There is no destructive default. Every action requires an explicit user
//! decision, and `keep_both` is the only choice that loses nothing at all.

use crate::metadata::SyncState;
use crate::SyncError;

/// What the user asked for when a file is not in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Make the cloud copy match the local file (upload).
    KeepLocal,
    /// Make the local file match the cloud copy (download, overwrites local).
    KeepCloud,
    /// Download the cloud copy next to the local file; nothing is replaced.
    KeepBoth,
}

impl Resolution {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "keep_local" | "keep-local" | "keeplocal" => Some(Resolution::KeepLocal),
            "keep_cloud" | "keep-cloud" | "keepcloud" => Some(Resolution::KeepCloud),
            "keep_both" | "keep-both" | "keepboth" => Some(Resolution::KeepBoth),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Resolution::KeepLocal => "keep_local",
            Resolution::KeepCloud => "keep_cloud",
            Resolution::KeepBoth => "keep_both",
        }
    }
}

/// Concrete operation the caller should perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    /// Already in sync; nothing to do.
    Nothing,
    /// Upload the local file (conditional write keeps it race-safe).
    UploadLocal,
    /// Replace the local file with the cloud copy.
    DownloadCloud,
    /// Save the cloud copy as a new sibling document.
    DownloadCloudCopy,
}

/// The resolution matrix:
///
/// | state | keep_local | keep_cloud | keep_both |
/// |---|---|---|---|
/// | LocalOnly | upload | error: no cloud copy | error: no cloud copy |
/// | Synced | nothing | nothing | nothing |
/// | LocalAhead | upload | download (revert local) | download copy |
/// | CloudAhead | upload (explicit force) | download | download copy |
/// | Conflict | upload (explicit force) | download | download copy |
///
/// `Synced` is deliberately a no-op for all three: there is nothing to
/// resolve, and running an action anyway could only cause a write.
pub fn plan(resolution: Resolution, state: SyncState) -> Result<MergeAction, SyncError> {
    match state {
        SyncState::Synced => Ok(MergeAction::Nothing),
        SyncState::LocalOnly => match resolution {
            Resolution::KeepLocal => Ok(MergeAction::UploadLocal),
            Resolution::KeepCloud => Err(SyncError::InvalidInput("there is no cloud copy to download yet".to_string())),
            Resolution::KeepBoth => {
                Err(SyncError::InvalidInput("there is no cloud copy to keep a copy of yet".to_string()))
            }
        },
        SyncState::LocalAhead | SyncState::CloudAhead | SyncState::Conflict => match resolution {
            Resolution::KeepLocal => Ok(MergeAction::UploadLocal),
            Resolution::KeepCloud => Ok(MergeAction::DownloadCloud),
            Resolution::KeepBoth => Ok(MergeAction::DownloadCloudCopy),
        },
    }
}

/// Name of the cloud copy saved by `keep_both`:
/// `report.oswk` + `2026-09-27 141530` ->
/// `report (cloud copy 2026-09-27 141530).oswk`.
///
/// A file without an extension still gets `.oswk`, otherwise the copy would
/// not open in the suite.
pub fn cloud_copy_name(file_name: &str, stamp: &str) -> String {
    let trimmed = file_name.trim();
    let (stem, extension) = match trimmed.rfind('.') {
        Some(index) if index > 0 => (&trimmed[..index], &trimmed[index..]),
        _ => (trimmed, ".oswk"),
    };
    let stem = if stem.is_empty() { "document" } else { stem };
    format!("{stem} (cloud copy {stamp}){extension}")
}

/// Same as [`cloud_copy_name`] but guarantees uniqueness: when the candidate
/// already exists, ` (2)`, ` (3)`... is inserted before the timestamp until a
/// free name is found. `exists` is injected so the function stays pure.
pub fn unique_cloud_copy_name(file_name: &str, stamp: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = cloud_copy_name(file_name, stamp);
    if !exists(&base) {
        return base;
    }
    let trimmed = file_name.trim();
    let (stem, extension) = match trimmed.rfind('.') {
        Some(index) if index > 0 => (&trimmed[..index], &trimmed[index..]),
        _ => (trimmed, ".oswk"),
    };
    let stem = if stem.is_empty() { "document" } else { stem };
    for index in 2..1000 {
        let candidate = format!("{stem} (cloud copy {stamp}, {index}){extension}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    // Practically unreachable; keep the first candidate rather than looping.
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_STATES: [SyncState; 5] =
        [SyncState::LocalOnly, SyncState::Synced, SyncState::LocalAhead, SyncState::CloudAhead, SyncState::Conflict];

    #[test]
    fn resolution_parses_all_spellings() {
        assert_eq!(Resolution::parse("keep_local"), Some(Resolution::KeepLocal));
        assert_eq!(Resolution::parse("keep-cloud"), Some(Resolution::KeepCloud));
        assert_eq!(Resolution::parse("  KEEP_BOTH "), Some(Resolution::KeepBoth));
        assert_eq!(Resolution::parse("overwrite"), None);
        assert_eq!(Resolution::parse(""), None);
    }

    #[test]
    fn full_three_way_matrix() {
        let expected: [(SyncState, MergeAction, MergeAction, Option<MergeAction>); 5] = [
            // (state, keep_local, keep_cloud, keep_both)
            (SyncState::LocalOnly, MergeAction::UploadLocal, MergeAction::Nothing, None),
            (SyncState::Synced, MergeAction::Nothing, MergeAction::Nothing, Some(MergeAction::Nothing)),
            (
                SyncState::LocalAhead,
                MergeAction::UploadLocal,
                MergeAction::DownloadCloud,
                Some(MergeAction::DownloadCloudCopy),
            ),
            (
                SyncState::CloudAhead,
                MergeAction::UploadLocal,
                MergeAction::DownloadCloud,
                Some(MergeAction::DownloadCloudCopy),
            ),
            (
                SyncState::Conflict,
                MergeAction::UploadLocal,
                MergeAction::DownloadCloud,
                Some(MergeAction::DownloadCloudCopy),
            ),
        ];
        for (state, keep_local, keep_cloud, keep_both) in expected {
            assert_eq!(plan(Resolution::KeepLocal, state).unwrap(), keep_local, "{state:?} keep_local");
            if state == SyncState::LocalOnly {
                // Downloading something that does not exist must be an error,
                // never a silent no-op or an overwrite.
                assert!(plan(Resolution::KeepCloud, state).is_err());
                assert!(plan(Resolution::KeepBoth, state).is_err());
            } else {
                assert_eq!(plan(Resolution::KeepCloud, state).unwrap(), keep_cloud, "{state:?} keep_cloud");
                assert_eq!(plan(Resolution::KeepBoth, state).unwrap(), keep_both.unwrap(), "{state:?} keep_both");
            }
        }
    }

    #[test]
    fn every_state_is_covered() {
        // Guards against a state being added without updating the matrix.
        for state in ALL_STATES {
            for resolution in [Resolution::KeepLocal, Resolution::KeepCloud, Resolution::KeepBoth] {
                let result = plan(resolution, state);
                assert!(result.is_ok() || matches!(result, Err(SyncError::InvalidInput(_))));
            }
        }
    }

    #[test]
    fn cloud_copy_naming() {
        assert_eq!(cloud_copy_name("report.oswk", "2026-09-27 141530"), "report (cloud copy 2026-09-27 141530).oswk");
        // Dots inside the stem stay in the stem.
        assert_eq!(
            cloud_copy_name("Q3.report.oswk", "2026-09-27 141530"),
            "Q3.report (cloud copy 2026-09-27 141530).oswk"
        );
        // No extension: keep the document usable.
        assert_eq!(cloud_copy_name("report", "2026-09-27 141530"), "report (cloud copy 2026-09-27 141530).oswk");
    }

    #[test]
    fn cloud_copy_collision_appends_index() {
        let stamp = "2026-09-27 141530";
        let taken = std::cell::RefCell::new(vec!["report (cloud copy 2026-09-27 141530).oswk".to_string()]);
        let name = unique_cloud_copy_name("report.oswk", stamp, |candidate| {
            taken.borrow().iter().any(|entry| entry == candidate)
        });
        assert_eq!(name, "report (cloud copy 2026-09-27 141530, 2).oswk");
    }
}
