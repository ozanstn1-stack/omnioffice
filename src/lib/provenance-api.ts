/**
 * Provenance manifests and the source chain (C8, D7).
 *
 * `pdfcore::provenance` writes and reads the C8 record embedded in PDFs;
 * `officecore::docx_provenance` does the same inside DOCX packages. The
 * `provenance_read_chain` command sniffs the container and returns one
 * derivation chain — sources, the transformation step and the output digest —
 * so the verification screen can present either format the same way.
 */
import type * as specta from "./generated/specta";
import { call } from "./typed-call";

export type ProvenanceSource = specta.ProvenanceSource_Serialize;
export type ProvenanceAi = specta.ProvenanceAi_Serialize;
export type ProvenanceManifest = specta.ProvenanceManifest_Serialize;
export type ProvenanceWriteRequest = specta.ProvenanceWriteRequest_Deserialize;
export type ProvenanceWriteResult = specta.ProvenanceWriteResult_Serialize;

/** One link in the derivation chain: a source, the step, or the output. */
export type ChainLink = specta.ChainLink;
/** The whole derivation chain of a PDF or DOCX. */
export type SourceChainReport = specta.SourceChainReport;

/** Reads the derivation chain and its integrity verdict. */
export const readSourceChain = (path: string) => call((c) => c.provenanceReadChain(path));

/** Writes a copy of a PDF or DOCX with the manifest embedded. */
export const writeProvenance = (request: ProvenanceWriteRequest) => call((c) => c.provenanceWrite(request));
