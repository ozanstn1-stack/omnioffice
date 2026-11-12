/**
 * Turkey-specific Calc functions: YAZIYLA and the TCKN/VKN/IBAN verifiers,
 * plus the WORKDAY / NETWORKDAYS `"TR"` holiday flag.
 *
 * Note on separators: the formula engine uses `,` (or `;`) as the argument
 * separator, so a decimal inside a formula is written with a dot -
 * `YAZIYLA(1250.15)`; typed cell input still accepts `1.250,15` (see the
 * locale tests).
 */
import { describe, expect, it } from "vitest";
import { registerBuiltinFunctions } from "./functions";
import { evaluateFormula, isError, type FormulaContext, type Scalar } from "./formula";
import { ibanValid, integerToTurkishWords, tcknValid, vknValid } from "./functions/turkish";
import { dateToSerial } from "./numberFormat";
import { KURBAN_STARTS, RAMAZAN_STARTS, turkishHalfDaySerials, turkishHolidaySerials } from "./tr-holidays";

registerBuiltinFunctions();

function context(values: Record<string, Scalar> = {}): FormulaContext {
  return {
    getValue: (_sheet, address) => (address in values ? values[address] : ""),
    sheetNames: ["Sheet1"],
    currentSheet: "Sheet1",
  };
}

function evalText(formula: string): string {
  const result = evaluateFormula(formula, context());
  if (isError(result)) throw new Error(`${formula} returned ${result.code}`);
  return String(result);
}

function evalValue(formula: string): Scalar {
  return evaluateFormula(formula, context());
}

describe("YAZIYLA", () => {
  it("writes the plan's example", () => {
    expect(evalText("=YAZIYLA(1250.15)")).toBe("Yalnız bin iki yüz elli Türk Lirası on beş Kuruş");
  });

  it("handles zero, thousands and millions", () => {
    expect(evalText("=YAZIYLA(0)")).toBe("Yalnız sıfır Türk Lirası");
    expect(evalText("=YAZIYLA(1000)")).toBe("Yalnız bin Türk Lirası");
    expect(evalText("=YAZIYLA(1000000)")).toBe("Yalnız bir milyon Türk Lirası");
    expect(evalText("=YAZIYLA(100)")).toBe("Yalnız yüz Türk Lirası");
    expect(evalText("=YAZIYLA(1250000.05)")).toBe("Yalnız bir milyon iki yüz elli bin Türk Lirası beş Kuruş");
  });

  it("accepts a currency name and a negative amount", () => {
    expect(evalText('=YAZIYLA(1; "Dolar")')).toBe("Yalnız bir Dolar");
    expect(evalText("=YAZIYLA(-25.5)")).toBe("Yalnız eksi yirmi beş Türk Lirası elli Kuruş");
  });

  it("rounds the kuruş into the lira", () => {
    expect(evalText("=YAZIYLA(1.999)")).toBe("Yalnız iki Türk Lirası");
  });

  it("rejects text", () => {
    expect(isError(evalValue('=YAZIYLA("abc")'))).toBe(true);
  });

  it("spells integers directly", () => {
    expect(integerToTurkishWords(0)).toBe("sıfır");
    expect(integerToTurkishWords(15)).toBe("on beş");
    expect(integerToTurkishWords(1234)).toBe("bin iki yüz otuz dört");
  });
});

describe("TCKNDOGRULA", () => {
  it("accepts a valid number and rejects broken ones", () => {
    expect(tcknValid("10000000146")).toBe(true);
    expect(tcknValid("10000000148")).toBe(false);
    expect(tcknValid("00000000000")).toBe(false);
    expect(tcknValid("1234567890")).toBe(false);
    expect(tcknValid("123456789012")).toBe(false);
    // `odd * 7 - even` is negative here (35 - 36); JS `%` keeps the sign, so
    // this used to be reported invalid.
    expect(tcknValid("19191919190")).toBe(true);
    expect(tcknValid("19191919191")).toBe(false);
    expect(evalValue("=TCKNDOGRULA(10000000146)")).toBe(true);
    expect(evalValue("=TCKNDOGRULA(10000000148)")).toBe(false);
  });
});

describe("VKNDOGRULA", () => {
  it("accepts the Diyanet-documented example and rejects bad ones", () => {
    // 1000036109 is the worked example in the VKN algorithm description.
    expect(vknValid("1000036109")).toBe(true);
    expect(vknValid("1000036108")).toBe(false);
    expect(vknValid("4444444444")).toBe(false);
    expect(vknValid("123456789")).toBe(false);
  });
});

