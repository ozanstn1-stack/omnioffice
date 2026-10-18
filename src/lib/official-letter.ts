/**
 * Resmî Yazışma Yönetmeliği (Resmî Yazışmalarda Uygulanacak Usul ve Esaslar
 * Hakkında Yönetmelik) Denetleyicisi (v5.3 - Issue #246).
 *
 * Kurallar:
 * 1. Sayfa Kenar Boşlukları: Standart A4 sayfa, tüm kenarlardan en az 2.5 cm (70.87 pt) boşluk (Madde 9).
 * 2. Yazı Tipi ve Boyutu: Times New Roman 12 pt veya Arial 11 pt (Madde 10).
 * 3. Satır Aralığı: 1.0 veya 1.15 satır aralığı (Madde 10).
 * 4. Başlık / Sayı / Konu Alanı: Belgenin üst kısmında Sayı ve Konu satırlarının bulunması (Madde 12-14).
 * 5. Tarih Biçimi: Gün/Ay/Yıl biçiminde (ör. 17.10.2026 veya 17 Ekim 2026) (Madde 15).
 * 6. İmza Bloğu: Adı SOYADI büyük harf, unvan ve sağa hizalı imza alanı (Madde 20).
 * 7. Paragraf Girintisi ve Hizalama: İki yana yaslı (justify) veya sola dayalı, 1.25 cm (35.43 pt) ilk satır girintisi (Madde 17).
 */

import type { TextDocument } from "./office-types";

export type RuleSeverity = "error" | "warning" | "info";

export interface LetterViolation {
  id: string;
  ruleId: string;
  ruleTitle: string;
  regulationArticle: string;
  severity: RuleSeverity;
  message: string;
  blockIndex?: number;
  canAutoFix: boolean;
  manualInstructions?: string;
}

export const REGULATION_RULES = {
  MARGINS: {
    id: "REG-01",
    title: "Sayfa Kenar Boşlukları",
    article: "Madde 9",
    minPt: 70.8, // 2.5 cm = ~70.87 pt
  },
  FONT: {
    id: "REG-02",
    title: "Yazı Tipi ve Boyutu",
    article: "Madde 10",
    allowedFonts: ["Times New Roman", "Arial"],
    timesMinPt: 12,
    arialMinPt: 11,
  },
  LINE_SPACING: {
    id: "REG-03",
    title: "Satır Aralığı",
    article: "Madde 10",
    maxMultiple: 1.5,
  },
  NUMBER_SUBJECT: {
    id: "REG-04",
    title: "Sayı ve Konu Satırları",
    article: "Madde 12-14",
  },
  DATE_FORMAT: {
    id: "REG-05",
    title: "Tarih Formatı",
    article: "Madde 15",
  },
  ALIGNMENT: {
    id: "REG-06",
    title: "Metin Hizalama ve Paragraf",
    article: "Madde 17",
  },
  SIGNATURE_BLOCK: {
    id: "REG-07",
    title: "İmza Bloğu",
    article: "Madde 20",
  },
} as const;

/**
 * Checks a TextDocument for Official Letter Regulation compliance.
 */
