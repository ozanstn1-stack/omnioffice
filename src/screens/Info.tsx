import { Badge, Card } from "../components/ui";
import { InfoCard } from "../components/files";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { formatBytes, formatPoints } from "../lib/format";
import { PageCanvas } from "../components/pages";
import { useState } from "react";

export function InfoScreen({ initialFiles }: { initialFiles?: string[] }) {
  const t = useT();
  const session = useTool({ suffix: "_info", accept: "pdf", initialPaths: initialFiles, dropEnabled: true });
  const [previewPage, setPreviewPage] = useState(1);

  const sizes = (() => {
    if (!session.info) return [];
    const map = new Map<string, number>();
    for (const geometry of session.info.pageGeometries) {
      const key = `${Math.round(geometry.display_width_pt)}×${Math.round(geometry.display_height_pt)}`;
      map.set(key, (map.get(key) ?? 0) + 1);
    }
    return [...map.entries()].sort((a, b) => b[1] - a[1]);
  })();

  return (
    <Screen title={t("info.title")} subtitle={session.info?.fileName ?? t("common.selectFile")}>
      <div className="two-col">
        <div className="flex flex-col gap-4">
          {session.info ? (
            <>
              <InfoCard info={session.info} />
              <Card className="p-5">
                <h3 className="font-semibold mb-3">{t("info.pageSizes")}</h3>
                <ul className="flex flex-col gap-2 text-[13px]">
                  {sizes.map(([key, count]) => {
                    const [width, height] = key.split("×").map(Number);
                    return (
                      <li key={key} className="flex items-center justify-between">
                        <span className="muted">
                          {formatPoints(width)} × {formatPoints(height)}
                        </span>
                        <Badge tone="accent">
                          {count} {t("common.pages")}
                        </Badge>
                      </li>
                    );
                  })}
                </ul>
              </Card>
              <Card className="p-4 text-xs muted">
                {t("common.original")}: {formatBytes(session.info.fileSizeBytes)} · {session.info.pageCount}{" "}
                {t("common.pages")} · {session.info.pdfVersion === "unknown" ? "PDF" : `PDF ${session.info.pdfVersion}`}
              </Card>
            </>
          ) : (
            <Card className="p-5 text-sm muted">{t("organize.empty")}</Card>
          )}
        </div>
        <div className="flex flex-col gap-4">
          {session.primary && session.info ? (
            <Card className="p-4">
              <PageCanvas
                path={session.primary.path}
                page={Math.min(previewPage, session.info.pageCount || 1)}
                password={session.password || undefined}
                maxWidth={1000}
                overlay={
                  <div
                    className="absolute bottom-2 right-2 text-[11px] px-2 py-0.5 rounded-md"
                    style={{ background: "rgb(0 0 0 / 0.5)", color: "white" }}
                  >
                    {Math.min(previewPage, session.info.pageCount || 1)} / {session.info.pageCount}
                  </div>
                }
              />
              <input
                type="range"
                className="w-full mt-3"
                style={{ accentColor: "var(--accent)" }}
                min={1}
                max={Math.max(1, session.info.pageCount)}
                value={Math.min(previewPage, session.info.pageCount || 1)}
                onChange={(event) => setPreviewPage(Number(event.target.value))}
              />
            </Card>
          ) : null}
        </div>
      </div>
    </Screen>
  );
}
