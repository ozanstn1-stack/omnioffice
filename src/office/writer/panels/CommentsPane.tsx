import { Trash2 } from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { TextDocument } from "../../../lib/office-types";
import { CommentReplyInput } from "../views";
import { PaneHead } from "./PaneHead";

type Update = (mutate: (document: TextDocument) => TextDocument) => void;

/** Writer's comments pane: resolve, delete (and unanchor) and reply. */
export function CommentsPane({
  document,
  author,
  update,
  onClose,
}: {
  document: TextDocument;
  /** The name a reply is signed with (writer/identity.ts). */
  author: string;
  update: Update;
  onClose: () => void;
}) {
  const t = useT();
  return (
    <div className="comments-sidebar">
      <PaneHead title={t("writer.comments")} onClose={onClose} />
      {document.comments.length === 0 ? <p className="muted">{t("writer.noComments")}</p> : null}
      {document.comments.map((comment) => (
        <div key={comment.id} className={`comment-card${comment.resolved ? " is-resolved" : ""}`}>
          <div className="row">
            <strong>{comment.author}</strong>
            <span className="spacer" />
            <button
              type="button"
              className="icon-btn"
              title={t("writer.resolveComment")}
              onClick={() =>
                update((doc) => ({
                  ...doc,
                  comments: doc.comments.map((entry) =>
                    entry.id === comment.id ? { ...entry, resolved: !entry.resolved } : entry,
                  ),
                }))
              }
            >
              ✓
            </button>
            <button
              type="button"
              className="icon-btn"
              title={t("common.delete")}
              onClick={() =>
                update((doc) => ({
                  ...doc,
                  comments: doc.comments.filter((entry) => entry.id !== comment.id),
                  blocks: doc.blocks.map((block) =>
                    block.type === "paragraph"
                      ? {
                          ...block,
                          runs: block.runs.map((run) => (run.comment === comment.id ? { ...run, comment: null } : run)),
                        }
                      : block,
                  ),
                }))
              }
            >
              <Trash2 size={13} />
            </button>
          </div>
          <p>{comment.text}</p>
          {(comment.replies ?? []).map((reply, replyIndex) => (
            <p key={replyIndex} className="comment-reply muted">
              <strong>{reply.author}:</strong> {reply.text}
            </p>
          ))}
          <CommentReplyInput
            placeholder={t("writer.reply")}
            onSubmit={(text) =>
              update((doc) => ({
                ...doc,
                comments: doc.comments.map((entry) =>
                  entry.id === comment.id
                    ? {
                        ...entry,
                        replies: [...(entry.replies ?? []), { author, text, created: new Date().toISOString() }],
                      }
                    : entry,
                ),
              }))
            }
          />
        </div>
      ))}
    </div>
  );
}
