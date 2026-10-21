/** Draw: vector shapes with SVG export. */
import { useEffect, useRef, useState } from "react";
import { FileDown, Image as ImageIcon, Plus, Trash2, FileImage } from "lucide-react";
import { useT } from "../../lib/i18n";
import { reportError, useToasts } from "../../lib/store";
import { saveFileBytes } from "../../lib/mobile";
import { jpegToPdf } from "../../lib/image-pdf";
import { uid } from "../../lib/office-types";
import { useDraw, type DrawDocument } from "../../lib/office-store";
import { Screen } from "../../components/layout";

// ---------------------------------------------------------------------------
// Draw
// ---------------------------------------------------------------------------

export function DrawScreen() {
  const t = useT();
  const { documents, loaded, load, save, remove } = useDraw();
  const [activeId, setActiveId] = useState<string | null>(null);
  const [tool, setTool] = useState<DrawDocument["elements"][number]["kind"]>("rect");
  const [color, setColor] = useState("#2563eb");
  const [stroke, setStroke] = useState("#1e293b");
  const [drawing, setDrawing] = useState<{
    startX: number;
    startY: number;
    points: Array<{ x: number; y: number }>;
  } | null>(null);
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!loaded) void load();
  }, [loaded, load]);

  const active = documents.find((document) => document.id === activeId) ?? documents[0] ?? null;

  const createDocument = () => {
    const document: DrawDocument = {
      id: uid(),
      name: `Drawing ${documents.length + 1}`,
      width: 800,
      height: 520,
      elements: [],
      updatedAt: new Date().toISOString(),
    };
    save(document);
    setActiveId(document.id);
  };

  const addElement = (element: DrawDocument["elements"][number]) => {
    if (!active) return;
    save({ ...active, elements: [...active.elements, element], updatedAt: new Date().toISOString() });
  };

  const pointerDown = (event: React.MouseEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (tool === "path") {
      setDrawing({ startX: x, startY: y, points: [{ x, y }] });
    } else {
      setDrawing({ startX: x, startY: y, points: [] });
    }
  };

  const pointerMove = (event: React.MouseEvent<SVGSVGElement>) => {
    if (!drawing) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    if (tool === "path") setDrawing({ ...drawing, points: [...drawing.points, { x, y }] });
  };

  const pointerUp = (event: React.MouseEvent<SVGSVGElement>) => {
    if (!drawing) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const width = Math.abs(x - drawing.startX);
    const height = Math.abs(y - drawing.startY);
    const left = Math.min(x, drawing.startX);
    const top = Math.min(y, drawing.startY);
    if (tool === "path") {
      if (drawing.points.length > 2)
        addElement({ id: uid(), kind: "path", x: 0, y: 0, w: 0, h: 0, points: drawing.points, stroke, strokeWidth: 2 });
    } else if (tool === "line" || tool === "arrow") {
      addElement({
        id: uid(),
        kind: tool,
        x: drawing.startX,
        y: drawing.startY,
        w: x - drawing.startX,
        h: y - drawing.startY,
        stroke,
        strokeWidth: 2,
      });
    } else if (tool === "text") {
      const text = window.prompt(t("draw.textPrompt")) ?? "Text";
      addElement({ id: uid(), kind: "text", x: left, y: top + 18, w: Math.max(80, width), h: 28, text, fill: color });
    } else {
      addElement({
        id: uid(),
        kind: tool,
        x: left,
        y: top,
        w: Math.max(6, width),
        h: Math.max(6, height),
        fill: tool === "rect" || tool === "ellipse" ? color : null,
        stroke: tool === "rect" || tool === "ellipse" ? undefined : stroke,
        strokeWidth: 2,
      });
    }
    setDrawing(null);
  };

  // Every export goes through saveFileBytes: the desktop save dialog or the
  // Android SAF picker, one write to the chosen destination. (The old PDF
  // export staged a temporary PNG next to the target, which the fs scope
  // refuses on the desktop and which cannot exist on Android.)
  const rasterize = async (mime: "image/png" | "image/jpeg"): Promise<Uint8Array> => {
    if (!active) return new Uint8Array();
    const blob = new Blob([buildSvg(active)], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    try {
      const image = new Image();
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error("Could not rasterise the drawing."));
        image.src = url;
      });
      const canvas = document.createElement("canvas");
      canvas.width = active.width * 2;
      canvas.height = active.height * 2;
      const context = canvas.getContext("2d")!;
      context.fillStyle = "#ffffff";
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      const base64 = canvas.toDataURL(mime, 0.92).split(",")[1] ?? "";
      return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
    } finally {
      URL.revokeObjectURL(url);
    }
  };

  const exportAs = async (format: "svg" | "png" | "pdf") => {
    if (!active) return;
    try {
      let bytes: Uint8Array;
      if (format === "svg") {
        bytes = new TextEncoder().encode(buildSvg(active));
      } else if (format === "png") {
        bytes = await rasterize("image/png");
      } else {
        // One page the size of the drawing (1 px = 1 pt), rendered at 2x.
        bytes = jpegToPdf(
          await rasterize("image/jpeg"),
          active.width * 2,
          active.height * 2,
          active.width,
          active.height,
        );
      }
      const saved = await saveFileBytes(bytes, `${active.name}.${format}`, {
        name: format.toUpperCase(),
        extensions: [format],
      });
      if (!saved) return;
      useToasts.getState().push({ kind: "success", title: t("draw.exported"), detail: saved });
    } catch (error) {
      reportError(error, t);
    }
  };
  const exportSvg = () => void exportAs("svg");
  const exportPng = () => void exportAs("png");
  const exportPdf = () => void exportAs("pdf");

  return (
    <Screen
      title={t("draw.title")}
      subtitle={t("draw.subtitle")}
      actions={
        <div className="row">
          <button type="button" className="btn btn-soft" onClick={exportSvg} disabled={!active}>
            SVG
          </button>
          <button type="button" className="btn btn-soft" onClick={exportPng} disabled={!active}>
            <FileImage size={15} /> PNG
          </button>
          <button type="button" className="btn btn-soft" onClick={exportPdf} disabled={!active}>
            <FileDown size={15} /> PDF
          </button>
          <button type="button" className="btn btn-primary" onClick={createDocument}>
            <Plus size={15} /> {t("draw.newDrawing")}
          </button>
        </div>
      }
    >
      <div className="draw-layout">
        <div className="card draw-toolbar">
          {(["rect", "ellipse", "line", "arrow", "text", "path"] as const).map((candidate) => (
            <button
              key={candidate}
              type="button"
              className={`chip${tool === candidate ? " is-active" : ""}`}
              onClick={() => setTool(candidate)}
            >
              {t(`draw.tool_${candidate}`)}
            </button>
          ))}
          <label className="field">
            <span>{t("draw.fill")}</span>
            <input type="color" value={color} onChange={(event) => setColor(event.target.value)} />
          </label>
          <label className="field">
            <span>{t("draw.stroke")}</span>
            <input type="color" value={stroke} onChange={(event) => setStroke(event.target.value)} />
          </label>
        </div>
        <div className="card draw-canvas-card">
          {active ? (
            <svg
              ref={svgRef}
              className="draw-canvas"
              viewBox={`0 0 ${active.width} ${active.height}`}
              onMouseDown={pointerDown}
              onMouseMove={pointerMove}
              onMouseUp={pointerUp}
              onMouseLeave={() => setDrawing(null)}
            >
              <rect width={active.width} height={active.height} fill="#ffffff" />
              {active.elements.map((element) => (
                <DrawElementView key={element.id} element={element} />
              ))}
              {drawing && tool === "path" ? (
                <polyline
                  points={drawing.points.map((point) => `${point.x},${point.y}`).join(" ")}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={2}
                />
              ) : null}
            </svg>
          ) : (
            <div className="empty-state">
              <ImageIcon size={22} />
              <p className="muted">{t("draw.none")}</p>
              <button type="button" className="btn btn-primary" onClick={createDocument}>
                {t("draw.newDrawing")}
              </button>
            </div>
          )}
        </div>
        <div className="card draw-list">
          {documents.map((document) => (
            <div key={document.id} className="row">
              <button
                type="button"
                className={`note-item${active?.id === document.id ? " is-active" : ""}`}
                onClick={() => setActiveId(document.id)}
              >
                {document.name}
              </button>
              <button type="button" className="icon-btn" onClick={() => remove(document.id)}>
                <Trash2 size={12} />
              </button>
            </div>
          ))}
        </div>
      </div>
    </Screen>
  );
}

