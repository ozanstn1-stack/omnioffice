/**
 * Turkey-specific Calc functions.
 *
 * `YAZIYLA` writes an amount in Turkish words (cheque and invoice practice),
 * and the three verifiers check Turkish identity, tax and IBAN numbers with
 * their official check algorithms. None of these exist in Excel or
 * OpenFormula, so a saved workbook keeps the cached value and the compatibility
 * report says so (see `officecore::compat`).
 */
import { registerFunction } from "../registry";
import { ERR, isError, toNumber, toText, type Scalar } from "../scalars";

const ONES = ["", "bir", "iki", "üç", "dört", "beş", "altı", "yedi", "sekiz", "dokuz"];
const TENS = ["", "on", "yirmi", "otuz", "kırk", "elli", "altmış", "yetmiş", "seksen", "doksan"];
const GROUPS = ["", "bin", "milyon", "milyar", "trilyon"];

/** Largest amount `YAZIYLA` spells out (999 trilyon). */
const MAX_AMOUNT = 999_999_999_999_999;

function threeDigits(value: number): string {
  const parts: string[] = [];
  const hundreds = Math.floor(value / 100);
  const rest = value % 100;
  // "bir yüz" is never written; "yüz" stands alone.
  if (hundreds === 1) parts.push("yüz");
  else if (hundreds > 1) parts.push(`${ONES[hundreds]} yüz`);
  const tens = Math.floor(rest / 10);
  if (tens > 0) parts.push(TENS[tens]);
  const ones = rest % 10;
  if (ones > 0) parts.push(ONES[ones]);
  return parts.join(" ");
}

/** Whole number in Turkish words (0 -> "sıfır"). */
export function integerToTurkishWords(value: number): string {
  if (value === 0) return "sıfır";
  const groups: number[] = [];
  let rest = value;
  while (rest > 0) {
    groups.push(rest % 1000);
    rest = Math.floor(rest / 1000);
  }
  const parts: string[] = [];
  for (let index = groups.length - 1; index >= 0; index -= 1) {
    const group = groups[index];
    if (group === 0) continue;
    // "bir bin" is written "bin"; "bir milyon" keeps its "bir".
    if (index === 1 && group === 1) parts.push("bin");
    else parts.push(`${threeDigits(group)} ${GROUPS[index]}`.trim());
  }
  return parts.join(" ");
}

function textArg(args: Scalar[][][], index: number): string {
  return toText(args[index]?.[0]?.[0] ?? "");
}

registerFunction(
  "YAZIYLA",
  (args) => {
    const value = toNumber(args[0]?.[0]?.[0] ?? "");
    if (isError(value)) return value;
    if (Math.abs(value) > MAX_AMOUNT) return ERR.num();
    const currency = textArg(args, 1).trim() || "Türk Lirası";
    const negative = value < 0;
    const absolute = Math.abs(value);
    let lira = Math.floor(absolute + 1e-9);
    let kurus = Math.round((absolute - lira) * 100);
    if (kurus === 100) {
      lira += 1;
      kurus = 0;
    }
    const words = `Yalnız ${negative ? "eksi " : ""}${integerToTurkishWords(lira)} ${currency}`;
    return kurus > 0 ? `${words} ${integerToTurkishWords(kurus)} Kuruş` : words;
  },
  1,
  2,
  false,
  { signature: "YAZIYLA(sayı; [para_birimi])", category: "Text" },
);

/** 11-digit TCKN: first digit non-zero and both check digits as specified. */
export function tcknValid(input: string): boolean {
  const digits = input.replace(/\s+/g, "");
  if (!/^\d{11}$/.test(digits) || digits.startsWith("0")) return false;
  const values = [...digits].map(Number);
  const odd = values[0] + values[2] + values[4] + values[6] + values[8];
  const even = values[1] + values[3] + values[5] + values[7];
  // JS `%` keeps the sign: `-1 % 10` is `-1`, so a valid TCKN whose sum is
  // negative (e.g. 35 - 36) was reported invalid.
  if ((((odd * 7 - even) % 10) + 10) % 10 !== values[9]) return false;
  return values.slice(0, 10).reduce((sum, value) => sum + value, 0) % 10 === values[10];
}

/** 10-digit VKN with the weighted modulo-9 check digit. */
export function vknValid(input: string): boolean {
  const digits = input.replace(/\s+/g, "");
  if (!/^\d{10}$/.test(digits)) return false;
  // 4444444444 passes the checksum but is not a valid number.
  if (/^(\d)\1{9}$/.test(digits)) return false;
  const values = [...digits].map(Number);
  let total = 0;
  for (let index = 0; index < 9; index += 1) {
    const tmp = (values[index] + (9 - index)) % 10;
    let contribution = (tmp * 2 ** (9 - index)) % 9;
    if (tmp !== 0 && contribution === 0) contribution = 9;
    total += contribution;
  }
  return (10 - (total % 10)) % 10 === values[9];
}

function mod97(numeric: string): number {
  let remainder = 0;
  for (const ch of numeric) {
    remainder = (remainder * 10 + (ch.charCodeAt(0) - 48)) % 97;
  }
  return remainder;
}

/** Turkish IBAN (TR + 24 digits) checked with the ISO 13616 modulo-97 rule. */
export function ibanValid(input: string): boolean {
  const iban = input.replace(/\s+/g, "").toUpperCase();
  if (!/^TR\d{24}$/.test(iban)) return false;
  const rearranged = iban.slice(4) + iban.slice(0, 4);
  const numeric = rearranged.replace(/[A-Z]/g, (ch) => String(ch.charCodeAt(0) - 55));
  return mod97(numeric) === 1;
}

registerFunction("TCKNDOGRULA", (args) => tcknValid(textArg(args, 0)), 1, 1, false, {
  signature: "TCKNDOGRULA(tckn)",
  category: "Text",
});
registerFunction("VKNDOGRULA", (args) => vknValid(textArg(args, 0)), 1, 1, false, {
  signature: "VKNDOGRULA(vkn)",
  category: "Text",
});
registerFunction("IBANDOGRULA", (args) => ibanValid(textArg(args, 0)), 1, 1, false, {
  signature: "IBANDOGRULA(iban)",
  category: "Text",
});
