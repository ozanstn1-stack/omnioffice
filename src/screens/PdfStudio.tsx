/**
 * PDF Studio: the V3.0 document-health tools - sanitizer, flattening,
 * PDF/A validation/conversion and real digital signatures. Every result shown
 * here comes from a real check in pdfcore; nothing is reported as "compliant"
 * or "signed" without validation, and signature trust is always reported as
 * unknown (this build has no system trust store and no revocation check).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  BadgeCheck,
  FileCheck2,
  Layers,
  Move,
  PenLine,
  RefreshCw,
  RotateCcw,
  RotateCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Wrench,
} from "lucide-react";
import { useT } from "../lib/i18n";
import { errorMessage, useToasts } from "../lib/store";
import { invokeTracked } from "../lib/api";
import { DropZone, FileList } from "../components/files";
import { Badge, Card, Field, Toggle } from "../components/ui";
import { PageCanvas } from "../components/pages";
import {
  pdfArchiveValidationData,
  pdfEditObjects,
  pdfFillForm,
  pdfInfo,
  pdfListFormFields,
  pdfListObjects,
  pdfListSigningCertificates,
  pdfSign,
  pdfValidateForm,
  pdfVerifySignatures,
  suggestOutput,
  toAppError,
  type FieldIssue,
  type FieldValue,
  type FormFieldInfo,
  type LtvReport,
  type ObjectEdit,
  type PageObjectInfo,
  type SignatureInfo,
  type SignatureReport,
  type SigningCertificateSummary,
  type SignResult,
} from "../lib/api";
import type { PdfInfo } from "../lib/types";
import { isAndroid, pickAndroidSaveTarget, publishOutputs, type AndroidTarget } from "../lib/mobile";

type StudioTab = "sanitize" | "repair" | "flatten" | "pdfa" | "signatures" | "objects";

/**
 * The page geometry the PDF Studio overlay needs. Mirroring `render::PageGeometry`
 * (which serializes with `width_pt`), this is normalized once here.
 */
export interface StudioPageGeometry {
  width: number;
  height: number;
  rotation: number;
}

/** Displayed page size (rotation 90/270 swaps width and height). */
export function displayedPageSize(geometry: StudioPageGeometry): { width: number; height: number } {
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  return rotation === 90 || rotation === 270
    ? { width: geometry.height, height: geometry.width }
    : { width: geometry.width, height: geometry.height };
}

/**
 * Maps a page-space rectangle (bottom-left origin, y up) to a normalized
 * rectangle over the rendered page image (top-left origin, y down). The page
 * rotation is applied exactly like `docutil::Matrix::display_to_page`, only
 * inverted - getting this wrong selects the wrong object.
 */
export function pageRectToDisplayRect(
  rect: [number, number, number, number],
  geometry: StudioPageGeometry,
): { x: number; y: number; w: number; h: number } {
  const x0 = Math.min(rect[0], rect[2]);
  const y0 = Math.min(rect[1], rect[3]);
  const x1 = Math.max(rect[0], rect[2]);
  const y1 = Math.max(rect[1], rect[3]);
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  const map = (x: number, y: number): [number, number] => {
    switch (rotation) {
      case 90:
        return [y, geometry.width - x];
      case 180:
        return [geometry.width - x, geometry.height - y];
      case 270:
        return [geometry.height - y, x];
      default:
        return [x, y];
    }
  };
  const corners = [map(x0, y0), map(x1, y0), map(x0, y1), map(x1, y1)];
  const minX = Math.min(...corners.map((corner) => corner[0]));
  const maxX = Math.max(...corners.map((corner) => corner[0]));
  const minY = Math.min(...corners.map((corner) => corner[1]));
  const maxY = Math.max(...corners.map((corner) => corner[1]));
  const displayed = displayedPageSize(geometry);
  return {
    x: minX / displayed.width,
    y: 1 - maxY / displayed.height,
    w: (maxX - minX) / displayed.width,
    h: (maxY - minY) / displayed.height,
  };
}

/** Inverse of `pageRectToDisplayRect`, used when a resize handle is released. */
export function displayRectToPageRect(
  rect: { x: number; y: number; w: number; h: number },
  geometry: StudioPageGeometry,
): [number, number, number, number] {
  const rotation = ((geometry.rotation % 360) + 360) % 360;
  const displayed = displayedPageSize(geometry);
  const left = rect.x * displayed.width;
  const right = (rect.x + rect.w) * displayed.width;
  const bottom = (1 - rect.y - rect.h) * displayed.height;
  const top = (1 - rect.y) * displayed.height;
  const map = (x: number, y: number): [number, number] => {
    switch (rotation) {
      case 90:
        return [geometry.width - y, x];
      case 180:
        return [geometry.width - x, geometry.height - y];
      case 270:
        return [y, geometry.height - x];
      default:
        return [x, y];
    }
  };
  const corners = [map(left, bottom), map(right, bottom), map(left, top), map(right, top)];
  const minX = Math.min(...corners.map((corner) => corner[0]));
  const maxX = Math.max(...corners.map((corner) => corner[0]));
  const minY = Math.min(...corners.map((corner) => corner[1]));
  const maxY = Math.max(...corners.map((corner) => corner[1]));
  return [minX, minY, maxX, maxY];
}

/** Deltas measured on the displayed (rotated) page back into page space. */
export function displayDeltaToPage(dx: number, dy: number, rotation: number): [number, number] {
  switch (((rotation % 360) + 360) % 360) {
    case 90:
      return [-dy, dx];
    case 180:
      return [-dx, -dy];
    case 270:
      return [dy, -dx];
    default:
      return [dx, dy];
  }
}

function objectKey(object: Pick<PageObjectInfo, "page" | "index">): string {
  return `${object.page}:${object.index}`;
}

/** `option_not_in_list` -> `optionNotInList`, matching the i18n key names. */
function issueI18nKey(code: string): string {
  return code.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase());
}

function isCheckedValue(value: string): boolean {
  const lower = value.trim().toLowerCase();
  return !["", "off", "false", "0", "no", "unchecked"].includes(lower);
}

interface ProgressEvent {
  jobId: string;
  stage: string;
  current: number;
  total: number;
  message?: string;
}

interface SanitizeFindingsBefore {
  javascriptEntries: number;
  embeddedFiles: number;
  actions: number;
  unsafeAnnotations: number;
  linkAnnotations: number;
  metadataPresent: boolean;
  openActionPresent: boolean;
}