function DrawElementView({ element }: { element: DrawDocument["elements"][number] }) {
  if (element.kind === "rect")
    return (
      <rect
        x={element.x}
        y={element.y}
        width={element.w}
        height={element.h}
        fill={element.fill ?? "none"}
        stroke={element.stroke === null ? undefined : element.stroke}
        strokeWidth={element.strokeWidth ?? 0}
        rx={6}
      />
    );
  if (element.kind === "ellipse")
    return (
      <ellipse
        cx={element.x + element.w / 2}
        cy={element.y + element.h / 2}
        rx={element.w / 2}
        ry={element.h / 2}
        fill={element.fill ?? "none"}
        stroke={element.stroke === null ? undefined : element.stroke}
        strokeWidth={element.strokeWidth ?? 0}
      />
    );
  if (element.kind === "line" || element.kind === "arrow")
    return (
      <g>
        <line
          x1={element.x}
          y1={element.y}
          x2={element.x + element.w}
          y2={element.y + element.h}
          stroke={element.stroke ?? "#1e293b"}
          strokeWidth={element.strokeWidth ?? 2}
        />
        {element.kind === "arrow" ? (
          <polygon
            points={`${element.x + element.w},${element.y + element.h} ${element.x + element.w - 10},${element.y + element.h - 4} ${element.x + element.w - 4},${element.y + element.h - 10}`}
            fill={element.stroke ?? "#1e293b"}
          />
        ) : null}
      </g>
    );
  if (element.kind === "text")
    return (
      <text x={element.x} y={element.y} fill={element.fill ?? "#1e293b"} fontSize={18}>
        {element.text}
      </text>
    );
  if (element.kind === "path" && element.points)
    return (
      <polyline
        points={element.points.map((point) => `${point.x},${point.y}`).join(" ")}
        fill="none"
        stroke={element.stroke ?? "#1e293b"}
        strokeWidth={element.strokeWidth ?? 2}
      />
    );
  return null;
}

