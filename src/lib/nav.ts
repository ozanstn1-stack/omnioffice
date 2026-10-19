import type { ScreenId } from "./routes";

export type { ScreenId };

export type ViewId = ScreenId;

export interface PageToolsTab {
  tab: "extract" | "delete" | "rotate" | "resize" | "crop" | "numbering" | "stamp" | "nup";
}

export type Navigate = (
  screen: ScreenId,
  options?: { files?: string[]; pageToolsTab?: PageToolsTab["tab"]; targetPage?: number },
) => void;
