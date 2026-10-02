/**
 * Financial functions.
 *
 * The cash-flow conventions follow Excel exactly so a model built here matches
 * one built in Excel or LibreOffice: `PMT` returns a negative number for money
 * received, `NPV` discounts the first value by one period, `IRR` needs at least
 * one sign change, and `RATE` is solved numerically.
 */
import { registerFunction } from "../registry";
import { ERR, FormulaError, flatten, isError, toNumber, toText, type Scalar } from "../scalars";

interface Args {
  rate: number;
  periods: number;
  present?: number;
  future?: number;
  type?: number;
}

function readNumber(args: Scalar[][][], index: number, fallback: number): number | FormulaError {
  if (args[index] === undefined) return fallback;
  const value = toNumber(args[index]?.[0]?.[0] ?? 0);
  return isError(value) ? value : value;
}

function cashFlows(args: Scalar[][][]): number[] | FormulaError {
  return flatten(args).reduce<number[]>((out, value) => {
    const number = toNumber(value);
    if (!isError(number)) out.push(number);
    return out;
  }, []);
}

/** Future value of a lump sum: (1 + rate)^periods. */
function powerFactor(rate: number, periods: number): number {
  return (1 + rate) ** periods;
}

registerFunction("PMT", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const periods = readNumber(args, 1, 0);
  if (typeof periods !== "number") return periods;
  const present = readNumber(args, 2, 0);
  if (typeof present !== "number") return present;
  const future = readNumber(args, 3, 0);
  if (typeof future !== "number") return future;
  const kind = readNumber(args, 4, 0);
  if (typeof kind !== "number") return kind;
  if (periods === 0) return ERR.num();
  if (rate === 0) return -(present + future) / periods;
  const factor = powerFactor(rate, periods);
  return (-present * factor - future) * rate / ((factor - 1) * (1 + kind * rate));
}, 3, 5, false, { signature: "PMT(rate, nper, pv, [fv], [type])", category: "Financial" });

registerFunction("PV", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const periods = readNumber(args, 1, 0);
  if (typeof periods !== "number") return periods;
  const payment = readNumber(args, 2, 0);
  if (typeof payment !== "number") return payment;
  const future = readNumber(args, 3, 0);
  if (typeof future !== "number") return future;
  const kind = readNumber(args, 4, 0);
  if (typeof kind !== "number") return kind;
  if (rate === 0) return -(future + payment * periods);
  const factor = powerFactor(rate, periods);
  return -(future + payment * (1 + kind * rate) * ((factor - 1) / rate)) / factor;
}, 3, 5, false, { signature: "PV(rate, nper, pmt, [fv], [type])", category: "Financial" });

registerFunction("FV", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const periods = readNumber(args, 1, 0);
  if (typeof periods !== "number") return periods;
  const payment = readNumber(args, 2, 0);
  if (typeof payment !== "number") return payment;
  const present = readNumber(args, 3, 0);
  if (typeof present !== "number") return present;
  const kind = readNumber(args, 4, 0);
  if (typeof kind !== "number") return kind;
  if (rate === 0) return -(present + payment * periods);
  const factor = powerFactor(rate, periods);
  return -(present * factor + payment * (1 + kind * rate) * ((factor - 1) / rate));
}, 3, 5, false, { signature: "FV(rate, nper, pmt, [pv], [type])", category: "Financial" });

/** Number of periods for an investment, solved from the annuity identity. */
registerFunction("NPER", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const payment = readNumber(args, 1, 0);
  if (typeof payment !== "number") return payment;
  const present = readNumber(args, 2, 0);
  if (typeof present !== "number") return present;
  const future = readNumber(args, 3, 0);
  if (typeof future !== "number") return future;
  const kind = readNumber(args, 4, 0);
  if (typeof kind !== "number") return kind;
  if (rate === 0) {
    if (payment === 0) return ERR.num();
    return -(present + future) / payment;
  }
  const adjusted = payment * (1 + kind * rate);
  const numerator = adjusted - future * rate;
  const denominator = present * rate + adjusted;
  if (numerator === 0 || denominator / numerator <= 0) return ERR.num();
  return Math.log(numerator / denominator) / Math.log(1 + rate);
}, 3, 5, false, { signature: "NPER(rate, pmt, pv, [fv], [type])", category: "Financial" });

/**
 * Solves the periodic interest rate by bisection.
 *
 * Bisection rather than Newton: the annuity equation is not monotonic near the
 * asymptotes, and a bracketed search cannot run away to a nonsense rate.
 */
