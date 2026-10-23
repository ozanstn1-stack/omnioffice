import { useEffect, useState } from "react";
import { AlertTriangle, ExternalLink, FileText, Mail, Paperclip } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { pickDocumentTokens } from "../lib/file-tokens";
import { reportError } from "../lib/store";
import { openAnyFile } from "../lib/mobile";
import { openEml } from "../lib/office-api";
import type { EmlMessage } from "../lib/office-api";
import type { Navigate } from "../lib/nav";

/** Extensions that must never be launched from an e-mail attachment. */
const RISKY = /\.(exe|com|scr|bat|cmd|ps1|vbs|js|jse|jar|msi|reg|hta|lnk|pif|dll)$/i;

function isRisky(name: string) {
  return RISKY.test(name);
}

function isPdf(name: string) {
  return /\.pdf$/i.test(name);
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** A conservative HTML -> text conversion for display only (no rendering). */
function htmlToText(html: string) {
  return html
    .replace(/<(script|style)[\s\S]*?<\/\1>/gi, "")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/(p|div|tr|li|h[1-6])>/gi, "\n")
    .replace(/<[^>]+>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

/**
 * KEP `.eml` viewer: headers, body and attachments. Read-only — nothing is
 * edited or sent, remote resources in an HTML body are never loaded (the HTML
 * is shown as text), and an attachment whose extension is executable cannot be
 * launched from here.
 */
export function Eml({ initialFiles = [], onNavigate }: { initialFiles?: string[]; onNavigate: Navigate }) {
  const t = useT();
  const [messages, setMessages] = useState<{ path: string; message: EmlMessage }[]>([]);
  const [busy, setBusy] = useState(false);
  const [showHeaders, setShowHeaders] = useState<Record<string, boolean>>({});

  const initialKey = initialFiles.join("|");

  const load = async (paths: string[]) => {
    if (!paths.length) return;
    setBusy(true);
    try {
      const loaded: { path: string; message: EmlMessage }[] = [];
      for (const path of paths) {
        try {
          loaded.push({ path, message: await openEml(path) });
        } catch (error) {
          reportError(error, t);
        }
      }
      setMessages((current) => [...loaded, ...current.filter((entry) => !paths.includes(entry.path))]);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!initialKey) return;
    let cancelled = false;
    const paths = initialKey.split("|");
    void (async () => {
      const loaded: { path: string; message: EmlMessage }[] = [];
      for (const path of paths) {
        try {
          loaded.push({ path, message: await openEml(path) });
        } catch (error) {
          reportError(error, t);
        }
      }
      if (!cancelled && loaded.length) {
        setMessages((current) => [...loaded, ...current.filter((entry) => !paths.includes(entry.path))]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [initialKey, t]);

  const pick = async () => {
    const picked = await pickDocumentTokens(["eml"], true).catch(() => []);
    await load(picked.map((file) => file.token));
  };

  const openAttachment = (name: string, path?: string) => {
    if (!path || isRisky(name)) return;
    if (isPdf(name)) {
      onNavigate("reader", { files: [path] });
      return;
    }
    void openAnyFile(path).catch(() => undefined);
  };

  return (
    <Screen title={t("eml.title")} subtitle={t("eml.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Button variant="primary" disabled={busy} onClick={() => void pick()}>
              <Mail size={16} /> {t("eml.pick")}
            </Button>
            <span className="text-xs muted">{t("eml.readOnly")}</span>
          </div>
        </Card>

        {messages.length === 0 ? (
          <Card className="p-5">
            <p className="muted">{t("eml.empty")}</p>
          </Card>
        ) : null}

        {messages.map((entry) => {
          const message = entry.message;
          const body = message.bodyText.trim()
            ? message.bodyText
            : message.bodyHtml
              ? htmlToText(message.bodyHtml)
              : "";
          const showHtmlNote = !message.bodyText.trim() && Boolean(message.bodyHtml);
          return (
            <Card key={entry.path} className="p-5 flex flex-col gap-3">
              <div className="flex items-center gap-2 flex-wrap">
                <FileText size={16} style={{ color: "var(--accent)" }} />
                <strong>{message.subject || entry.path.split(/[\\/]/).pop()}</strong>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setShowHeaders((current) => ({ ...current, [entry.path]: !current[entry.path] }))}
                >
                  {t("eml.headers")}
                </Button>
              </div>
              <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))" }}>
                <div>
                  <span className="label">{t("eml.from")}</span>
                  <div>{message.from || "—"}</div>
                </div>
                <div>
                  <span className="label">{t("eml.to")}</span>
                  <div>{message.to || "—"}</div>
                </div>
                {message.cc ? (
                  <div>
                    <span className="label">{t("eml.cc")}</span>
                    <div>{message.cc}</div>
                  </div>
                ) : null}
                <div>
                  <span className="label">{t("eml.date")}</span>
                  <div>{message.date || "—"}</div>
                </div>
              </div>
              {showHeaders[entry.path] ? (
                <pre className="text-xs muted" style={{ whiteSpace: "pre-wrap", maxHeight: 240, overflow: "auto" }}>
                  {message.headers.map((header) => `${header.name}: ${header.value}`).join("\n")}
                </pre>
              ) : null}
              <div>
                <span className="label">{t("eml.body")}</span>
                <pre className="text-sm" style={{ whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
                  {body || t("eml.noBody")}
                </pre>
                {showHtmlNote ? <p className="text-xs muted">{t("eml.bodyHtmlNote")}</p> : null}
              </div>
              <div className="flex flex-col gap-2">
                <span className="label">
                  {t("eml.attachments")} ({message.attachments.length})
                </span>
                {message.attachments.length === 0 ? (
                  <p className="muted text-sm">{t("eml.noAttachments")}</p>
                ) : (
                  <ul className="flex flex-col gap-2">
                    {message.attachments.map((attachment, index) => {
                      const risky = isRisky(attachment.name);
                      return (
                        <li key={`${attachment.name}-${index}`} className="flex items-center gap-2 flex-wrap">
                          <Paperclip size={14} />
                          <span>{attachment.name}</span>
                          <Badge>{attachment.contentType || "—"}</Badge>
                          <span className="text-xs muted">{formatSize(attachment.size)}</span>
                          {risky ? (
                            <>
                              <Badge tone="danger">
                                <AlertTriangle size={12} /> {t("eml.risky")}
                              </Badge>
                              <span className="text-xs muted">{t("eml.riskyHint")}</span>
                            </>
                          ) : (
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={!attachment.path}
                              onClick={() => openAttachment(attachment.name, attachment.path)}
                            >
                              <ExternalLink size={13} /> {t("eml.open")}
                            </Button>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
              {message.warnings.map((warning) => (
                <p key={warning} className="text-xs muted">
                  {warning}
                </p>
              ))}
            </Card>
          );
        })}
      </div>
    </Screen>
  );
}
