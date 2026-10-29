import { useState, type ReactNode } from "react";
import { FileDown, FileText, ShieldCheck } from "lucide-react";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { Badge, Button, Card, TextArea, TextInput } from "../components/ui";
import { Screen } from "../components/layout";
import { pickSaveToken } from "../lib/file-tokens";
import { uid } from "../lib/format";
import { useT } from "../lib/i18n";
import { isAndroid, pickAndroidSaveTarget, publishOutputs } from "../lib/mobile";
import type { ViewId } from "../lib/nav";
import { exportPdfa, saveDocument } from "../lib/office-api";
import { useOfficeTabs } from "../lib/office-store";
import { checkOfficialLetter } from "../lib/official-letter";
import {
  buildOfficialLetter,
  commitSequence,
  formatLetterNumber,
  nextSequence,
  sequenceKey,
  type OfficialLetterContact,
} from "../lib/official-letter-template";
import { reportError, useToasts } from "../lib/store";
import { call } from "../lib/typed-call";

/** What stays from one letter to the next: the institution, signer and contact. */
interface LetterProfile {
  institution: string;
  detsis: string;
  filePlanCode: string;
  electronic: boolean;
  signerName: string;
  signerTitle: string;
  contact: OfficialLetterContact;
}

const PROFILE_KEY = "omni.officialLetter.profile";

const EMPTY_PROFILE: LetterProfile = {
  institution: "",
  detsis: "",
  filePlanCode: "",
  electronic: true,
  signerName: "",
  signerTitle: "",
  contact: { address: "", phone: "", email: "", kep: "", web: "", contactPerson: "" },
};

function loadProfile(): LetterProfile {
  try {
    const stored = JSON.parse(localStorage.getItem(PROFILE_KEY) ?? "null") as Partial<LetterProfile> | null;
    return stored
      ? { ...EMPTY_PROFILE, ...stored, contact: { ...EMPTY_PROFILE.contact, ...stored.contact } }
      : EMPTY_PROFILE;
  } catch {
    return EMPTY_PROFILE;
  }
}

function saveProfile(profile: LetterProfile) {
  try {
    localStorage.setItem(PROFILE_KEY, JSON.stringify(profile));
  } catch {
    // Storage can be unavailable (private mode); the form still works.
  }
}

const lines = (value: string) => value.split("\n");
const today = () => new Date().toISOString().slice(0, 10);
/** YYYY-MM-DD (the date input) to GG.AA.YYYY (the letter). */
const letterDate = (iso: string) => {
  const [year, month, day] = iso.split("-");
  return year && month && day ? `${day}.${month}.${year}` : iso;
};

function Labelled({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <label className="label" htmlFor={id}>
        {label}
      </label>
      {children}
    </div>
  );
}