function solveRate(present: number, payment: number, future: number, periods: number, type: number, guess: number): number | FormulaError {
  if (periods <= 0) return ERR.num();
  const at = (rate: number) => {
    if (Math.abs(rate) < 1e-10) return present + payment * periods + future;
    const factor = (1 + rate) ** periods;
    return present * factor + payment * (1 + type * rate) * ((factor - 1) / rate) + future;
  };
  let low = guess - 1;
  let high = guess + 1;
  let lowValue = at(low);
  let highValue = at(high);
  let expansions = 0;
  while (lowValue * highValue > 0 && expansions < 60) {
    low = low * 2 - 1;
    high = high * 2 + 1;
    if (!Number.isFinite(low) || !Number.isFinite(high)) return ERR.num();
    lowValue = at(low);
    highValue = at(high);
    expansions += 1;
  }
  if (lowValue * highValue > 0) return ERR.num();
  for (let step = 0; step < 200; step += 1) {
    const middle = (low + high) / 2;
    const value = at(middle);
    if (Math.abs(value) < 1e-9 || high - low < 1e-12) return middle;
    if (lowValue * value <= 0) {
      high = middle;
      highValue = value;
    } else {
      low = middle;
      lowValue = value;
    }
  }
  return (low + high) / 2;
}

registerFunction("RATE", (args) => {
  const periods = readNumber(args, 0, 0);
  if (typeof periods !== "number") return periods;
  const payment = readNumber(args, 1, 0);
  if (typeof payment !== "number") return payment;
  const present = readNumber(args, 2, 0);
  if (typeof present !== "number") return present;
  const future = readNumber(args, 3, 0);
  if (typeof future !== "number") return future;
  const kind = readNumber(args, 4, 0);
  if (typeof kind !== "number") return kind;
  const guess = readNumber(args, 5, 0.1);
  if (typeof guess !== "number") return guess;
  return solveRate(present, payment, future, periods, kind, guess);
}, 3, 6, false, { signature: "RATE(nper, pmt, pv, [fv], [type], [guess])", category: "Financial" });

registerFunction("NPV", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const flows = cashFlows(args.slice(1));
  if (isError(flows)) return flows;
  if (flows.length === 0) return ERR.num();
  if (rate === -1) return ERR.div();
  let total = 0;
  flows.forEach((value, index) => {
    total += value / (1 + rate) ** (index + 1);
  });
  return total;
}, 2, 64, false, { signature: "NPV(rate, value1, ...)", category: "Financial" });

registerFunction("IRR", (args) => {
  const flows = cashFlows([args[0] ?? []]);
  if (isError(flows)) return flows;
  if (flows.length < 2) return ERR.num();
  const hasPositive = flows.some((value) => value > 0);
  const hasNegative = flows.some((value) => value < 0);
  if (!hasPositive || !hasNegative) return ERR.num();
  const guess = args[1] ? toNumber(args[1]?.[0]?.[0] ?? 0.1) : 0.1;
  const at = (rate: number) => {
    let total = 0;
    flows.forEach((value, index) => {
      total += value / (1 + rate) ** index;
    });
    return total;
  };
  let low = -0.999_999;
  let high = 10;
  let lowValue = at(low);
  let highValue = at(high);
  if (lowValue * highValue > 0) return ERR.num();
  for (let step = 0; step < 300; step += 1) {
    const middle = (low + high) / 2;
    const value = at(middle);
    if (Math.abs(value) < 1e-10 || high - low < 1e-13) return middle;
    if (lowValue * value <= 0) {
      high = middle;
      highValue = value;
    } else {
      low = middle;
      lowValue = value;
    }
  }
  void guess;
  return (low + high) / 2;
}, 1, 2, false, { signature: "IRR(values, [guess])", category: "Financial" });

/** Pairs values with their dates for the X-prefixed discount functions. */
function datedFlows(values: Scalar[], dates: Scalar[]): Array<{ value: number; date: number }> | null {
  if (values.length !== dates.length || values.length === 0) return null;
  const out: Array<{ value: number; date: number }> = [];
  for (let index = 0; index < values.length; index += 1) {
    const value = toNumber(values[index]);
    const date = toNumber(dates[index]);
    if (isError(value) || isError(date)) return null;
    out.push({ value, date });
  }
  out.sort((a, b) => a.date - b.date);
  return out;
}

registerFunction("XNPV", (args) => {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const flows = datedFlows(flatten([args[1] ?? []]), flatten([args[2] ?? []]));
  if (!flows) return ERR.num();
  const base = flows[0].date;
  let total = 0;
  for (const flow of flows) {
    const years = (flow.date - base) / 365;
    total += flow.value / (1 + rate) ** years;
  }
  return total;
}, 3, 3, false, { signature: "XNPV(rate, values, dates)", category: "Financial" });