export function checkOfficialLetter(doc: TextDocument): LetterViolation[] {
  const violations: LetterViolation[] = [];

  // 1. Margin check
  const page = doc.page;
  if (page) {
    const min = REGULATION_RULES.MARGINS.minPt;
    if (page.marginTopPt < min || page.marginBottomPt < min || page.marginLeftPt < min || page.marginRightPt < min) {
      violations.push({
        id: "margin-violation",
        ruleId: REGULATION_RULES.MARGINS.id,
        ruleTitle: REGULATION_RULES.MARGINS.title,
        regulationArticle: REGULATION_RULES.MARGINS.article,
        severity: "error",
        message:
          "Sayfa kenar boşlukları en az 2.5 cm (yaklaşık 71 pt) olmalıdır. (Şu anki: Üst=" +
          Math.round(page.marginTopPt) +
          "pt, Alt=" +
          Math.round(page.marginBottomPt) +
          "pt, Sol=" +
          Math.round(page.marginLeftPt) +
          "pt, Sağ=" +
          Math.round(page.marginRightPt) +
          "pt)",
        canAutoFix: true,
      });
    }
  }

  let hasNumber = false;
  let hasSubject = false;
  let hasSignature = false;
  let hasDate = false;

  doc.blocks.forEach((block, idx) => {
    if (block.type === "paragraph") {
      const fullText = (block.runs || []).map((r) => r.text || "").join("");
      const trimmed = fullText.trim();

      // Number check
      if (/^Sayı\s*:/i.test(trimmed)) {
        hasNumber = true;
      }

      // Subject check
      if (/^Konu\s*:/i.test(trimmed)) {
        hasSubject = true;
      }

      // Date check (e.g. 17/10/2026, 17.10.2026, 17 Ekim 2026)
      if (
        /\b\d{1,2}[./]\d{1,2}[./]\d{4}\b/.test(trimmed) ||
        /\b\d{1,2}\s+(Ocak|Şubat|Mart|Nisan|Mayıs|Haziran|Temmuz|Ağustos|Eylül|Ekim|Kasım|Aralık)\s+\d{4}\b/i.test(
          trimmed,
        )
      ) {
        hasDate = true;
      }

      // Font & size check across runs
      for (const run of block.runs || []) {
        if (!run.text || !run.text.trim()) continue;

        if (run.font && run.font !== "Times New Roman" && run.font !== "Arial") {
          violations.push({
            id: `font-family-${idx}`,
            ruleId: REGULATION_RULES.FONT.id,
            ruleTitle: REGULATION_RULES.FONT.title,
            regulationArticle: REGULATION_RULES.FONT.article,
            severity: "warning",
            message: `Uygun olmayan yazı tipi: '${run.font}'. Yönetmeliğe göre Times New Roman (12 pt) veya Arial (11 pt) kullanılmalıdır.`,
            blockIndex: idx,
            canAutoFix: true,
          });
          break;
        }

        if (
          run.sizePt &&
          (run.sizePt < 11 ||
            run.sizePt > 12 ||
            (run.font === "Times New Roman" && run.sizePt !== 12) ||
            (run.font === "Arial" && run.sizePt !== 11))
        ) {
          violations.push({
            id: `font-size-${idx}`,
            ruleId: REGULATION_RULES.FONT.id,
            ruleTitle: REGULATION_RULES.FONT.title,
            regulationArticle: REGULATION_RULES.FONT.article,
            severity: "warning",
            message: `Yazı boyutu (${run.sizePt} pt) standartların dışında. Times New Roman için 12 pt, Arial için 11 pt kullanılmalıdır.`,
            blockIndex: idx,
            canAutoFix: true,
          });
          break;
        }
      }

      // Line spacing check (Madde 10)
      if (
        trimmed.length > 0 &&
        block.props &&
        (block.props.lineSpacing > REGULATION_RULES.LINE_SPACING.maxMultiple || block.props.lineSpacing < 1.0)
      ) {
        violations.push({
          id: `line-spacing-${idx}`,
          ruleId: REGULATION_RULES.LINE_SPACING.id,
          ruleTitle: REGULATION_RULES.LINE_SPACING.title,
          regulationArticle: REGULATION_RULES.LINE_SPACING.article,
          severity: "warning",
          message: `Satır aralığı (${block.props.lineSpacing}) yönetmeliğe uygun değil. 1.0 veya 1.15 (en fazla 1.5) olmalıdır.`,
          blockIndex: idx,
          canAutoFix: true,
        });
      }

      // Check alignment and first-line indent (body paragraphs should typically be justify or left with 1.25 cm indent)
      if (block.props && trimmed.length > 80 && !/^(Sayı|Konu|İlgi)\s*:/i.test(trimmed)) {
        if (block.props.align === "center") {
          violations.push({
            id: `align-${idx}`,
            ruleId: REGULATION_RULES.ALIGNMENT.id,
            ruleTitle: REGULATION_RULES.ALIGNMENT.title,
            regulationArticle: REGULATION_RULES.ALIGNMENT.article,
            severity: "info",
            message: "Uzun gövde paragrafları iki yana yaslı (justify) veya sola dayalı olmalıdır.",
            blockIndex: idx,
            canAutoFix: true,
          });
        }
        if ((block.props.firstLinePt ?? 0) < 28) {
          violations.push({
            id: `indent-${idx}`,
            ruleId: REGULATION_RULES.ALIGNMENT.id,
            ruleTitle: REGULATION_RULES.ALIGNMENT.title,
            regulationArticle: REGULATION_RULES.ALIGNMENT.article,
            severity: "info",
            message: "Gövde paragraflarında 1.25 cm (~35.4 pt) ilk satır girintisi bulunmalıdır.",
            blockIndex: idx,
            canAutoFix: true,
          });
        }
      }

      // Detect potential signature pattern (Name SURNAME in capital)
      if (/^[A-ZÇĞİÖŞÜa-zçğıöşü\s]+\s+[A-ZÇĞİÖŞÜ]{2,}$/.test(trimmed) && trimmed.length < 50) {
        hasSignature = true;
      }
    }
  });

  if (!hasSignature && doc.blocks.length > 2) {
    violations.push({
      id: "missing-signature",
      ruleId: REGULATION_RULES.SIGNATURE_BLOCK.id,
      ruleTitle: REGULATION_RULES.SIGNATURE_BLOCK.title,
      regulationArticle: REGULATION_RULES.SIGNATURE_BLOCK.article,
      severity: "info",
      message: "Belge sonunda imza bloğu tespit edilemedi.",
      canAutoFix: false,
      manualInstructions: "Belgenin sonuna Ad SOYAD ve unvan içeren imza bloğu ekleyin.",
    });
  }

  if (!hasNumber) {
    violations.push({
      id: "missing-number",
      ruleId: REGULATION_RULES.NUMBER_SUBJECT.id,
      ruleTitle: REGULATION_RULES.NUMBER_SUBJECT.title,
      regulationArticle: REGULATION_RULES.NUMBER_SUBJECT.article,
      severity: "warning",
      message: "Belgede 'Sayı:' satırı bulunamadı.",
      canAutoFix: false,
      manualInstructions: "Belgenin sol üst alanına 'Sayı: ' alanını ekleyin.",
    });
  }

  if (!hasSubject) {
    violations.push({
      id: "missing-subject",
      ruleId: REGULATION_RULES.NUMBER_SUBJECT.id,
      ruleTitle: REGULATION_RULES.NUMBER_SUBJECT.title,
      regulationArticle: REGULATION_RULES.NUMBER_SUBJECT.article,
      severity: "warning",
      message: "Belgede 'Konu:' satırı bulunamadı.",
      canAutoFix: false,
      manualInstructions: "Belgenin sol üst alanına 'Konu: ' alanını ekleyin.",
    });
  }

  if (!hasDate) {
    violations.push({
      id: "missing-date",
      ruleId: REGULATION_RULES.DATE_FORMAT.id,
      ruleTitle: REGULATION_RULES.DATE_FORMAT.title,
      regulationArticle: REGULATION_RULES.DATE_FORMAT.article,
      severity: "info",
      message: "Standart formatta güncel tarih satırı saptanamadı.",
      canAutoFix: false,
      manualInstructions: "Sağ üst köşeye günün tarihini (ör. GG.AA.YYYY) ekleyin.",
    });
  }

  return violations;
}

