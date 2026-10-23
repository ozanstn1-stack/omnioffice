/**
 * Routing for token-based documents. The token is opaque; the display name
 * decides the destination, and the screen receives the token only.
 */
import { flagEnabled } from "./flags";
import { routeForPath } from "./open-route";
import type { FileToken } from "./file-tokens";
import type { ScreenId } from "./nav";

export interface TokenDocumentHandlers {
  showClosedUdf: () => void;
  openOffice: (token: string) => void;
  openScreen: (screen: ScreenId, files: string[]) => void;
  openExternal: (token: string) => void;
}

export function openTokenDocument(ref: FileToken, handlers: TokenDocumentHandlers): void {
  if (ref.name.toLowerCase().endsWith(".udf") && !flagEnabled("experimentalUdf")) {
    handlers.showClosedUdf();
    return;
  }
  const route = routeForPath(ref.name);
  if (!route) {
    handlers.openExternal(ref.token);
    return;
  }
  if (route.office) {
    handlers.openOffice(ref.token);
    return;
  }
  handlers.openScreen(route.screen, [ref.token]);
}
