import type { ScreenId } from "./routes";
import type { TaskPackId } from "./ai-task-packs";

export type { ScreenId };

export type ViewId = ScreenId;

export interface PageToolsTab {
  tab: "extract" | "delete" | "rotate" | "resize" | "crop" | "numbering" | "stamp" | "nup";
}

export type Navigate = (
  screen: ScreenId,
  options?: { files?: string[]; pageToolsTab?: PageToolsTab["tab"]; targetPage?: number; pack?: TaskPackId },
) => void;
