/**
 * The scanner's four-corner picker.
 *
 * Shows an upright, downscaled preview of a page photo (`office_scan_preview`)
 * with four handles to drag onto the page's corners; arrow keys move the
 * focused handle by 1 % (Shift: 5 %). The corners start where the on-device
 * detector (`office_scan_detect`) found the page, or on the default inset when
 * it found nothing; "find corners automatically" runs it again. Saving hands
 * back the corners in pixels of the full-size photo, which `office_scan_pages`
 * flattens - or nothing when the whole photo is kept. This is the
 * privacy-preserving counterpart of the ML Kit document scanner: no Google
 * Play services and no usage logging.
 */
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { Maximize, Search } from "lucide-react";
import { Button, Modal } from "../components/ui";
import { useT } from "../lib/i18n";
import { scanPreview, type ScanPreview } from "../lib/office-api";
import { detectedCorners, scanDetect } from "../lib/scan-detect";
import {
  cornersToPixels,
  defaultCorners,
  isFullFrame,
  moveCorner,
  pixelsToCorners,
  type Corner,
} from "../lib/scan-corners";

const CORNER_KEYS = [
  "scanner.cornerTopLeft",
  "scanner.cornerTopRight",
  "scanner.cornerBottomRight",
  "scanner.cornerBottomLeft",
];

const FULL_FRAME: Corner[] = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];

export function CornerPicker({
  name,
  path,
  initial,
  onSave,
  onClose,
}: {
  name: string;
  path: string;
  /** Corners chosen earlier, in full-size photo pixels. */
  initial?: [number, number][];
  /** Pixel corners, or undefined to keep the whole photo. */
  onSave: (corners: [number, number][] | undefined) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [preview, setPreview] = useState<ScanPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [corners, setCorners] = useState<Corner[]>(defaultCorners);
  const [detection, setDetection] = useState<"plain" | "detecting" | "found" | "missing">(
    initial ? "plain" : "detecting",
  );
  const frameRef = useRef<HTMLDivElement | null>(null);
  const dragging = useRef<number | null>(null);

  useEffect(() => {
    let live = true;
    scanPreview(path)
      .then(async (result) => {
        if (!live) return;
        setPreview(result);
        if (initial) {
          setCorners(pixelsToCorners(initial, result.width, result.height));
          return;
        }
        // No corners yet: ask the detector where the page is.
        const found = detectedCorners(await scanDetect(path).catch(() => null));
        if (!live) return;
        if (found) {
          setCorners(pixelsToCorners(found, result.width, result.height));
          setDetection("found");
        } else {
          setDetection("missing");
        }
      })
      .catch((reason: unknown) => {
        if (live) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      live = false;
    };
    // The preview and the automatic detection load once per opened picker.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path]);

  const findCorners = async () => {
    if (!preview) return;
    setDetection("detecting");
    const found = detectedCorners(await scanDetect(path).catch(() => null));
    if (found) {
      setCorners(pixelsToCorners(found, preview.width, preview.height));
      setDetection("found");
    } else {
      setDetection("missing");
    }
  };

  const pointAt = (event: PointerEvent): Corner | null => {
    const rect = frameRef.current?.getBoundingClientRect();
    if (!rect || !rect.width || !rect.height) return null;
    return [(event.clientX - rect.left) / rect.width, (event.clientY - rect.top) / rect.height];
  };

  const onPointerDown = (index: number) => (event: PointerEvent<HTMLButtonElement>) => {
    dragging.current = index;
    event.currentTarget.setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragging.current === null) return;
    const point = pointAt(event);
    if (point) setCorners((current) => moveCorner(current, dragging.current!, point));
  };
  const onPointerUp = () => {
    dragging.current = null;
  };

  const onKeyDown = (index: number) => (event: KeyboardEvent<HTMLButtonElement>) => {
    const step = event.shiftKey ? 0.05 : 0.01;
    const delta: Record<string, Corner> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step],
    };
    const move = delta[event.key];
    if (!move) return;
    event.preventDefault();
    setCorners((current) => moveCorner(current, index, [current[index][0] + move[0], current[index][1] + move[1]]));
  };

  const save = () => {
    if (!preview) return;
    onSave(isFullFrame(corners) ? undefined : cornersToPixels(corners, preview.width, preview.height));
  };

  const polygon = corners.map(([x, y]) => `${x * 100},${y * 100}`).join(" ");

  return (
    <Modal
      title={t("scanner.cornersTitle", { name })}
      onClose={onClose}
      width={640}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!preview} onClick={save}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <p className="text-sm muted mb-3">{t("scanner.cornersHint")}</p>
      {detection === "detecting" ? <p className="text-xs muted mb-2">{t("scanner.detecting")}</p> : null}
      {detection === "found" ? <p className="text-xs muted mb-2">{t("scanner.autoFound")}</p> : null}
      {detection === "missing" ? <p className="text-xs text-[var(--warn)] mb-2">{t("scanner.autoMissing")}</p> : null}
      {error ? <p className="text-sm text-[var(--danger)]">{error}</p> : null}
      {preview ? (
        <div className="flex flex-col gap-3 items-center">
          <div ref={frameRef} className="relative select-none touch-none" style={{ maxWidth: "100%", lineHeight: 0 }}>
            <img
              src={preview.dataUrl}
              alt={name}
              draggable={false}
              style={{ maxWidth: "100%", maxHeight: "60vh", display: "block" }}
            />
            <svg
              viewBox="0 0 100 100"
              preserveAspectRatio="none"
              className="absolute inset-0 w-full h-full pointer-events-none"
              aria-hidden="true"
            >
              <polygon
                points={polygon}
                fill="rgba(59,130,246,0.15)"
                stroke="var(--accent)"
                strokeWidth={0.6}
                vectorEffect="non-scaling-stroke"
              />
            </svg>
            {corners.map(([x, y], index) => (
              <button
                key={CORNER_KEYS[index]}
                type="button"
                aria-label={t(CORNER_KEYS[index])}
                className="absolute rounded-full border-2"
                style={{
                  left: `${x * 100}%`,
                  top: `${y * 100}%`,
                  width: 28,
                  height: 28,
                  transform: "translate(-50%, -50%)",
                  background: "var(--accent-weak)",
                  borderColor: "var(--accent)",
                  touchAction: "none",
                  cursor: "grab",
                }}
                onPointerDown={onPointerDown(index)}
                onPointerMove={onPointerMove}
                onPointerUp={onPointerUp}
                onPointerCancel={onPointerUp}
                onKeyDown={onKeyDown(index)}
              />
            ))}
          </div>
          <div className="flex items-center gap-2 flex-wrap justify-center">
            <Button size="sm" variant="ghost" disabled={detection === "detecting"} onClick={() => void findCorners()}>
              <Search size={14} /> {t("scanner.autoDetect")}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setCorners(FULL_FRAME)}>
              <Maximize size={14} /> {t("scanner.cornersWhole")}
            </Button>
          </div>
        </div>
      ) : error ? null : (
        <p className="text-sm muted">{t("common.loading")}</p>
      )}
    </Modal>
  );
}
