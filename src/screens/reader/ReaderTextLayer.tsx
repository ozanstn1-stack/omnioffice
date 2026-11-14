import { memo, useMemo } from "react";
import type { TextRunInfo } from "../../lib/api";
import type { PageGeometry } from "../../lib/types";
import { textLayerItems } from "../../lib/reader-text";

export const ReaderTextLayer = memo(function ReaderTextLayer({
  runs,
  geometry,
  width,
}: {
  runs: readonly TextRunInfo[];
  geometry: PageGeometry;
  width: number;
}) {
  const items = useMemo(() => textLayerItems(runs, geometry, width), [runs, geometry, width]);
  if (items.length === 0) return null;
  return (
    <div
      className="absolute inset-0 overflow-hidden"
      style={{ lineHeight: 1, userSelect: "text", WebkitUserSelect: "text" }}
    >
      {items.map((item, index) => (
        <span
          key={index}
          style={{
            position: "absolute",
            left: item.left,
            top: item.top,
            width: item.width,
            height: item.height,
            fontSize: item.fontSize,
            lineHeight: `${item.height}px`,
            color: "transparent",
            whiteSpace: "pre",
            cursor: "text",
          }}
        >
          {item.text}
        </span>
      ))}
    </div>
  );
});
