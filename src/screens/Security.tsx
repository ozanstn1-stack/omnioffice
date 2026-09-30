import { useState } from "react";
import { Eye, EyeOff, KeyRound, Lock, LockOpen, ShieldCheck } from "lucide-react";
import { Badge, Card, Field, Segmented, Toggle } from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { protectPdf, unlockPdf } from "../lib/api";
import { useToasts } from "../lib/store";

type Tab = "protect" | "unlock";

export function Security({ tab: initialTab, initialFiles, dragging }: { tab: Tab; initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>(initialTab);
  return (
    <Screen
      title={tab === "protect" ? t("security.protectTitle") : t("security.unlockTitle")}
      subtitle={tab === "protect" ? t("security.protectSubtitle") : t("security.unlockSubtitle")}
      actions={
        <Segmented<Tab>
          value={tab}
          onChange={setTab}
          options={[
            { value: "protect", label: t("nav.protect") },
            { value: "unlock", label: t("nav.unlock") },
          ]}
        />
      }
    >
      {tab === "protect" ? <Protect initialFiles={initialFiles} dragging={dragging} /> : <Unlock initialFiles={initialFiles} dragging={dragging} />}
    </Screen>
  );
}

function PasswordField({
  value,
  onChange,
  placeholder,
  autoFocus = false,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
}) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="relative">
      <input
        className="input pr-10"
        type={visible ? "text" : "password"}
        value={value}
        // eslint-disable-next-line jsx-a11y/no-autofocus -- the password field is the dialog's only target; focusing it is the expected flow
        autoFocus={autoFocus}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
        autoComplete="new-password"
        spellCheck={false}
      />
      <button
        type="button"
        className="absolute right-1 top-1/2 -translate-y-1/2 icon-btn"
        onClick={() => setVisible((previous) => !previous)}
        aria-label={visible ? "Hide" : "Show"}
        tabIndex={-1}
      >
        {visible ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
    </div>
  );
}

function Protect({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_protected", accept: "pdf", initialPaths: initialFiles });
  const pushToast = useToasts((s) => s.push);
  const [userPassword, setUserPassword] = useState("");
  const [ownerPassword, setOwnerPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [allowPrinting, setAllowPrinting] = useState(true);
  const [allowCopying, setAllowCopying] = useState(true);
  const [allowEditing, setAllowEditing] = useState(true);
  const [allowCommenting, setAllowCommenting] = useState(true);

  const run = () => {
    if (!userPassword && !ownerPassword) {
      pushToast({ kind: "error", title: t("errors.title"), detail: t("security.passwordRequired") });
      return;
    }
    if (userPassword !== confirm) {
      pushToast({ kind: "error", title: t("errors.title"), detail: t("security.passwordMismatch") });
      return;
    }
    void session.run(async (jobId, overwrite) => {
      const result = await protectPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        {
          userPassword,
          ownerPassword: ownerPassword || userPassword,
          allowPrinting,
          allowCopying,
          allowEditing,
          allowCommenting,
        } as never,
        jobId,
        session.password || undefined,
      );
      // never keep passwords in component state after a successful run
      setUserPassword("");
      setOwnerPassword("");
      setConfirm("");
      return result;
    });
  };

  return (
    <TwoColumn
      main={
        !session.primary ? (
          <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
        ) : (
          <>
            <OptionCard>
              <FileList files={session.files} onRemove={session.removeFile} onAdd={session.pickFiles} addLabel={t("common.addPdf")} />
            </OptionCard>
            {session.info ? (
              <Card className="p-4">
                <InfoStrip info={session.info} error={session.infoError} />
                {session.info.encrypted ? (
                  <p className="text-xs mt-2" style={{ color: "var(--warn)" }}>
                    {t("errors.password_required")}
                  </p>
                ) : null}
              </Card>
            ) : null}
            <OptionCard title={t("security.permissions")}>
              <Toggle checked={allowPrinting} onChange={setAllowPrinting} label={t("security.allowPrinting")} />
              <Toggle checked={allowCopying} onChange={setAllowCopying} label={t("security.allowCopying")} />
              <Toggle checked={allowEditing} onChange={setAllowEditing} label={t("security.allowEditing")} />
              <Toggle checked={allowCommenting} onChange={setAllowCommenting} label={t("security.allowCommenting")} />
            </OptionCard>
          </>
        )
      }
      side={
        <>
          <OutputBar session={session} runLabel={t("security.run")} onRun={run} disabled={!session.primary} />
          <OptionCard title={t("nav.protect")}>
            <Field label={t("security.userPassword")}>
              <PasswordField value={userPassword} onChange={setUserPassword} placeholder="••••••••" />
            </Field>
            <Field label={t("security.ownerPassword")} hint={t("common.optional")}>
              <PasswordField value={ownerPassword} onChange={setOwnerPassword} placeholder={t("security.ownerPassword")} />
            </Field>
            <Field label={t("security.confirmPassword")}>
              <PasswordField value={confirm} onChange={setConfirm} placeholder={t("security.confirmPassword")} />
            </Field>
            {confirm && userPassword !== confirm ? (
              <p className="text-xs" style={{ color: "var(--danger)" }}>
                {t("security.passwordMismatch")}
              </p>
            ) : null}
            <Badge tone="ok">
              <ShieldCheck size={12} /> AES-256
            </Badge>
            <p className="text-xs muted flex items-start gap-1.5">
              <Lock size={12} style={{ marginTop: 2 }} /> {t("security.note")}
            </p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}

function Unlock({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_unlocked", accept: "pdf", initialPaths: initialFiles });
  const [password, setPassword] = useState("");

  const run = () =>
    session.run(async (jobId, overwrite) => {
      if (!password) throw { code: "invalid_input", message: t("security.passwordRequired") };
      const result = await unlockPdf(session.primary?.path ?? "", session.outputSpec(overwrite), password, jobId);
      setPassword("");
      return result;
    });

  return (
    <TwoColumn
      main={
        !session.primary ? (
          <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
        ) : (
          <>
            <OptionCard>
              <FileList files={session.files} onRemove={session.removeFile} onAdd={session.pickFiles} addLabel={t("common.addPdf")} />
            </OptionCard>
            {session.info ? (
              <Card className="p-4">
                <InfoStrip info={session.info} error={session.infoError} />
              </Card>
            ) : null}
            <Card soft className="p-4 text-xs muted flex items-start gap-2">
              <KeyRound size={14} style={{ marginTop: 2 }} />
              <span>{t("security.unlockNote")}</span>
            </Card>
          </>
        )
      }
      side={
        <>
          <OutputBar session={session} runLabel={t("security.unlockRun")} onRun={run} disabled={!session.primary || !password} />
          <OptionCard title={t("nav.unlock")}>
            <Field label={t("security.unlockPassword")}>
              {/* eslint-disable-next-line jsx-a11y/no-autofocus -- the unlock password is the only field in this panel; focusing it is expected */}
              <PasswordField value={password} onChange={setPassword} placeholder="••••••••" autoFocus />
            </Field>
            <p className="text-xs muted flex items-start gap-1.5">
              <LockOpen size={12} style={{ marginTop: 2 }} /> {t("security.unlockNote")}
            </p>
          </OptionCard>
          {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
        </>
      }
    />
  );
}