registerFunction("XIRR", (args) => {
  const flows = datedFlows(flatten([args[0] ?? []]), flatten([args[1] ?? []]));
  if (!flows || flows.length < 2) return ERR.num();
  if (!flows.some((flow) => flow.value > 0) || !flows.some((flow) => flow.value < 0)) return ERR.num();
  const base = flows[0].date;
  const at = (rate: number) => {
    let total = 0;
    for (const flow of flows) {
      const years = (flow.date - base) / 365;
      total += flow.value / (1 + rate) ** years;
    }
    return total;
  };
  let low = -0.999_999;
  let high = 10;
  let lowValue = at(low);
  let highValue = at(high);
  if (lowValue * highValue > 0) return ERR.num();
  for (let step = 0; step < 300; step += 1) {
    const middle = (low + high) / 2;
    const value = at(middle);
    if (Math.abs(value) < 1e-10 || high - low < 1e-13) return middle;
    if (lowValue * value <= 0) {
      high = middle;
      highValue = value;
    } else {
      low = middle;
      lowValue = value;
    }
  }
  return (low + high) / 2;
}, 2, 3, false, { signature: "XIRR(values, dates, [guess])", category: "Financial" });

registerFunction("MIRR", (args) => {
  const flows = cashFlows([args[0] ?? []]);
  if (isError(flows)) return flows;
  if (flows.length < 2) return ERR.num();
  const finance = readNumber(args, 1, 0);
  if (typeof finance !== "number") return finance;
  const reinvest = readNumber(args, 2, 0);
  if (typeof reinvest !== "number") return reinvest;
  const negatives: number[] = [];
  const positives: number[] = [];
  flows.forEach((value, index) => {
    if (value < 0) negatives.push(value / (1 + finance) ** index);
    else positives.push(value * (1 + reinvest) ** (flows.length - 1 - index));
  });
  const negativeTotal = negatives.reduce((sum, value) => sum + value, 0);
  const positiveTotal = positives.reduce((sum, value) => sum + value, 0);
  if (negativeTotal === 0 || positiveTotal === 0) return ERR.div();
  const periods = flows.length - 1;
  return (-positiveTotal / negativeTotal) ** (1 / periods) - 1;
}, 3, 3, false, { signature: "MIRR(values, finance_rate, reinvest_rate)", category: "Financial" });

registerFunction("SLN", (args) => {
  const cost = readNumber(args, 0, 0);
  if (typeof cost !== "number") return cost;
  const salvage = readNumber(args, 1, 0);
  if (typeof salvage !== "number") return salvage;
  const life = readNumber(args, 2, 0);
  if (typeof life !== "number") return life;
  if (life === 0) return ERR.div();
  return (cost - salvage) / life;
}, 3, 3, false, { signature: "SLN(cost, salvage, life)", category: "Financial" });

registerFunction("SYD", (args) => {
  const cost = readNumber(args, 0, 0);
  if (typeof cost !== "number") return cost;
  const salvage = readNumber(args, 1, 0);
  if (typeof salvage !== "number") return salvage;
  const life = readNumber(args, 2, 0);
  if (typeof life !== "number") return life;
  const period = readNumber(args, 3, 1);
  if (typeof period !== "number") return period;
  if (life <= 0 || period < 1 || period > life) return ERR.num();
  return ((cost - salvage) * (life - period + 1) * 2) / (life * (life + 1));
}, 4, 4, false, { signature: "SYD(cost, salvage, life, period)", category: "Financial" });

registerFunction("DB", (args) => {
  const cost = readNumber(args, 0, 0);
  if (isError(cost)) return cost;
  const salvageArg = readNumber(args, 1, 0);
  if (isError(salvageArg)) return salvageArg;
  // A salvage value of zero is legal for DB (the asset is written off).
  const salvage = salvageArg;
  const life = readNumber(args, 2, 0);
  if (typeof life !== "number") return life;
  const period = readNumber(args, 3, 1);
  if (typeof period !== "number") return period;
  const month = readNumber(args, 4, 12);
  if (typeof month !== "number") return month;
  if (cost <= 0 || life <= 0 || period < 1) return ERR.num();
  // A zero (or negative) salvage means the asset is fully written off.
  const residual = Math.max(0, salvage);
  const rate = Math.round((1 - (residual / cost) ** (1 / life)) * 1000) / 1000;
  const total = rate * cost;
  let depreciation = total * month / 12;
  let remaining = cost;
  for (let index = 1; index < period; index += 1) {
    depreciation = Math.min(remaining, total - depreciation);
    remaining -= depreciation;
  }
  if (period === life + 1) return ((cost - total) * rate) * (12 - month) / 12;
  return Math.min(depreciation, remaining);
}, 4, 5, false, { signature: "DB(cost, salvage, life, period, [month])", category: "Financial" });