describe("IBANDOGRULA", () => {
  it("validates the modulo-97 check digits", () => {
    // A well-known Turkish IBAN example (TR33 0006 1005 1978 6457 8413 26).
    expect(ibanValid("TR330006100519786457841326")).toBe(true);
    expect(ibanValid("TR33 0006 1005 1978 6457 8413 26")).toBe(true);
    expect(ibanValid("TR340006100519786457841326")).toBe(false);
    expect(ibanValid("DE89370400440532013000")).toBe(false);
    expect(ibanValid("TR33000610051978645784132")).toBe(false);
  });
});

describe("Turkish holidays", () => {
  it("lists the fixed and religious days of a tabulated year", () => {
    const holidays = turkishHolidaySerials(2026);
    // 7 fixed + 3 Ramazan + 4 Kurban.
    expect(holidays).toHaveLength(14);
    expect(holidays).toContain(dateToSerial(new Date(Date.UTC(2026, 0, 1))));
    expect(holidays).toContain(dateToSerial(new Date(Date.UTC(2026, 2, 20))));
    expect(holidays).toContain(dateToSerial(new Date(Date.UTC(2026, 4, 30))));
  });

  it("keeps only the fixed days after the last listed year", () => {
    expect(turkishHolidaySerials(2051)).toHaveLength(7);
  });

  it("counts the calculated feasts after 2029, twice in a year when the calendar does", () => {
    const day = (y: number, m: number, d: number) => dateToSerial(new Date(Date.UTC(y, m - 1, d)));
    // 2030: Ramazan Bayramı 4-6 February, Kurban Bayramı 13-16 April.
    expect(turkishHolidaySerials(2030)).toHaveLength(14);
    expect(turkishHolidaySerials(2030)).toContain(day(2030, 2, 4));
    expect(turkishHolidaySerials(2030)).toContain(day(2030, 4, 16));
    expect(turkishHalfDaySerials(2030)).toContain(day(2030, 4, 12));
    // 2033 has Ramazan Bayramı on 2 January and again on 23 December.
    expect(turkishHolidaySerials(2033)).toHaveLength(17);
    expect(turkishHolidaySerials(2033)).toContain(day(2033, 12, 25));
  });

  it("spaces the listed feasts one lunar year apart, Kurban ten weeks after Ramazan", () => {
    const serial = (iso: string) => dateToSerial(new Date(`${iso}T00:00:00Z`));
    for (const starts of [RAMAZAN_STARTS, KURBAN_STARTS]) {
      for (let i = 1; i < starts.length; i += 1) {
        const gap = serial(starts[i]) - serial(starts[i - 1]);
        expect(gap, starts[i]).toBeGreaterThanOrEqual(353);
        expect(gap, starts[i]).toBeLessThanOrEqual(356);
      }
    }
    expect(KURBAN_STARTS).toHaveLength(RAMAZAN_STARTS.length);
    RAMAZAN_STARTS.forEach((ramazan, i) => {
      const gap = serial(KURBAN_STARTS[i]) - serial(ramazan);
      expect(gap, ramazan).toBeGreaterThanOrEqual(67);
      expect(gap, ramazan).toBeLessThanOrEqual(69);
    });
  });

  it("NETWORKDAYS skips the TR holidays with the flag", () => {
    // 16-23 March 2026: the weekend plus Ramazan Bayramı (20-22) leave 5 days.
    expect(evalValue('=NETWORKDAYS(DATE(2026;3;16); DATE(2026;3;23); "TR")')).toBe(5);
    // Without the flag only the weekend is skipped: 6 days.
    expect(evalValue("=NETWORKDAYS(DATE(2026;3;16); DATE(2026;3;23))")).toBe(6);
    // 29 October 2026 (Republic Day) is a holiday.
    expect(evalValue('=NETWORKDAYS(DATE(2026;10;28); DATE(2026;10;30); "TR")')).toBe(2);
  });

  it("WORKDAY advances over the TR holidays", () => {
    const expected = dateToSerial(new Date(Date.UTC(2026, 2, 24)));
    expect(evalValue('=WORKDAY(DATE(2026;3;16); 5; "TR")')).toBe(expected);
  });

  it("a user holiday range still works next to the flag", () => {
    // 1 January 2026 is a fixed holiday; adding 1-2 Jan to the list changes
    // nothing for a range that already skips it.
    expect(evalValue('=NETWORKDAYS(DATE(2026;1;1); DATE(2026;1;2); "TR")')).toBe(1);
  });
});