function buildSvg(document: DrawDocument): string {
  const elements = document.elements
    .map((element) => {
      if (element.kind === "rect")
        return `<rect x="${element.x}" y="${element.y}" width="${element.w}" height="${element.h}" fill="${element.fill ?? "none"}" stroke="${element.stroke ?? "none"}" stroke-width="${element.strokeWidth ?? 0}" rx="6"/>`;
      if (element.kind === "ellipse")
        return `<ellipse cx="${element.x + element.w / 2}" cy="${element.y + element.h / 2}" rx="${element.w / 2}" ry="${element.h / 2}" fill="${element.fill ?? "none"}" stroke="${element.stroke ?? "none"}" stroke-width="${element.strokeWidth ?? 0}"/>`;
      if (element.kind === "line" || element.kind === "arrow")
        return `<line x1="${element.x}" y1="${element.y}" x2="${element.x + element.w}" y2="${element.y + element.h}" stroke="${element.stroke ?? "#1e293b"}" stroke-width="${element.strokeWidth ?? 2}"/>`;
      if (element.kind === "text")
        return `<text x="${element.x}" y="${element.y}" fill="${element.fill ?? "#1e293b"}" font-size="18" font-family="Segoe UI, sans-serif">${escapeXml(element.text ?? "")}</text>`;
      if (element.kind === "path" && element.points)
        return `<polyline points="${element.points.map((point) => `${point.x},${point.y}`).join(" ")}" fill="none" stroke="${element.stroke ?? "#1e293b"}" stroke-width="${element.strokeWidth ?? 2}"/>`;
      return "";
    })
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="${document.width}" height="${document.height}" viewBox="0 0 ${document.width} ${document.height}">\n<rect width="${document.width}" height="${document.height}" fill="#ffffff"/>\n${elements}\n</svg>`;
}

function escapeXml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