registerFunction("DDB", (args) => {
  const cost = readNumber(args, 0, 0);
  if (typeof cost !== "number") return cost;
  const salvage = readNumber(args, 1, 0);
  if (typeof salvage !== "number") return salvage;
  const life = readNumber(args, 2, 0);
  if (typeof life !== "number") return life;
  const period = readNumber(args, 3, 1);
  if (typeof period !== "number") return period;
  const factor = readNumber(args, 4, 2);
  if (typeof factor !== "number") return factor;
  if (cost <= 0 || life <= 0 || period < 1 || factor <= 0) return ERR.num();
  let bookValue = cost;
  for (let index = 1; index < period; index += 1) {
    const depreciation = Math.min(bookValue, (bookValue - salvage) * factor / life);
    bookValue -= depreciation;
  }
  return Math.min(bookValue, (bookValue - salvage) * factor / life);
}, 4, 5, false, { signature: "DDB(cost, salvage, life, period, [factor])", category: "Financial" });

registerFunction("EFFECT", (args) => {
  const nominal = readNumber(args, 0, 0);
  if (typeof nominal !== "number") return nominal;
  const periods = readNumber(args, 1, 12);
  if (typeof periods !== "number") return periods;
  if (nominal <= 0 || periods < 1) return ERR.num();
  return (1 + nominal / periods) ** periods - 1;
}, 2, 2, false, { signature: "EFFECT(nominal_rate, npery)", category: "Financial" });

registerFunction("NOMINAL", (args) => {
  const effective = readNumber(args, 0, 0);
  if (typeof effective !== "number") return effective;
  const periods = readNumber(args, 1, 12);
  if (typeof periods !== "number") return periods;
  if (effective <= 0 || periods < 1) return ERR.num();
  return ((1 + effective) ** (1 / periods) - 1) * periods;
}, 2, 2, false, { signature: "NOMINAL(effect_rate, npery)", category: "Financial" });

/**
 * Interest part of a single annuity payment (Excel's IPMT with fv = 0).
 * `payment` comes from the same closed form as `PMT` above; with `kind = 1`
 * the first period carries no interest because the payment is made at the
 * beginning of the period.
 */
function periodInterest(rate: number, period: number, present: number, payment: number, kind: number): number {
  if (rate === 0) return 0;
  let index = period;
  if (kind === 1) {
    if (index === 1) return 0;
    index -= 1;
  }
  const factor = powerFactor(rate, index - 1);
  const balance = -(present * factor + payment * ((factor - 1) / rate));
  return balance * rate;
}

/** Shared body of CUMIPMT/CUMPRINC; `principal` selects the principal part. */
function cumulativePayment(args: Scalar[][][], principal: boolean): number | FormulaError {
  const rate = readNumber(args, 0, 0);
  if (typeof rate !== "number") return rate;
  const periods = readNumber(args, 1, 0);
  if (typeof periods !== "number") return periods;
  const present = readNumber(args, 2, 0);
  if (typeof present !== "number") return present;
  const startValue = readNumber(args, 3, 0);
  if (typeof startValue !== "number") return startValue;
  const endValue = readNumber(args, 4, 0);
  if (typeof endValue !== "number") return endValue;
  const kindValue = readNumber(args, 5, 0);
  if (typeof kindValue !== "number") return kindValue;
  const start = Math.trunc(startValue);
  const end = Math.trunc(endValue);
  const kind = Math.trunc(kindValue);
  if (rate <= 0 || periods <= 0 || present <= 0) return ERR.num();
  if (start < 1 || end < start || end > periods || (kind !== 0 && kind !== 1)) return ERR.num();
  const factor = powerFactor(rate, periods);
  const payment = (-present * factor) * rate / ((factor - 1) * (1 + kind * rate));
  let total = 0;
  for (let period = start; period <= end; period += 1) {
    const interest = periodInterest(rate, period, present, payment, kind);
    total += principal ? payment - interest : interest;
  }
  return total;
}

registerFunction("CUMIPMT", (args) => cumulativePayment(args, false), 6, 6, false, {
  signature: "CUMIPMT(rate, nper, pv, start_period, end_period, type)",
  category: "Financial",
});

registerFunction("CUMPRINC", (args) => cumulativePayment(args, true), 6, 6, false, {
  signature: "CUMPRINC(rate, nper, pv, start_period, end_period, type)",
  category: "Financial",
});

export { toText as financialToText };
export type { Args as FinancialArgs };
