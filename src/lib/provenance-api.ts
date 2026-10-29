/**
 * Provenance manifests (C8): typed calls for writing and reading the record
 * that says what a generated PDF was made from. The manifest itself is
 * `pdfcore::provenance`; the commands are `src-tauri/src/provenance.rs`.
 */
import type * as specta from "./generated/specta";
import { call } from "./typed-call";

export type ProvenanceSource = specta.ProvenanceSource_Serialize;
export type ProvenanceAi = specta.ProvenanceAi_Serialize;
export type ProvenanceManifest = specta.ProvenanceManifest_Serialize;
export type ProvenanceReport = specta.ProvenanceReport_Serialize;
export type ProvenanceWriteRequest = specta.ProvenanceWriteRequest_Deserialize;
export type ProvenanceWriteResult = specta.ProvenanceWriteResult_Serialize;

/** Writes a copy of `input` with the manifest embedded. */
export const writeProvenance = (request: ProvenanceWriteRequest) => call((c) => c.pdfProvenanceWrite(request));

/** Reads and verifies the manifest of a document, if it carries one. */
export const readProvenance = (path: string) => call((c) => c.pdfProvenanceRead(path));
