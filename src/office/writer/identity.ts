/**
 * Author names for new Writer comments and tracked changes.
 *
 * The Settings "Your name" field wins when it is set; an empty setting keeps
 * the historical defaults ("Me" for comments, "You" for tracked changes) so
 * documents behave exactly as before for users who never fill it in.
 */
export function commentAuthor(settingsName: string): string {
  return settingsName.trim() || "Me";
}

export function revisionAuthor(documentAuthor: string, settingsName: string): string {
  return documentAuthor.trim() || settingsName.trim() || "You";
}