/**
 * 1-click auto-fixer for supported violations.
 */
export function fixOfficialLetterViolation(doc: TextDocument, violationId: string): TextDocument {
  const updated = JSON.parse(JSON.stringify(doc)) as TextDocument;

  if (violationId === "margin-violation" && updated.page) {
    const min = REGULATION_RULES.MARGINS.minPt;
    updated.page.marginTopPt = Math.max(updated.page.marginTopPt, min);
    updated.page.marginBottomPt = Math.max(updated.page.marginBottomPt, min);
    updated.page.marginLeftPt = Math.max(updated.page.marginLeftPt, min);
    updated.page.marginRightPt = Math.max(updated.page.marginRightPt, min);
    return updated;
  }

  if (violationId.startsWith("font-family-")) {
    const idx = parseInt(violationId.replace("font-family-", ""), 10);
    const block = updated.blocks[idx];
    if (block && block.type === "paragraph") {
      for (const run of block.runs || []) {
        run.font = "Times New Roman";
      }
    }
    return updated;
  }

  if (violationId.startsWith("font-size-")) {
    const idx = parseInt(violationId.replace("font-size-", ""), 10);
    const block = updated.blocks[idx];
    if (block && block.type === "paragraph") {
      for (const run of block.runs || []) {
        run.sizePt = run.font === "Arial" ? 11 : 12;
      }
    }
    return updated;
  }

  if (violationId.startsWith("line-spacing-")) {
    const idx = parseInt(violationId.replace("line-spacing-", ""), 10);
    const block = updated.blocks[idx];
    if (block && block.type === "paragraph" && block.props) {
      block.props.lineSpacing = 1.15;
    }
    return updated;
  }

  if (violationId.startsWith("align-")) {
    const idx = parseInt(violationId.replace("align-", ""), 10);
    const block = updated.blocks[idx];
    if (block && block.type === "paragraph" && block.props) {
      block.props.align = "justify";
    }
    return updated;
  }

  if (violationId.startsWith("indent-")) {
    const idx = parseInt(violationId.replace("indent-", ""), 10);
    const block = updated.blocks[idx];
    if (block && block.type === "paragraph" && block.props) {
      block.props.firstLinePt = 35.4;
    }
    return updated;
  }

  return updated;
}
