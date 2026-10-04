import { useMemo } from "react";
import {
  BookOpen,
  Combine,
  Files,
  Hash,
  Images,
  Layers,
  Lock,
  Minimize2,
  Scissors,
  Stamp,
  Wand2,
} from "lucide-react";
import { Card } from "../components/ui";
import { DropArea, Screen } from "../components/shell";
import { usePdfSession } from "../lib/session";
import { formatBytes, isPdfName } from "../lib/format";
import type { ScreenId } from "../lib/nav";

const CARDS: { id: ScreenId; title: string; description: string; icon: React.ReactNode; accent?: boolean }[] = [
  { id: "reader", title: "Read", description: "Continuous scroll, zoom and full-text search.", icon: <BookOpen size={20} />, accent: true },
  { id: "merge", title: "Merge PDFs", description: "Combine documents into one, in your order.", icon: <Combine size={20} />, accent: true },
  { id: "organize", title: "Organize pages", description: "Reorder, rotate, duplicate and delete pages.", icon: <Layers size={20} /> },
  { id: "split", title: "Split PDF", description: "Ranges, every N pages, individual pages.", icon: <Scissors size={20} /> },
  { id: "compress", title: "Compress", description: "Lossless or strong re-render with estimates.", icon: <Minimize2 size={20} />, accent: true },
  { id: "convert", title: "Convert", description: "PDF → PNG/JPG and images → PDF.", icon: <Images size={20} />, accent: true },
  { id: "pageops", title: "Page tools", description: "Extract, delete and rotate pages quickly.", icon: <Wand2 size={20} /> },
  { id: "watermark", title: "Watermark", description: "Text stamps with position and opacity.", icon: <Stamp size={20} /> },
  { id: "numbering", title: "Page numbers", description: "Four formats, six positions, start value.", icon: <Hash size={20} /> },
  { id: "metadata", title: "Metadata", description: "Edit or clear title, author, keywords.", icon: <Files size={20} /> },
];

export function HomeScreen({ onNavigate }: { onNavigate: (screen: ScreenId, files?: File[]) => void }) {
  const session = usePdfSession({ accept: "any" });

  // Dropping on the home screen suggests the right tool based on the file type.
  const handleDrop = async (files: File[]) => {
    const first = files[0];
    if (!first) return;
    const target: ScreenId = isPdfName(first.name) ? "reader" : "convert";
    onNavigate(target, files);
  };

  const totalSize = useMemo(() => session.files.reduce((sum, file) => sum + file.size, 0), [session.files]);
  const isPdf = session.primary ? isPdfName(session.primary.name) : false;

  return (
    <Screen title="OmniOffice" subtitle="All your PDF tools in one place — offline, in your browser.">
      <div
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          const files = Array.from(event.dataTransfer?.files ?? []);
          if (files.length) void handleDrop(files);
        }}
      >
        <DropArea session={session} hint="or click to choose · PDF, JPG, PNG, WEBP" />
      </div>

      {session.files.length ? (
        <Card className="p-4 flex flex-wrap items-center gap-3 fade-in">
          <p className="text-[13.5px]">
            <strong>{session.primary?.name}</strong> · {formatBytes(totalSize)}
          </p>
          <div className="flex flex-wrap gap-2">
            {(isPdf
              ? (["reader", "organize", "merge", "split", "compress", "pageops", "watermark", "metadata", "convert"] as ScreenId[])
              : (["convert"] as ScreenId[])
            ).map((id) => (
              <button key={id} className="btn btn-sm" onClick={() => onNavigate(id)}>
                {CARDS.find((card) => card.id === id)?.title ?? id}
              </button>
            ))}
          </div>
        </Card>
      ) : null}

      <section>
        <h2 className="text-[13px] font-bold uppercase tracking-wider muted mb-3">Tools</h2>
        <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))" }}>
          {CARDS.map((card) => (
            <button key={card.id} className="tool-card" onClick={() => onNavigate(card.id)}>
              <span className="tool-icon" style={card.accent ? { background: "var(--accent)", color: "var(--accent-text)" } : undefined}>
                {card.icon}
              </span>
              <span>
                <span className="block font-semibold text-[14.5px]">{card.title}</span>
                <span className="block text-xs muted mt-1 leading-relaxed">{card.description}</span>
              </span>
            </button>
          ))}
        </div>
      </section>

      <Card soft className="p-4 flex items-start gap-3">
        <Lock size={16} style={{ color: "var(--ok)", marginTop: 2 }} />
        <div className="text-[13px]">
          <p className="font-semibold">100% local processing</p>
          <p className="text-xs muted mt-1">
            Documents are processed inside this browser tab. The extension has no network permissions, no telemetry and never
            uploads your files. The original PDFs are never modified — results are downloaded as new files.
          </p>
          <p className="text-xs muted mt-2">
            Included: reading, merge, organize, split, extract/delete/rotate, compress, watermark, page numbers, metadata and
            image conversion. OCR, password protect/unlock and page size/crop live in the Windows desktop app.
          </p>
        </div>
      </Card>
    </Screen>
  );
}