interface SanitizeReport {
  javascriptRemoved: number;
  embeddedFilesRemoved: number;
  actionsRemoved: number;
  /** Count of removed metadata entries (Rust `u32`), not a boolean. */
  metadataRemoved: number;
  annotationsRemoved: number;
  linksRemoved: number;
  warnings: string[];
  findingsBefore?: SanitizeFindingsBefore;
}

interface FlattenReport {
  annotationsFlattened: number;
  fieldsFlattened: number;
  pagesTouched: number;
  warnings: string[];
}

/** Result of the qpdf-backed repair/linearize commands (`RepairReport` in Rust). */
interface RepairReport {
  output: string;
  pages: number;
  warnings: string[];
}

interface PdfaCheck {
  id: string;
  level: string;
  status: string;
  message: string;
}

interface PdfaReport {
  valid: boolean;
  level: string;
  checks: PdfaCheck[];
  failures: number;
  warnings: number;
  converted: boolean;
}

function fileBaseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function signatureTone(info: SignatureInfo): "ok" | "warn" | "danger" {
  if (!info.signatureValid || !info.digestMatches) return "danger";
  if (info.modifiedAfterSigning || !info.coversWholeDocument) return "warn";
  return "ok";
}

export function PdfStudio({ initialFiles, dragging }: { initialFiles?: string[]; dragging?: boolean }) {
  const t = useT();
  const [tab, setTab] = useState<StudioTab>("sanitize");
  const [files, setFiles] = useState<string[]>(initialFiles ?? []);
  const [level, setLevel] = useState("A-2b");
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [sanitizeReport, setSanitizeReport] = useState<SanitizeReport | null>(null);
  const [repairReport, setRepairReport] = useState<RepairReport | null>(null);
  const [flattenReport, setFlattenReport] = useState<FlattenReport | null>(null);
  const [pdfaReport, setPdfaReport] = useState<PdfaReport | null>(null);

  // Signature tab state.
  const [signReport, setSignReport] = useState<SignatureReport | null>(null);
  const [ltvReport, setLtvReport] = useState<LtvReport | null>(null);
  const [archiving, setArchiving] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [certificates, setCertificates] = useState<SigningCertificateSummary[]>([]);
  const [certIndex, setCertIndex] = useState<number | null>(null);
  const [pfxPath, setPfxPath] = useState<string | null>(null);
  const [pfxName, setPfxName] = useState("");
  const [pfxPassword, setPfxPassword] = useState("");
  const [page, setPage] = useState(1);
  const [reason, setReason] = useState("");
  const [location, setLocation] = useState("");
  const [signerName, setSignerName] = useState("");
  const [appearance, setAppearance] = useState(true);
  const [signing, setSigning] = useState(false);
  const [lastResult, setLastResult] = useState<SignResult | null>(null);

  // "Forms & objects" tab state.
  const [fields, setFields] = useState<FormFieldInfo[]>([]);
  const [objects, setObjects] = useState<PageObjectInfo[]>([]);
  const [studioInfo, setStudioInfo] = useState<PdfInfo | null>(null);
  const [fieldInputs, setFieldInputs] = useState<Record<string, string>>({});
  const [fieldChecks, setFieldChecks] = useState<Record<string, boolean>>({});
  const [fieldMulti, setFieldMulti] = useState<Record<string, string[]>>({});
  const [issues, setIssues] = useState<FieldIssue[]>([]);
  const [pendingEdits, setPendingEdits] = useState<ObjectEdit[]>([]);
  const [removedObjects, setRemovedObjects] = useState<Set<string>>(new Set());
  const [rectOverrides, setRectOverrides] = useState<Record<string, [number, number, number, number]>>({});
  const [selectedObject, setSelectedObject] = useState<string | null>(null);
  const [flattenAfterFill, setFlattenAfterFill] = useState(false);
  const [dataLoading, setDataLoading] = useState(false);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [dataVersion, setDataVersion] = useState(0);
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{
    key: string;
    mode: "move" | number;
    startX: number;
    startY: number;
    startRect: [number, number, number, number];
  } | null>(null);

  const windows = !isAndroid() && typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- adopting the files handed to the tool
    if (initialFiles?.length) setFiles(initialFiles);
  }, [initialFiles]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<ProgressEvent>("job:progress", (event) => {
      if (running && event.payload.jobId === running) setProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [running]);

  const input = files[0];
  const toast = (kind: "success" | "error", title: string, detail?: string) =>
    useToasts.getState().push({ kind, title, detail });

  const run = async (action: () => Promise<void>, jobId: string) => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    setRunning(jobId);
    setProgress(null);
    try {
      await action();
    } catch (reason) {
      toast("error", t("errors.title"), errorMessage(reason, t));
    } finally {
      setRunning(null);
      setProgress(null);
    }
  };

  const runSanitize = () =>
    run(async () => {
      const report = await invokeTracked<SanitizeReport>("sanitize_pdf", {
        request: { input, jobId: "studio-sanitize", options: null },
      });
      setSanitizeReport(report);
      toast("success", t("studio.sanitizeDone"));
    }, "studio-sanitize");

  const runRepair = () =>
    run(async () => {
      const report = await invokeTracked<RepairReport>("pdf_repair", {
        request: { input, jobId: "studio-repair" },
      });
      setRepairReport(report);
      toast("success", t("studio.repairDone"));
    }, "studio-repair");

  const runLinearize = () =>
    run(async () => {
      const report = await invokeTracked<RepairReport>("pdf_linearize", {
        request: { input, jobId: "studio-linearize" },
      });
      setRepairReport(report);
      toast("success", t("studio.linearizeDone"));
    }, "studio-linearize");

  const runFlatten = () =>
    run(async () => {
      const report = await invokeTracked<FlattenReport>("flatten_pdf", {
        request: { input, jobId: "studio-flatten", options: null },
      });
      setFlattenReport(report);
      toast("success", t("studio.flattenDone"));
    }, "studio-flatten");

  const runValidate = () =>
    run(async () => {
      const report = await invoke<PdfaReport>("pdfa_validate", { request: { input, level } });
      setPdfaReport(report);
    }, "studio-pdfa-validate");

  const runConvert = () =>
    run(async () => {
      const report = await invokeTracked<PdfaReport>("pdfa_convert", {
        request: { input, level, jobId: "studio-pdfa" },
      });
      setPdfaReport(report);
      toast(report.valid ? "success" : "error", report.valid ? t("studio.pdfaValid") : t("studio.pdfaStillFailing"));
    }, "studio-pdfa");

  // -------------------------------------------------------------------------
  // Forms & objects: real AcroForm fields plus annotation/widget/image editing
  // -------------------------------------------------------------------------

  const pageCount = studioInfo?.pageCount ?? 0;
  // Keep the current page inside the (possibly shrunken) document by deriving
  // the clamped value instead of clamping in an effect.
  const shownPage = pageCount && page > pageCount ? pageCount : page;

  const studioGeometry = useMemo<StudioPageGeometry | null>(() => {
    const found = studioInfo?.pageGeometries.find((entry) => entry.page === shownPage);
    if (!found) return null;
    return { width: found.width_pt, height: found.height_pt, rotation: found.rotation ?? 0 };
  }, [studioInfo, shownPage]);

  // Reload the field tree, the page object list and the geometry from the
  // actual file whenever the tab is opened, the input changes or the user
  // asks for a refresh. A failure clears the panels instead of leaving stale
  // objects from a previous document.
  useEffect(() => {
    if (tab !== "objects" || !input) return;
    let cancelled = false;
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the async load owns loading/error
    setDataLoading(true);
    void (async () => {
      try {
        const [fieldList, objectList, info] = await Promise.all([
          pdfListFormFields(input),
          pdfListObjects(input),
          pdfInfo(input),
        ]);
        if (cancelled) return;
        setFields(fieldList ?? []);
        setObjects(objectList ?? []);
        setStudioInfo(info);
        const inputs: Record<string, string> = {};
        const checks: Record<string, boolean> = {};
        const multi: Record<string, string[]> = {};
        for (const field of fieldList ?? []) {
          if (field.fieldType === "checkbox") {
            checks[field.name] = isCheckedValue(field.value);
          } else if (field.fieldType === "choice" && field.multiSelect) {
            multi[field.name] = field.values.length ? field.values : field.value ? [field.value] : [];
          } else {
            inputs[field.name] = field.value;
          }
        }
        setFieldInputs(inputs);
        setFieldChecks(checks);
        setFieldMulti(multi);
        setPendingEdits([]);
        setRemovedObjects(new Set());
        setRectOverrides({});
        setSelectedObject(null);
        setIssues([]);
      } catch (error) {
        if (cancelled) return;
        setFields([]);
        setObjects([]);
        setStudioInfo(null);
        toast("error", t("errors.title"), toAppError(error).message);
      } finally {
        if (!cancelled) setDataLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, input, dataVersion]);

  const pageObjects = useMemo(
    () => objects.filter((object) => object.page === shownPage && !removedObjects.has(objectKey(object))),
    [objects, shownPage, removedObjects],
  );

  const currentRectOf = useCallback(
    (object: PageObjectInfo): [number, number, number, number] => rectOverrides[objectKey(object)] ?? object.rect,
    [rectOverrides],
  );

  const pushEdit = (edit: ObjectEdit) => {
    setPendingEdits((previous) => [
      ...previous.filter(
        (entry) => !(entry.page === edit.page && entry.index === edit.index && entry.action === edit.action),
      ),
      edit,
    ]);
  };

  const buildFieldValues = (): FieldValue[] =>
    fields
      .filter((field) => ["text", "choice", "checkbox", "radio"].includes(field.fieldType))
      .map((field) => {
        if (field.fieldType === "checkbox") {
          return { name: field.name, value: fieldChecks[field.name] ? "true" : "false", values: [] };
        }
        if (field.fieldType === "choice" && field.multiSelect) {
          return { name: field.name, value: "", values: fieldMulti[field.name] ?? [] };
        }
        return { name: field.name, value: fieldInputs[field.name] ?? "", values: [] };
      });

  const validateFormValues = async () => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    setBusyAction("validate");
    try {
      setIssues(await pdfValidateForm(input, buildFieldValues()));
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setBusyAction(null);
    }
  };

  /** Android target picker or the desktop save dialog, shared by both actions. */
  const pickStudioOutput = async (
    suffix: string,
    fallbackName: string,
  ): Promise<{ output: string; androidTarget: AndroidTarget | null } | null> => {
    if (!input) return null;
    if (isAndroid()) {
      const target = await pickAndroidSaveTarget(fallbackName).catch(() => null);
      if (!target) return null;
      return { output: input.replace(/\.pdf$/i, suffix), androidTarget: target };
    }
    const suggested = await suggestOutput(input, suffix).catch(() => input.replace(/\.pdf$/i, suffix));
    const picked = await saveDialog({
      title: t("common.saveAs"),
      defaultPath: suggested,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked) return null;
    return { output: String(picked), androidTarget: null };
  };

  const fillAndSave = async () => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    const target = await pickStudioOutput("-filled.pdf", fileBaseName(input).replace(/\.pdf$/i, "-filled.pdf"));
    if (!target) return;
    setBusyAction("fill");
    try {
      const report = await pdfFillForm({
        input,
        output: { path: target.output, overwrite: "replace" },
        values: buildFieldValues(),
        password: null,
      });
      if (flattenAfterFill) {
        // Flatten after fill uses the existing flatten core (which burns the
        // regenerated appearances into the page); it is not duplicated here.
        await invoke("flatten_pdf", {
          request: {
            input: target.output,
            output: { path: target.output, overwrite: "replace" },
            options: null,
          },
        });
      }
      if (target.androidTarget) await publishOutputs([target.output], { file: target.androidTarget });
      toast("success", t("studio.fillDone"), `${report.filled} / ${fields.length}`);
      if (report.warnings.length) toast("success", t("studio.warnings"), report.warnings.join("\n"));
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setBusyAction(null);
    }
  };

  const applyObjectChanges = async () => {
    if (!input || !pendingEdits.length) return;
    const target = await pickStudioOutput("-objects.pdf", fileBaseName(input).replace(/\.pdf$/i, "-objects.pdf"));
    if (!target) return;
    setBusyAction("objects");
    try {
      const report = await pdfEditObjects({
        input,
        output: { path: target.output, overwrite: "replace" },
        edits: pendingEdits,
        password: null,
      });
      if (target.androidTarget) await publishOutputs([target.output], { file: target.androidTarget });
      toast("success", t("studio.editsApplied"), `${report.edited} / ${report.deleted}`);
      if (report.warnings.length) toast("success", t("studio.warnings"), report.warnings.join("\n"));
      setPendingEdits([]);
      setDataVersion((value) => value + 1);
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setBusyAction(null);
    }
  };

  /** Points-per-pixel scale of the rendered page image (null while loading). */
  const imageScale = (): { sx: number; sy: number; widthPx: number; heightPx: number } | null => {
    const image = canvasRef.current?.querySelector("img");
    if (!image || !studioGeometry) return null;
    const bounds = image.getBoundingClientRect();
    if (bounds.width < 1 || bounds.height < 1) return null;
    const displayed = displayedPageSize(studioGeometry);
    return {
      sx: displayed.width / bounds.width,
      sy: displayed.height / bounds.height,
      widthPx: bounds.width,
      heightPx: bounds.height,
    };
  };

  const startObjectDrag = (event: React.PointerEvent, object: PageObjectInfo, mode: "move" | number) => {
    event.stopPropagation();
    if (!studioGeometry) return;
    const key = objectKey(object);
    if (mode === "move") setSelectedObject(key);
    dragRef.current = {
      key,
      mode,
      startX: event.clientX,
      startY: event.clientY,
      startRect: currentRectOf(object),
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  };

  const handleObjectPointerMove = (event: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag || !studioGeometry) return;
    const scale = imageScale();
    if (!scale) return;
    if (drag.mode === "move") {
      const [dx, dy] = displayDeltaToPage(
        (event.clientX - drag.startX) * scale.sx,
        (event.clientY - drag.startY) * scale.sy,
        studioGeometry.rotation,
      );
      setRectOverrides((previous) => ({
        ...previous,
        [drag.key]: [drag.startRect[0] + dx, drag.startRect[1] + dy, drag.startRect[2] + dx, drag.startRect[3] + dy],
      }));
      return;
    }
    const display = pageRectToDisplayRect(drag.startRect, studioGeometry);
    const nx = display.x + (event.clientX - drag.startX) / scale.widthPx;
    const ny = display.y + (event.clientY - drag.startY) / scale.heightPx;
    let left = display.x;
    let top = display.y;
    let right = display.x + display.w;
    let bottom = display.y + display.h;
    if (drag.mode === 0) {
      left = nx;
      top = ny;
    }
    if (drag.mode === 1) {
      right = nx;
      top = ny;
    }
    if (drag.mode === 2) {
      left = nx;
      bottom = ny;
    }
    if (drag.mode === 3) {
      right = nx;
      bottom = ny;
    }
    const normalized = {
      x: Math.min(left, right),
      y: Math.min(top, bottom),
      w: Math.abs(right - left),
      h: Math.abs(bottom - top),
    };
    setRectOverrides((previous) => ({ ...previous, [drag.key]: displayRectToPageRect(normalized, studioGeometry) }));
  };

  const finishObjectPointer = () => {
    const drag = dragRef.current;
    dragRef.current = null;
    if (!drag) return;
    const object = objects.find((candidate) => objectKey(candidate) === drag.key);
    const rect = rectOverrides[drag.key];
    if (!object || !rect) return;
    if (drag.mode === "move") {
      const dx = rect[0] - drag.startRect[0];
      const dy = rect[1] - drag.startRect[1];
      if (Math.abs(dx) < 0.01 && Math.abs(dy) < 0.01) return;
      pushEdit({ page: object.page, index: object.index, action: "move", dx, dy });
      return;
    }
    pushEdit({ page: object.page, index: object.index, action: "resize", rect });
  };

  const deleteSelectedObject = () => {
    const object = objects.find((candidate) => objectKey(candidate) === selectedObject);
    if (!object) return;
    setRemovedObjects((previous) => new Set(previous).add(objectKey(object)));
    setPendingEdits((previous) => [
      ...previous.filter((entry) => !(entry.page === object.page && entry.index === object.index)),
      { page: object.page, index: object.index, action: "delete" },
    ]);
    setSelectedObject(null);
  };

  const rotateSelectedObject = (degrees: number) => {
    const object = objects.find((candidate) => objectKey(candidate) === selectedObject);
    if (!object) return;
    pushEdit({ page: object.page, index: object.index, action: "rotate", degrees });
    // Preview: quarter turns reshape the box around its center.
    const rect = currentRectOf(object);
    const normalized = ((degrees % 360) + 360) % 360;
    if (normalized === 90 || normalized === 270) {
      const cx = (rect[0] + rect[2]) / 2;
      const cy = (rect[1] + rect[3]) / 2;
      const width = rect[2] - rect[0];
      const height = rect[3] - rect[1];
      setRectOverrides((previous) => ({
        ...previous,
        [objectKey(object)]: [cx - height / 2, cy - width / 2, cx + height / 2, cy + width / 2],
      }));
    }
  };

  const selectedObjectInfo = objects.find((candidate) => objectKey(candidate) === selectedObject) ?? null;

  // -------------------------------------------------------------------------
  // Digital signatures
  // -------------------------------------------------------------------------

  const loadCertificates = async () => {
    try {
      const list = await pdfListSigningCertificates();
      setCertificates(list);
      if (list.length && certIndex === null) setCertIndex(list[0].index);
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    }
  };

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the certificate loader owns its state
    if (tab === "signatures" && windows) void loadCertificates();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the loader is stable and tab/windows gate it
  }, [tab, windows]);

  /** Picks a PFX: on Android through the mobile bridge, otherwise the dialog. */
  const choosePfx = async () => {
    if (isAndroid()) {
      try {
        const mobile = await import("../lib/mobile");
        const helper = (mobile as { pickCertificatePfx?: () => Promise<{ path: string; name: string } | null> })
          .pickCertificatePfx;
        if (helper) {
          const picked = await helper();
          if (picked) {
            setPfxPath(picked.path);
            setPfxName(picked.name);
            setCertIndex(null);
          }
          return;
        }
      } catch {
        // Fall through to the standard dialog below.
      }
    }
    const picked = await open({ multiple: false, filters: [{ name: "PKCS#12", extensions: ["pfx", "p12"] }] });
    if (!picked) return;
    const path = String(Array.isArray(picked) ? picked[0] : picked);
    setPfxPath(path);
    setPfxName(fileBaseName(path));
    setCertIndex(null);
  };

  const verifySignatures = async () => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    setVerifying(true);
    try {
      const report = await pdfVerifySignatures(input);
      setSignReport(report);
      toast("success", t("studio.verifyDone"));
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setVerifying(false);
    }
  };

  const archiveValidationData = async () => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    setArchiving(true);
    try {
      const report = await pdfArchiveValidationData(input);
      setLtvReport(report);
      toast("success", t("studio.ltvDone"));
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setArchiving(false);
    }
  };

  const signDocument = async () => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    if (!pfxPath && certIndex === null) {
      toast("error", t("errors.title"), t("studio.needCertificate"));
      return;
    }
    // Pick the destination with the platform's save flow.
    let output = "";
    let androidTarget: AndroidTarget | null = null;
    const defaultName = fileBaseName(input).replace(/\.pdf$/i, "-signed.pdf");
    if (isAndroid()) {
      const target = await pickAndroidSaveTarget(defaultName).catch(() => null);
      if (!target) return;
      androidTarget = target;
      output = input.replace(/\.pdf$/i, "-signed.pdf");
    } else {
      const suggested = await suggestOutput(input, "-signed").catch(() => input.replace(/\.pdf$/i, "-signed.pdf"));
      const picked = await saveDialog({
        title: t("common.saveAs"),
        defaultPath: suggested,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!picked) return;
      output = String(picked);
    }

    setSigning(true);
    try {
      const result = await pdfSign({
        input,
        output,
        pfxPath,
        pfxPassword: pfxPath ? pfxPassword : null,
        certIndex: pfxPath ? null : certIndex,
        options: {
          page: shownPage,
          reason,
          location,
          appearance,
          signerName: signerName.trim() || null,
        },
      });
      // Never keep the PFX password in component state after signing.
      setPfxPassword("");
      setLastResult(result);
      setSignReport({ signatures: [result.signature], warnings: [] });
      if (androidTarget) {
        await publishOutputs([result.output], { file: androidTarget });
      }
      toast("success", t("studio.signDone"));
    } catch (error) {
      toast("error", t("errors.title"), toAppError(error).message);
    } finally {
      setSigning(false);
    }
  };

  const tabs: { id: StudioTab; label: string; icon: React.ReactElement }[] = [
    { id: "sanitize", label: t("studio.sanitize"), icon: <ShieldAlert size={14} /> },
    { id: "repair", label: t("studio.repair"), icon: <Wrench size={14} /> },
    { id: "flatten", label: t("studio.flatten"), icon: <Layers size={14} /> },
    { id: "pdfa", label: t("studio.pdfa"), icon: <FileCheck2 size={14} /> },
    { id: "signatures", label: t("studio.signatures"), icon: <PenLine size={14} /> },
    { id: "objects", label: t("studio.objects"), icon: <Move size={14} /> },
  ];

  // Rectangles drawn over the rendered page. The overlay lives inside
  // PageCanvas' absolutely positioned layer, so normalized coordinates land on
  // the same pixels the image shows.
  const objectOverlay = studioGeometry ? (
    <div className="absolute inset-0">
      {/* The pointer handlers below drive a ref-based drag; the compiler flags
          the closure's ref reads, but the overlay itself only renders state. */}
      {/* eslint-disable-next-line react-hooks/refs -- drag handlers own the refs; the overlay renders state */}
      {pageObjects.map((object) => {
        const key = objectKey(object);
        const display = pageRectToDisplayRect(currentRectOf(object), studioGeometry);
        const selected = selectedObject === key;
        return (
          <div
            key={key}
            role="button"
            tabIndex={0}
            aria-label={`${object.subtype} ${object.resourceName ?? object.fieldName ?? ""}`.trim()}
            onPointerDown={(event) => startObjectDrag(event, object, "move")}
            onPointerMove={handleObjectPointerMove}
            onPointerUp={finishObjectPointer}
            onPointerCancel={finishObjectPointer}
            onClick={(event) => {
              event.stopPropagation();
              setSelectedObject(key);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                setSelectedObject(key);
              }
            }}
            style={{
              position: "absolute",
              left: `${display.x * 100}%`,
              top: `${display.y * 100}%`,
              width: `${display.w * 100}%`,
              height: `${display.h * 100}%`,
              border: selected
                ? "2px solid var(--accent)"
                : "1px dashed color-mix(in srgb, var(--accent) 55%, transparent)",
              background: selected ? "color-mix(in srgb, var(--accent) 18%, transparent)" : "transparent",
              touchAction: "none",
              cursor: "move",
              padding: 0,
            }}
          >
            {selected
              ? [0, 1, 2, 3].map((corner) => (
                  <span
                    key={corner}
                    role="presentation"
                    onPointerDown={(event) => startObjectDrag(event, object, corner)}
                    onPointerMove={handleObjectPointerMove}
                    onPointerUp={finishObjectPointer}
                    onPointerCancel={finishObjectPointer}
                    style={{
                      position: "absolute",
                      width: 12,
                      height: 12,
                      background: "var(--accent)",
                      border: "1px solid white",
                      left: corner === 0 || corner === 2 ? -6 : "calc(100% - 6px)",
                      top: corner === 0 || corner === 1 ? -6 : "calc(100% - 6px)",
                      touchAction: "none",
                      cursor: "nwse-resize",
                    }}
                  />
                ))
              : null}
          </div>
        );
      })}
    </div>
  ) : null;

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>
            <ShieldCheck size={18} /> {t("studio.title")}
          </h1>
          <p className="muted">{t("studio.subtitle")}</p>
        </div>
      </div>

      <div className="row" style={{ marginBottom: 12 }}>
        {tabs.map((entry) => (
          <button
            key={entry.id}
            type="button"
            className="btn btn-soft"
            data-active={tab === entry.id}
            onClick={() => setTab(entry.id)}
          >
            {entry.icon} {entry.label}
          </button>
        ))}
      </div>

      <Card>
        <DropZone onPaths={(paths) => setFiles(paths)} dragging={dragging ?? false} accept="pdf" />
        <FileList
          files={files.map((path) => ({ path, name: path.split(/[\\/]/).pop() ?? path, sizeBytes: 0 }))}
          onRemove={(index) => setFiles((current) => current.filter((_, position) => position !== index))}
          onAdd={async () => {
            const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
            if (!picked) return;
            setFiles([String(Array.isArray(picked) ? picked[0] : picked)]);
          }}
          addLabel={t("common.addPdf")}
        />
      </Card>

      {progress ? (
        <Card soft>
          <p className="muted small">
            {progress.stage} {progress.total > 0 ? `${progress.current}/${progress.total}` : ""}{" "}
            {progress.message ?? ""}
          </p>
        </Card>
      ) : null}

      {tab === "sanitize" ? (
        <Card>
          <strong>{t("studio.sanitize")}</strong>
          <p className="muted small">{t("studio.sanitizeHint")}</p>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!input || running !== null}
            onClick={() => void runSanitize()}
          >
            {t("studio.run")}
          </button>
          {sanitizeReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone="ok">
                  {t("studio.javascript")}: {sanitizeReport.javascriptRemoved}
                </Badge>
                <Badge tone="ok">
                  {t("studio.attachments")}: {sanitizeReport.embeddedFilesRemoved}
                </Badge>
                <Badge tone="ok">
                  {t("studio.actions")}: {sanitizeReport.actionsRemoved}
                </Badge>
                <Badge tone={sanitizeReport.metadataRemoved ? "ok" : "warn"}>{t("studio.metadata")}</Badge>
                <Badge tone="accent">
                  {t("studio.annotations")}: {sanitizeReport.annotationsRemoved}
                </Badge>
              </div>
              {sanitizeReport.warnings.map((warning, index) => (
                <p key={index} className="muted small">
                  {warning}
                </p>
              ))}
              <p className="muted small">{t("studio.sanitizeVerify")}</p>
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "repair" ? (
        <Card>
          <strong>{t("studio.repair")}</strong>
          <p className="muted small">{t("studio.repairHint")}</p>
          <div className="row">
            <button
              type="button"
              className="btn btn-primary"
              disabled={!input || running !== null}
              onClick={() => void runRepair()}
            >
              {t("studio.repairRun")}
            </button>
            <button
              type="button"
              className="btn"
              disabled={!input || running !== null}
              onClick={() => void runLinearize()}
            >
              {t("studio.linearizeRun")}
            </button>
          </div>
          {repairReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone="ok">
                  {t("studio.repairPages")}: {repairReport.pages}
                </Badge>
                <code className="break-all">{repairReport.output}</code>
              </div>
              {repairReport.warnings.map((warning, index) => (
                <p key={index} className="muted small">
                  {warning}
                </p>
              ))}
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "flatten" ? (
        <Card>
          <strong>{t("studio.flatten")}</strong>
          <p className="muted small">{t("studio.flattenHint")}</p>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!input || running !== null}
            onClick={() => void runFlatten()}
          >
            {t("studio.run")}
          </button>
          {flattenReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone="ok">
                  {t("studio.annotations")}: {flattenReport.annotationsFlattened}
                </Badge>
                <Badge tone="ok">
                  {t("studio.fields")}: {flattenReport.fieldsFlattened}
                </Badge>
                <Badge tone="accent">
                  {t("studio.pages")}: {flattenReport.pagesTouched}
                </Badge>
              </div>
              {flattenReport.warnings.map((warning, index) => (
                <p key={index} className="muted small">
                  {warning}
                </p>
              ))}
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "pdfa" ? (
        <Card>
          <strong>{t("studio.pdfa")}</strong>
          <p className="muted small">{t("studio.pdfaHint")}</p>
          <div className="row">
            <select value={level} onChange={(event) => setLevel(event.target.value)}>
              <option value="A-1b">PDF/A-1b</option>
              <option value="A-2b">PDF/A-2b</option>
              <option value="A-3b">PDF/A-3b</option>
            </select>
            <button
              type="button"
              className="btn btn-soft"
              disabled={!input || running !== null}
              onClick={() => void runValidate()}
            >
              {t("studio.validate")}
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={!input || running !== null}
              onClick={() => void runConvert()}
            >
              {t("studio.convert")}
            </button>
          </div>
          {pdfaReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone={pdfaReport.valid ? "ok" : "danger"}>
                  {pdfaReport.valid ? t("studio.pdfaValid") : t("studio.pdfaInvalid")}
                </Badge>
                <span className="muted small">
                  {pdfaReport.level} · {pdfaReport.failures} {t("studio.failures")} · {pdfaReport.warnings}{" "}
                  {t("studio.warnings")}
                </span>
              </div>
              {pdfaReport.checks.map((check) => (
                <div key={check.id} className="row">
                  <Badge tone={check.status === "pass" ? "ok" : check.status === "warning" ? "warn" : "danger"}>
                    {check.status}
                  </Badge>
                  <strong className="small">{check.id}</strong>
                  <span className="muted small">{check.message}</span>
                </div>
              ))}
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "signatures" ? (
        <>
          <Card>
            <strong>{t("studio.signatures")}</strong>
            <p className="muted small">{t("studio.signaturesHint")}</p>
            <div className="row">
              <button
                type="button"
                className="btn btn-soft"
                disabled={!input || verifying || signing}
                onClick={() => void verifySignatures()}
              >
                <ShieldCheck size={14} /> {t("studio.verify")}
              </button>
              <button
                type="button"
                className="btn btn-soft"
                disabled={!input || archiving || signing}
                onClick={() => void archiveValidationData()}
              >
                <ShieldCheck size={14} /> {t("studio.ltv")}
              </button>
            </div>
            <p className="muted small">{t("studio.ltvHint")}</p>
            {ltvReport ? (
              <p className="muted small" style={{ marginTop: 10 }}>
                <Badge tone="ok">{t("studio.ltvDone")}</Badge> {t("studio.ltvResult")}: {ltvReport.certificates}
                {ltvReport.warnings.map((warning, index) => (
                  <span key={index} className="muted small">
                    {" "}
                    {warning}
                  </span>
                ))}
              </p>
            ) : null}
            {signReport ? (
              <>
                {signReport.warnings.map((warning, index) => (
                  <p key={index} className="muted small" style={{ marginTop: 10 }}>
                    <Badge tone="warn">{t("studio.notes")}</Badge> {warning}
                  </p>
                ))}
                {signReport.signatures.length ? (
                  signReport.signatures.map((info, index) => (
                    <div key={index} className="stack" style={{ marginTop: 12 }}>
                      <div className="row">
                        <Badge tone={signatureTone(info)}>
                          {info.signatureValid ? t("studio.signatureValid") : t("studio.signatureInvalid")}
                        </Badge>
                        <Badge tone={info.digestMatches ? "ok" : "danger"}>
                          {info.digestMatches ? t("studio.digestOk") : t("studio.digestBad")}
                        </Badge>
                        <Badge tone={info.coversWholeDocument ? "ok" : "warn"}>{t("studio.coversWhole")}</Badge>
                        {info.modifiedAfterSigning ? <Badge tone="danger">{t("studio.modifiedAfter")}</Badge> : null}
                        {info.supersededByLaterRevision ? <Badge tone="warn">{t("studio.superseded")}</Badge> : null}
                        <Badge tone="warn">{t("studio.trustUnknown")}</Badge>
                      </div>
                      <p className="muted small">
                        <strong>{info.fieldName}</strong> · {info.subFilter || "?"} · {info.algorithm}
                      </p>
                      <p className="muted small">
                        {t("studio.signer")}: {info.signer.subject || "?"}
                      </p>
                      <p className="muted small">
                        {t("studio.issuer")}: {info.signer.issuer || "?"}
                      </p>
                      <p className="muted small">
                        {t("studio.serial")}: {info.signer.serialHex || "?"} · {t("studio.validity")}:{" "}
                        {info.signer.notBefore || "?"} → {info.signer.notAfter || "?"}
                        {info.signer.expired ? ` (${t("studio.expired")})` : ""}
                      </p>
                      {info.signingTime ? (
                        <p className="muted small">
                          {t("studio.signingTime")}: {info.signingTime}
                        </p>
                      ) : null}
                      <p className="muted small">
                        {t("studio.fingerprint")}: {info.signer.sha256Fingerprint || "?"}
                      </p>
                      {info.chain.length > 1 ? (
                        <div className="stack">
                          <p className="muted small">
                            {t("studio.chain")}: {info.chainLinked ? t("studio.chainLinked") : t("studio.chainBroken")}
                            {info.selfSignedChain ? ` · ${t("studio.selfSigned")}` : ""}
                          </p>
                          {info.chain.map((cert, position) => (
                            <p key={position} className="muted small">
                              {position + 1}. {cert.subject}
                              {cert.isCa ? " (CA)" : ""}
                              {cert.expired ? ` (${t("studio.expired")})` : ""}
                            </p>
                          ))}
                        </div>
                      ) : null}
                      {info.notes.map((note, position) => (
                        <p key={position} className="muted small">
                          {t("studio.notes")}: {note}
                        </p>
                      ))}
                    </div>
                  ))
                ) : signReport.warnings.length ? null : (
                  <p className="muted small" style={{ marginTop: 10 }}>
                    {t("studio.noSignatures")}
                  </p>
                )}
              </>
            ) : null}
          </Card>

          <Card>
            <strong>{t("studio.signSection")}</strong>
            <div className="stack">
              {windows ? (
                <div className="stack">
                  <div className="row">
                    <span className="muted small">{t("studio.storeCertificates")}</span>
                    <button type="button" className="btn btn-soft" onClick={() => void loadCertificates()}>
                      <RefreshCw size={13} /> {t("studio.refresh")}
                    </button>
                  </div>
                  {certificates.length ? (
                    <select
                      value={certIndex ?? ""}
                      onChange={(event) => {
                        setCertIndex(Number(event.target.value));
                        setPfxPath(null);
                      }}
                    >
                      {certificates.map((cert) => (
                        <option key={cert.index} value={cert.index}>
                          {cert.subject}
                          {cert.expired ? ` (${t("studio.expired")})` : ""}
                          {cert.hasPrivateKey ? "" : " - no private key"}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <p className="muted small">{t("studio.noStoreCertificates")}</p>
                  )}
                  <p className="muted small">{t("studio.storeSigningNote")}</p>
                </div>
              ) : null}

              <div className="row">
                <button type="button" className="btn btn-soft" onClick={() => void choosePfx()}>
                  {t("studio.choosePfx")}
                </button>
                {pfxName ? <span className="muted small">{pfxName}</span> : null}
              </div>
              {pfxPath ? (
                <Field label={t("studio.pfxPassword")} hint={t("studio.pfxHint")}>
                  <input
                    className="input"
                    type="password"
                    value={pfxPassword}
                    onChange={(event) => setPfxPassword(event.target.value)}
                    autoComplete="off"
                    spellCheck={false}
                  />
                </Field>
              ) : null}

              <div className="row">
                <Field label={t("studio.page")}>
                  <input
                    className="input"
                    type="number"
                    min={1}
                    value={shownPage}
                    onChange={(event) => setPage(Math.max(1, Number(event.target.value) || 1))}
                    style={{ width: 90 }}
                  />
                </Field>
                <Field label={t("studio.reason")}>
                  <input className="input" value={reason} onChange={(event) => setReason(event.target.value)} />
                </Field>
                <Field label={t("studio.location")}>
                  <input className="input" value={location} onChange={(event) => setLocation(event.target.value)} />
                </Field>
                <Field label={t("studio.signerName")}>
                  <input className="input" value={signerName} onChange={(event) => setSignerName(event.target.value)} />
                </Field>
              </div>
              <Toggle checked={appearance} onChange={setAppearance} label={t("studio.visibleAppearance")} />

              <button
                type="button"
                className="btn btn-primary"
                disabled={!input || signing || verifying || (!pfxPath && certIndex === null)}
                onClick={() => void signDocument()}
              >
                <PenLine size={14} /> {t("studio.signSave")}
              </button>

              {lastResult ? (
                <div className="stack">
                  <Badge tone={lastResult.signature.signatureValid ? "ok" : "danger"}>
                    <BadgeCheck size={12} />{" "}
                    {lastResult.signature.signatureValid ? t("studio.signatureValid") : t("studio.signatureInvalid")}
                  </Badge>
                  <p className="muted small">{lastResult.output}</p>
                </div>
              ) : null}
            </div>
          </Card>
        </>
      ) : null}

      {tab === "objects" ? (
        <>
          <Card>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <div>
                <strong>{t("studio.fields")}</strong>
                <p className="muted small">{t("studio.objectsHint")}</p>
              </div>
              <button
                type="button"
                className="btn btn-soft"
                disabled={!input || dataLoading}
                onClick={() => setDataVersion((value) => value + 1)}
              >
                <RefreshCw size={13} /> {t("studio.refreshData")}
              </button>
            </div>
            {!dataLoading && !fields.length ? <p className="muted small">{t("studio.noFields")}</p> : null}
            {fields.map((field) => {
              const fieldIssues = issues.filter((issue) => issue.field === field.name);
              return (
                <div
                  key={field.name}
                  className="stack"
                  style={{ marginTop: 10, borderTop: "1px solid var(--border)", paddingTop: 8 }}
                >
                  <div className="row">
                    <strong className="small">{field.name}</strong>
                    <Badge tone="accent">{field.fieldType}</Badge>
                    {field.required ? <Badge tone="warn">{t("studio.required")}</Badge> : null}
                    {field.readOnly ? <Badge>{t("studio.readOnly")}</Badge> : null}
                    {field.hasScript ? <Badge tone="warn">{t("studio.scripted")}</Badge> : null}
                    {field.maxLength !== null ? (
                      <span className="muted small">{t("studio.maxLength", { count: field.maxLength })}</span>
                    ) : null}
                    {field.tooltip ? <span className="muted small">{field.tooltip}</span> : null}
                  </div>
                  {field.fieldType === "text" ? (
                    field.multiline ? (
                      <textarea
                        className="input"
                        rows={3}
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      />
                    ) : (
                      <input
                        className="input"
                        type={field.password ? "password" : "text"}
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      />
                    )
                  ) : null}
                  {field.fieldType === "checkbox" ? (
                    <label className="row small">
                      <input
                        type="checkbox"
                        checked={fieldChecks[field.name] ?? false}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldChecks((previous) => ({ ...previous, [field.name]: event.target.checked }))
                        }
                      />
                      {field.options.length ? field.options.map((option) => option.value).join(", ") : "Yes / Off"}
                    </label>
                  ) : null}
                  {field.fieldType === "radio" ? (
                    field.options.length ? (
                      <select
                        className="input"
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      >
                        <option value="">—</option>
                        {field.options.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <input
                        className="input"
                        placeholder={t("studio.radioManual")}
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      />
                    )
                  ) : null}
                  {field.fieldType === "choice" ? (
                    field.editable ? (
                      <input
                        className="input"
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      />
                    ) : field.multiSelect ? (
                      <select
                        className="input"
                        multiple
                        value={fieldMulti[field.name] ?? []}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldMulti((previous) => ({
                            ...previous,
                            [field.name]: Array.from(event.target.selectedOptions).map((option) => option.value),
                          }))
                        }
                      >
                        {field.options.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <select
                        className="input"
                        value={fieldInputs[field.name] ?? ""}
                        disabled={field.readOnly}
                        onChange={(event) =>
                          setFieldInputs((previous) => ({ ...previous, [field.name]: event.target.value }))
                        }
                      >
                        <option value="">—</option>
                        {field.options.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    )
                  ) : null}
                  {field.fieldType === "pushbutton" ||
                  field.fieldType === "signature" ||
                  field.fieldType === "unknown" ? (
                    <p className="muted small">{t("studio.unsupportedField")}</p>
                  ) : null}
                  {fieldIssues.map((issue, index) => (
                    <p
                      key={index}
                      className="small"
                      style={issue.severity === "error" ? { color: "var(--danger)" } : undefined}
                    >
                      {t(`studio.issue.${issueI18nKey(issue.code)}`)} — {issue.message}
                    </p>
                  ))}
                </div>
              );
            })}
            <div className="row" style={{ marginTop: 12 }}>
              <button
                type="button"
                className="btn btn-soft"
                disabled={!input || busyAction !== null || !fields.length}
                onClick={() => void validateFormValues()}
              >
                {t("studio.validate")}
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={!input || busyAction !== null || !fields.length}
                onClick={() => void fillAndSave()}
              >
                {t("studio.fillSave")}
              </button>
            </div>
            <Toggle checked={flattenAfterFill} onChange={setFlattenAfterFill} label={t("studio.flattenAfter")} />
            {issues.length ? (
              <p className="muted small" style={{ marginTop: 8 }}>
                {t("studio.issues")}: {issues.length}
              </p>
            ) : null}
          </Card>

          <Card>
            <strong>{t("studio.objectsOnPage", { page: shownPage })}</strong>
            <p className="muted small">{t("studio.selectObjectHint")}</p>
            {pageCount > 1 ? (
              <div className="row" style={{ justifyContent: "center", marginBottom: 8 }}>
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={shownPage <= 1}
                  onClick={() => setPage((value) => Math.max(1, value - 1))}
                >
                  ‹
                </button>
                <span className="muted small">
                  {shownPage} / {pageCount}
                </span>
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={shownPage >= pageCount}
                  onClick={() => setPage((value) => Math.min(pageCount, value + 1))}
                >
                  ›
                </button>
              </div>
            ) : null}
            {input ? (
              <div className="mx-auto" style={{ maxWidth: 620 }}>
                <div ref={canvasRef} style={{ width: "fit-content", margin: "0 auto" }}>
                  <PageCanvas path={input} page={shownPage} maxWidth={820} overlay={objectOverlay} />
                </div>
              </div>
            ) : null}
            {!dataLoading && !pageObjects.length ? <p className="muted small">{t("studio.noObjects")}</p> : null}
            <div className="stack" style={{ marginTop: 10 }}>
              {pageObjects.map((object) => {
                const key = objectKey(object);
                const kindLabel =
                  object.kind === "image"
                    ? t("studio.objectImage")
                    : object.kind === "widget"
                      ? t("studio.objectWidget")
                      : t("studio.objectAnnotation");
                return (
                  <button
                    key={key}
                    type="button"
                    className="btn btn-soft"
                    data-active={selectedObject === key}
                    onClick={() => setSelectedObject(key)}
                  >
                    <span className="small">
                      {kindLabel} · {object.subtype}
                      {object.fieldName ? ` · ${object.fieldName}` : ""}
                      {object.resourceName ? ` · ${object.resourceName}` : ""}
                    </span>
                  </button>
                );
              })}
            </div>
            {selectedObjectInfo ? (
              <div className="row" style={{ marginTop: 10 }}>
                <button type="button" className="btn btn-soft" onClick={() => rotateSelectedObject(-90)}>
                  <RotateCcw size={13} /> {t("studio.rotateLeft")}
                </button>
                <button type="button" className="btn btn-soft" onClick={() => rotateSelectedObject(90)}>
                  <RotateCw size={13} /> {t("studio.rotateRight")}
                </button>
                <button type="button" className="btn btn-soft" onClick={deleteSelectedObject}>
                  <Trash2 size={13} /> {t("studio.deleteObject")}
                </button>
              </div>
            ) : null}
            <div className="row" style={{ marginTop: 10 }}>
              <button
                type="button"
                className="btn btn-primary"
                disabled={!pendingEdits.length || busyAction !== null}
                onClick={() => void applyObjectChanges()}
              >
                {t("studio.applyEdits")}
              </button>
              <span className="muted small">{t("studio.pendingEdits", { count: pendingEdits.length })}</span>
            </div>
          </Card>
        </>
      ) : null}
    </div>
  );
}
