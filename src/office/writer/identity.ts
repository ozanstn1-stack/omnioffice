/**
 * Author names for new Writer comments and tracked changes.
 *
 * The Settings "Your name" field wins when it is set; an empty setting keeps
 * the historical defaults ("Me" for comments, "You" for tracked changes) so
 * documents behave exactly as before for users who never fill it in. For a
 * tracked change the configured name comes before the document's own author:
 * the person making the change is the user, not whoever wrote the file.
 */
export function commentAuthor(settingsName: string): string {
  return settingsName.trim() || "Me";
}

export function revisionAuthor(documentAuthor: string, settingsName: string): string {
  return settingsName.trim() || documentAuthor.trim() || "You";
}
