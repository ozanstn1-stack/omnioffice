/**
 * Dependency-free signature pad: draws on a 2D canvas with pointer events
 * (mouse, touch or stylus) and exports the strokes as a transparent PNG. The
 * same helpers also turn a picked PNG/JPG into annotation bytes. Kept free of
 * canvas libraries so the Android WebView build stays small.
 */
import { useRef, useState } from "react";
import { Image as ImageIcon } from "lucide-react";
import { Button, Modal } from "../../components/ui";
import { useT } from "../../lib/i18n";
import { pickFileBytes } from "../../lib/mobile";

/** Backing-store size of the pad in pixels; the CSS box scales it. */
export const PAD_WIDTH = 960;
export const PAD_HEIGHT = 360;

const INK = "#101418";
const INK_WIDTH = 3;

export interface PadPoint {
  x: number;
  y: number;
}

/** SVG path for points, e.g. for a stroke preview (`M x y L x y ...`). */
export function strokePath(points: PadPoint[]): string {
  return points.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`).join(" ");
}

/** Base64 body (no data-URL prefix) of raw bytes. */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk));
  }
  return btoa(binary);
}

/** MIME type of an image stored as base64, from the PNG/JPEG magic. */
export function mimeForBase64(base64: string): string {
  return base64.startsWith("/9j/") ? "image/jpeg" : "image/png";
}

/** Data URL for image/signature bytes stored as base64. */
export function dataUrlForBase64(base64: string): string {
  return `data:${mimeForBase64(base64)};base64,${base64}`;
}

/**
 * Rasterizes strokes into a transparent PNG and returns its base64 body.
 * Null when no 2D context is available (headless environments).
 */
export function strokesToPngBase64(strokes: PadPoint[][], width = PAD_WIDTH, height = PAD_HEIGHT): string | null {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.clearRect(0, 0, width, height);
  context.strokeStyle = INK;
  context.lineWidth = INK_WIDTH;
  context.lineCap = "round";
  context.lineJoin = "round";
  for (const stroke of strokes) {
    if (stroke.length < 2) continue;
    context.beginPath();
    context.moveTo(stroke[0].x, stroke[0].y);
    for (const point of stroke.slice(1)) context.lineTo(point.x, point.y);
    context.stroke();
  }
  const dataUrl = canvas.toDataURL("image/png");
  const comma = dataUrl.indexOf(",");
  return comma >= 0 ? dataUrl.slice(comma + 1) : null;
}

/** Drawing dialog: draw a signature or pick a PNG/JPG of one. */
export function SignaturePad({ onUse, onClose }: { onUse: (base64: string) => void; onClose: () => void }) {
  const t = useT();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const strokesRef = useRef<PadPoint[][]>([]);
  const drawingRef = useRef(false);
  const [strokeCount, setStrokeCount] = useState(0);

  const toPoint = (event: { clientX: number; clientY: number }): PadPoint | null => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const bounds = canvas.getBoundingClientRect();
    if (bounds.width < 1 || bounds.height < 1) return null;
    return {
      x: ((event.clientX - bounds.left) / bounds.width) * PAD_WIDTH,
      y: ((event.clientY - bounds.top) / bounds.height) * PAD_HEIGHT,
    };
  };

  const drawSegment = (from: PadPoint, to: PadPoint) => {
    const context = canvasRef.current?.getContext("2d");
    if (!context) return;
    context.strokeStyle = INK;
    context.lineWidth = INK_WIDTH;
    context.lineCap = "round";
    context.lineJoin = "round";
    context.beginPath();
    context.moveTo(from.x, from.y);
    context.lineTo(to.x, to.y);
    context.stroke();
  };

  const clear = () => {
    strokesRef.current = [];
    setStrokeCount(0);
    canvasRef.current?.getContext("2d")?.clearRect(0, 0, PAD_WIDTH, PAD_HEIGHT);
  };

  const applySignature = () => {
    const base64 = strokesToPngBase64(strokesRef.current);
    if (base64) onUse(base64);
  };

  const pickSignatureImage = async () => {
    const picked = await pickFileBytes({
      name: "PNG / JPG",
      extensions: ["png", "jpg", "jpeg"],
      mimeTypes: ["image/png", "image/jpeg"],
    }).catch(() => null);
    if (!picked) return;
    onUse(bytesToBase64(picked.bytes));
  };

  return (
    <Modal
      title={t("annotate.signaturePad")}
      onClose={onClose}
      width={680}
      footer={
        <>
          <Button variant="ghost" onClick={clear} disabled={!strokeCount}>
            {t("annotate.signatureClear")}
          </Button>
          <Button variant="ghost" icon={<ImageIcon size={14} />} onClick={() => void pickSignatureImage()}>
            {t("annotate.signatureImage")}
          </Button>
          <Button variant="primary" onClick={applySignature} disabled={!strokeCount}>
            {t("annotate.signatureUse")}
          </Button>
        </>
      }
    >
      <p className="muted small mb-2">{t("annotate.signatureHint")}</p>
      <canvas
        ref={canvasRef}
        width={PAD_WIDTH}
        height={PAD_HEIGHT}
        className="annotate-signature-pad"
        aria-label={t("annotate.signaturePad")}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture?.(event.pointerId);
          const point = toPoint(event);
          if (!point) return;
          drawingRef.current = true;
          strokesRef.current = [...strokesRef.current, [point]];
          setStrokeCount(strokesRef.current.length);
        }}
        onPointerMove={(event) => {
          if (!drawingRef.current) return;
          const point = toPoint(event);
          if (!point) return;
          const strokes = strokesRef.current;
          const stroke = strokes[strokes.length - 1];
          const previous = stroke?.[stroke.length - 1];
          if (!previous) return;
          strokesRef.current = [...strokes.slice(0, -1), [...stroke, point]];
          drawSegment(previous, point);
        }}
        onPointerUp={() => {
          drawingRef.current = false;
        }}
        onPointerCancel={() => {
          drawingRef.current = false;
        }}
      />
    </Modal>
  );
}