export function OfficialLetterWizard({ onNavigate }: { onNavigate?: (view: ViewId) => void }) {
  const t = useT();
  const [profile, setProfileState] = useState<LetterProfile>(loadProfile);
  const [date, setDate] = useState(today);
  const [subject, setSubject] = useState("");
  const [addressee, setAddressee] = useState("");
  const [addresseePlace, setAddresseePlace] = useState("");
  const [references, setReferences] = useState("");
  const [body, setBody] = useState("");
  const [closing, setClosing] = useState("Bilgilerinizi ve gereğini rica ederim.");
  const [eSigned, setESigned] = useState(true);
  const [attachments, setAttachments] = useState("");
  const [distributionAction, setDistributionAction] = useState("");
  const [distributionInfo, setDistributionInfo] = useState("");
  const [sequenceOverride, setSequenceOverride] = useState<string | null>(null);
  const [, setUsed] = useState(0);
  const [busy, setBusy] = useState(false);

  const setProfile = (patch: Partial<LetterProfile>) =>
    setProfileState((current) => {
      const next = { ...current, ...patch };
      saveProfile(next);
      return next;
    });
  const setContact = (patch: Partial<OfficialLetterContact>) =>
    setProfile({ contact: { ...profile.contact, ...patch } });

  const year = Number(date.slice(0, 4)) || new Date().getFullYear();
  const key = sequenceKey(year, profile.detsis, profile.filePlanCode);
  // Read on every render: `setUsed` re-renders after a letter used its number.
  const sequence = sequenceOverride ?? String(nextSequence(key));

  const fields = {
    institution: lines(profile.institution),
    electronic: profile.electronic,
    detsis: profile.detsis,
    filePlanCode: profile.filePlanCode,
    sequence,
    date: letterDate(date),
    subject,
    addressee,
    addresseePlace,
    references: lines(references),
    body,
    closing,
    signerName: profile.signerName,
    signerTitle: profile.signerTitle,
    eSigned,
    attachments: lines(attachments),
    distributionAction: lines(distributionAction),
    distributionInfo: lines(distributionInfo),
    contact: profile.contact,
  };
  const doc = buildOfficialLetter(fields);
  const issues = checkOfficialLetter(doc);

  const toast = (kind: "success" | "error" | "info", title: string, detail?: string) =>
    useToasts.getState().push({ kind, title, ...(detail ? { detail } : {}) });

  const ready = () => {
    if (subject.trim() && addressee.trim() && body.trim() && profile.signerName.trim()) return true;
    toast("info", t("officialLetter.missing"));
    return false;
  };

  /** The number is used up once the letter leaves the wizard. */
  const spendNumber = () => {
    const value = Number(sequence);
    if (Number.isFinite(value) && value > 0) commitSequence(key, value);
    setSequenceOverride(null);
    setUsed((count) => count + 1);
  };

  const openInWriter = () => {
    if (!ready()) return;
    useOfficeTabs.getState().create("writer", subject.trim(), doc);
    spendNumber();
    onNavigate?.("office");
  };

  const save = async (format: "docx" | "pdf") => {
    if (!ready()) return;
    const name = `${subject.trim() || "resmi-yazi"}.${format}`;
    const write = async (path: string) => {
      if (format === "docx") {
        await saveDocument("writer", doc, path);
        toast("success", t("officialLetter.saved", { name }));
        return;
      }
      const report = await exportPdfa("writer", doc, path, "2b");
      if (report.valid) toast("success", t("officialLetter.pdfaValid"));
      else toast("error", t("officialLetter.pdfaInvalid", { n: report.failures }));
    };
    setBusy(true);
    try {
      if (isAndroid()) {
        const target = await pickAndroidSaveTarget(name, format);
        if (!target) return;
        const dir = await join(await appCacheDir(), "official-letter");
        await call((c) => c.ensureDir(dir));
        const temp = await join(dir, `${uid("letter")}.${format}`);
        await write(temp);
        const failures = await publishOutputs([temp], { file: target });
        if (failures.length) {
          toast("error", t("errors.title"), failures[0].error);
          return;
        }
      } else {
        const target = await pickSaveToken(name, [format]).catch(() => null);
        if (!target) return;
        await write(target.token);
      }
      spendNumber();
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const contactFields: [keyof OfficialLetterContact, string][] = [
    ["address", t("officialLetter.address")],
    ["phone", t("officialLetter.phone")],
    ["email", t("officialLetter.email")],
    ["kep", t("officialLetter.kep")],
    ["web", t("officialLetter.web")],
    ["contactPerson", t("officialLetter.contactPerson")],
  ];

  return (
    <Screen title={t("officialLetter.title")} subtitle={t("officialLetter.subtitle")}>
      <div className="flex flex-col gap-4 max-w-4xl w-full mx-auto">
        <Card className="p-5 flex flex-col gap-4">
          <h3 className="text-sm font-semibold">{t("officialLetter.sectionInstitution")}</h3>
          <Labelled id="letter-institution" label={t("officialLetter.institution")}>
            <TextArea
              id="letter-institution"
              rows={2}
              value={profile.institution}
              onChange={(event) => setProfile({ institution: event.target.value })}
            />
          </Labelled>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <Labelled id="letter-detsis" label={t("officialLetter.detsis")}>
              <TextInput
                id="letter-detsis"
                inputMode="numeric"
                value={profile.detsis}
                onChange={(event) => setProfile({ detsis: event.target.value })}
              />
            </Labelled>
            <Labelled id="letter-plan" label={t("officialLetter.filePlanCode")}>
              <TextInput
                id="letter-plan"
                value={profile.filePlanCode}
                onChange={(event) => setProfile({ filePlanCode: event.target.value })}
              />
            </Labelled>
            <Labelled id="letter-sequence" label={t("officialLetter.sequence")}>
              <TextInput
                id="letter-sequence"
                inputMode="numeric"
                value={sequence}
                onChange={(event) => setSequenceOverride(event.target.value)}
              />
            </Labelled>
            <Labelled id="letter-date" label={t("officialLetter.date")}>
              <TextInput id="letter-date" type="date" value={date} onChange={(event) => setDate(event.target.value)} />
            </Labelled>
          </div>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={profile.electronic}
              onChange={(event) => setProfile({ electronic: event.target.checked })}
            />
            <span>{t("officialLetter.electronic")}</span>
          </label>
          <p className="text-sm font-mono break-all">
            {t("officialLetter.number", { number: formatLetterNumber(fields) })}
          </p>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="text-sm font-semibold">{t("officialLetter.sectionLetter")}</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <Labelled id="letter-subject" label={t("officialLetter.subject")}>
              <TextInput id="letter-subject" value={subject} onChange={(event) => setSubject(event.target.value)} />
            </Labelled>
            <Labelled id="letter-addressee" label={t("officialLetter.addressee")}>
              <TextInput
                id="letter-addressee"
                value={addressee}
                onChange={(event) => setAddressee(event.target.value)}
              />
            </Labelled>
            <Labelled id="letter-place" label={t("officialLetter.addresseePlace")}>
              <TextInput
                id="letter-place"
                value={addresseePlace}
                onChange={(event) => setAddresseePlace(event.target.value)}
              />
            </Labelled>
          </div>
          <Labelled id="letter-references" label={t("officialLetter.references")}>
            <TextArea
              id="letter-references"
              rows={2}
              value={references}
              onChange={(event) => setReferences(event.target.value)}
            />
          </Labelled>
          <Labelled id="letter-body" label={t("officialLetter.body")}>
            <TextArea id="letter-body" rows={7} value={body} onChange={(event) => setBody(event.target.value)} />
          </Labelled>
          <Labelled id="letter-closing" label={t("officialLetter.closing")}>
            <TextInput id="letter-closing" value={closing} onChange={(event) => setClosing(event.target.value)} />
          </Labelled>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="text-sm font-semibold">{t("officialLetter.sectionSignature")}</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <Labelled id="letter-signer" label={t("officialLetter.signerName")}>
              <TextInput
                id="letter-signer"
                value={profile.signerName}
                onChange={(event) => setProfile({ signerName: event.target.value })}
              />
            </Labelled>
            <Labelled id="letter-title" label={t("officialLetter.signerTitle")}>
              <TextInput
                id="letter-title"
                value={profile.signerTitle}
                onChange={(event) => setProfile({ signerTitle: event.target.value })}
              />
            </Labelled>
          </div>
          <label className="checkbox">
            <input type="checkbox" checked={eSigned} onChange={(event) => setESigned(event.target.checked)} />
            <span>{t("officialLetter.eSigned")}</span>
          </label>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="text-sm font-semibold">{t("officialLetter.sectionAttachments")}</h3>
          <Labelled id="letter-attachments" label={t("officialLetter.attachments")}>
            <TextArea
              id="letter-attachments"
              rows={2}
              value={attachments}
              onChange={(event) => setAttachments(event.target.value)}
            />
          </Labelled>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <Labelled id="letter-action" label={t("officialLetter.distributionAction")}>
              <TextArea
                id="letter-action"
                rows={2}
                value={distributionAction}
                onChange={(event) => setDistributionAction(event.target.value)}
              />
            </Labelled>
            <Labelled id="letter-info" label={t("officialLetter.distributionInfo")}>
              <TextArea
                id="letter-info"
                rows={2}
                value={distributionInfo}
                onChange={(event) => setDistributionInfo(event.target.value)}
              />
            </Labelled>
          </div>
        </Card>

        <Card className="p-5 flex flex-col gap-4">
          <h3 className="text-sm font-semibold">{t("officialLetter.sectionContact")}</h3>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            {contactFields.map(([field, label]) => (
              <Labelled key={field} id={`letter-contact-${field}`} label={label}>
                <TextInput
                  id={`letter-contact-${field}`}
                  value={profile.contact[field]}
                  onChange={(event) => setContact({ [field]: event.target.value })}
                />
              </Labelled>
            ))}
          </div>
        </Card>

        <Card className="p-5 flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <ShieldCheck size={16} />
            <span className="text-sm font-semibold">{t("officialLetter.check")}</span>
            {issues.length === 0 ? (
              <Badge tone="ok">{t("officialLetter.checkClean")}</Badge>
            ) : (
              <Badge tone="warn">{t("officialLetter.checkIssues", { n: issues.length })}</Badge>
            )}
          </div>
          {issues.length ? (
            <ul className="text-xs muted flex flex-col gap-1">
              {issues.map((issue) => (
                <li key={issue.id}>
                  {issue.regulationArticle}: {issue.message}
                </li>
              ))}
            </ul>
          ) : null}
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" icon={<FileText size={14} />} disabled={busy} onClick={openInWriter}>
              {t("officialLetter.openInWriter")}
            </Button>
            <Button icon={<FileDown size={14} />} disabled={busy} onClick={() => void save("pdf")}>
              {t("officialLetter.savePdfa")}
            </Button>
            <Button icon={<FileDown size={14} />} disabled={busy} onClick={() => void save("docx")}>
              {t("officialLetter.saveDocx")}
            </Button>
          </div>
        </Card>
      </div>
    </Screen>
  );
}
