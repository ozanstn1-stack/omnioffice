/**
 * Evidence bundles (C3): the typed calls for building a bundle, verifying one
 * on reopen and matching original files to its exhibits. The bundle itself is
 * `pdfcore::evidence`; the commands are `src-tauri/src/evidence.rs`.
 */
import type * as specta from "./generated/specta";
import { call } from "./typed-call";
import type { OutputSpec } from "./types";

export type EvidenceDetails = specta.EvidenceDetails_Deserialize;
export type EvidenceEntry = specta.EvidenceEntry;
export type EvidenceManifest = specta.EvidenceManifest_Serialize;
export type EvidenceVerification = specta.EvidenceVerification_Serialize;
export type EvidenceOriginalMatch = specta.EvidenceOriginalMatch_Serialize;
export type EvidenceExhibit = specta.EvidenceExhibit_Deserialize;

export const buildEvidenceBundle = (exhibits: EvidenceExhibit[], details: EvidenceDetails, output: OutputSpec) =>
  call((c) => c.evidenceBundleBuild({ exhibits, details, output: { path: output.path, overwrite: output.overwrite } }));

export const verifyEvidenceBundle = (path: string) => call((c) => c.evidenceBundleVerify(path));

export const matchEvidenceOriginals = (bundle: string, originals: string[]) =>
  call((c) => c.evidenceMatchOriginals(bundle, originals));
