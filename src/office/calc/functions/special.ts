/**
 * Special functions behind the probability distributions: the normal
 * distribution and its quantile, the log-gamma function, and the regularized
 * incomplete beta and gamma functions (which give the binomial, Poisson and
 * Student t cumulative distributions).
 *
 * Plain double arithmetic; the normal CDF is good to a few units in the last
 * place and the quantile to about 1e-16 (Wichura's AS241), the others to about
 * 1e-14, which is well inside what a worksheet shows.
 */

const SQRT_2PI = Math.sqrt(2 * Math.PI);

/**
 * exp(-z²/2) with z² carried as an exact two-part sum (Dekker's product), so
 * the rounding of z·z does not turn into a relative error of z²·1e-16 in the
 * result - which is 1e-13 by z = 30.
 */
function expNegHalfSquare(z: number): number {
  const square = z * z;
  const split = 134217729 * z;
  const high = split - (split - z);
  const low = z - high;
  const error = high * high - square + 2 * high * low + low * low;
  return Math.exp(-0.5 * square) * (1 - 0.5 * error);
}

export function normalPdf(z: number): number {
  return expNegHalfSquare(z) / SQRT_2PI;
}

/**
 * Upper tail Q(t) = 1 - Φ(t) for t >= 1, from the continued fraction
 * Q(t) = φ(t) / (t + 1/(t + 2/(t + 3/(t + ...)))), summed from the far end.
 * The fraction needs roughly 500 / t² terms for a double, so the work falls
 * quickly with t.
 */
function normalTail(t: number): number {
  let denominator = t;
  for (let k = Math.ceil(600 / (t * t)) + 20; k >= 1; k -= 1) denominator = t + k / denominator;
  return normalPdf(t) / denominator;
}

/**
 * The standard normal CDF. Near the centre it sums the all-positive series
 * Φ(z) = 1/2 + φ(z)·(z + z³/3 + z⁵/(3·5) + ...), so nothing cancels; beyond
 * |z| = 1 the tail comes from the continued fraction, which keeps the
 * relative accuracy of very small probabilities.
 */
export function normalCdf(z: number): number {
  if (Number.isNaN(z)) return Number.NaN;
  const t = Math.abs(z);
  if (t >= 1) {
    if (t > 40) return z > 0 ? 1 : 0;
    const tail = normalTail(t);
    return z > 0 ? 1 - tail : tail;
  }
  let term = t;
  let sum = t;
  for (let n = 1; n < 80; n += 1) {
    term *= (t * t) / (2 * n + 1);
    sum += term;
    if (term < sum * 1e-17) break;
  }
  const upper = 0.5 + normalPdf(t) * sum;
  return z > 0 ? upper : 1 - upper;
}

/**
 * The inverse of the standard normal CDF, for 0 < p < 1.
 * Wichura, "Algorithm AS241: The Percentage Points of the Normal Distribution"
 * (PPND16), Applied Statistics 37 (1988): relative error about 1e-16.
 */
export function normalQuantile(p: number): number {
  const q = p - 0.5;
  if (Math.abs(q) <= 0.425) {
    const r = 0.180625 - q * q;
    const numerator =
      (((((((2509.0809287301227 * r + 33430.57558358813) * r + 67265.7709270087) * r + 45921.95393154987) * r +
        13731.69376550946) *
        r +
        1971.5909503065513) *
        r +
        133.14166789178438) *
        r +
        3.3871328727963665) *
      q;
    const denominator =
      ((((((5226.495278852854 * r + 28729.085735721943) * r + 39307.89580009271) * r + 21213.794301586597) * r +
        5394.196021424751) *
        r +
        687.1870074920579) *
        r +
        42.31333070160091) *
        r +
      1;
    return numerator / denominator;
  }
  let r = Math.sqrt(-Math.log(q <= 0 ? p : 1 - p));
  let value: number;
  if (r <= 5) {
    r -= 1.6;
    const numerator =
      ((((((0.0007745450142783414 * r + 0.022723844989269184) * r + 0.2417807251774506) * r + 1.2704582524523684) * r +
        3.6478483247632045) *
        r +
        5.769497221460691) *
        r +
        4.630337846156546) *
        r +
      1.4234371107496835;
    const denominator =
      ((((((1.0507500716444169e-9 * r + 0.0005475938084995345) * r + 0.015198666563616457) * r + 0.14810397642748008) *
        r +
        0.6897673349851) *
        r +
        1.6763848301838038) *
        r +
        2.053191626637759) *
        r +
      1;
    value = numerator / denominator;
  } else {
    r -= 5;
    const numerator =
      ((((((2.0103343992922881e-7 * r + 0.000027115555687434876) * r + 0.0012426609473880784) * r +
        0.026532189526576124) *
        r +
        0.29656057182850487) *
        r +
        1.7848265399172913) *
        r +
        5.463784911164114) *
        r +
      6.657904643501103;
    const denominator =
      ((((((2.0442631033899397e-15 * r + 1.421511758316446e-7) * r + 0.000018463183175100548) * r +
        0.0007868691311456133) *
        r +
        0.014875361290850615) *
        r +
        0.1369298809227358) *
        r +
        0.599832206555888) *
        r +
      1;
    value = numerator / denominator;
  }
  return q < 0 ? -value : value;
}

const LANCZOS = [
  0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059,
  12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
];

/** ln(n!) for n = 0..170, summed once from ln(2), ln(3), ... */
const LOG_FACTORIALS: number[] = [0];
for (let n = 1; n <= 170; n += 1) LOG_FACTORIALS.push(LOG_FACTORIALS[n - 1] + Math.log(n));

/** ln Γ(x) for x > 0 (Lanczos approximation, g = 7; exact sums for whole numbers up to 171). */
export function logGamma(x: number): number {
  if (Number.isInteger(x) && x >= 1 && x <= 171) return LOG_FACTORIALS[x - 1];
  if (x < 0.5) {
    // Reflection: Γ(x)Γ(1-x) = π / sin(πx).
    return Math.log(Math.PI / Math.abs(Math.sin(Math.PI * x))) - logGamma(1 - x);
  }
  const shifted = x - 1;
  let sum = LANCZOS[0];
  for (let i = 1; i < LANCZOS.length; i += 1) sum += LANCZOS[i] / (shifted + i);
  const t = shifted + 7.5;
  return 0.5 * Math.log(2 * Math.PI) + (shifted + 0.5) * Math.log(t) - t + Math.log(sum);
}

const TINY = 1e-300;

/**
 * How many terms an incomplete beta/gamma expansion may use: it needs about
 * sqrt(size) of them to converge, so a binomial with a billion trials still
 * gets an answer.
 */
function iterationLimit(size: number): number {
  return Math.min(500_000, Math.ceil(12 * Math.sqrt(size)) + 1000);
}

/** Continued fraction for the incomplete beta function (modified Lentz). */
function betaFraction(x: number, a: number, b: number): number {
  const qab = a + b;
  const qap = a + 1;
  const qam = a - 1;
  let c = 1;
  let d = 1 - (qab * x) / qap;
  if (Math.abs(d) < TINY) d = TINY;
  d = 1 / d;
  let h = d;
  const limit = iterationLimit(Math.max(a, b));
  for (let m = 1; m <= limit; m += 1) {
    const m2 = 2 * m;
    let aa = (m * (b - m) * x) / ((qam + m2) * (a + m2));
    d = 1 + aa * d;
    if (Math.abs(d) < TINY) d = TINY;
    c = 1 + aa / c;
    if (Math.abs(c) < TINY) c = TINY;
    d = 1 / d;
    h *= d * c;
    aa = (-(a + m) * (qab + m) * x) / ((a + m2) * (qap + m2));
    d = 1 + aa * d;
    if (Math.abs(d) < TINY) d = TINY;
    c = 1 + aa / c;
    if (Math.abs(c) < TINY) c = TINY;
    d = 1 / d;
    const delta = d * c;
    h *= delta;
    if (Math.abs(delta - 1) < 3e-16) break;
  }
  return h;
}

/** The regularized incomplete beta function I_x(a, b), for a, b > 0 and 0 <= x <= 1. */
export function betaIncomplete(x: number, a: number, b: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const front = Math.exp(logGamma(a + b) - logGamma(a) - logGamma(b) + a * Math.log(x) + b * Math.log1p(-x));
  // The fraction converges quickly on one side of the mean only.
  return x < (a + 1) / (a + b + 2) ? (front * betaFraction(x, a, b)) / a : 1 - (front * betaFraction(1 - x, b, a)) / b;
}

/**
 * The regularized incomplete gamma functions P(a, x) and Q(a, x) = 1 - P(a, x),
 * for a > 0 and x >= 0. The series is used below a + 1 and the continued
 * fraction above, so each is computed on the side where it converges.
 */
export function gammaIncomplete(a: number, x: number): { lower: number; upper: number } {
  if (x <= 0) return { lower: 0, upper: 1 };
  const prefix = Math.exp(-x + a * Math.log(x) - logGamma(a));
  const limit = iterationLimit(a);
  if (x < a + 1) {
    let term = 1 / a;
    let sum = term;
    for (let n = 1; n < limit; n += 1) {
      term *= x / (a + n);
      sum += term;
      if (term < sum * 1e-17) break;
    }
    const lower = prefix * sum;
    return { lower, upper: 1 - lower };
  }
  let b = x + 1 - a;
  let c = 1 / TINY;
  let d = 1 / b;
  let h = d;
  for (let i = 1; i < limit; i += 1) {
    const an = -i * (i - a);
    b += 2;
    d = an * d + b;
    if (Math.abs(d) < TINY) d = TINY;
    c = b + an / c;
    if (Math.abs(c) < TINY) c = TINY;
    d = 1 / d;
    const delta = d * c;
    h *= delta;
    if (Math.abs(delta - 1) < 3e-16) break;
  }
  const upper = prefix * h;
  return { lower: 1 - upper, upper };
}

/** P(T > t) for t >= 0 under Student's t with `df` degrees of freedom. */
export function studentUpperTail(t: number, df: number): number {
  if (t === 0) return 0.5;
  return 0.5 * betaIncomplete(df / (df + t * t), df / 2, 0.5);
}

/** The cumulative distribution of Student's t. */
export function studentCdf(t: number, df: number): number {
  const tail = studentUpperTail(Math.abs(t), df);
  return t > 0 ? 1 - tail : tail;
}

export function studentPdf(t: number, df: number): number {
  const logDensity =
    logGamma((df + 1) / 2) -
    logGamma(df / 2) -
    0.5 * Math.log(df * Math.PI) -
    ((df + 1) / 2) * Math.log1p((t * t) / df);
  return Math.exp(logDensity);
}

/**
 * The t that leaves `tail` probability above it (0 < tail < 0.5), found by a
 * safeguarded Newton iteration on the upper tail.
 */
export function studentUpperQuantile(tail: number, df: number): number {
  if (df === 1) return Math.tan(Math.PI * (0.5 - tail));
  let low = 0;
  let high = Math.max(1, normalQuantile(1 - tail));
  while (studentUpperTail(high, df) > tail && high < 1e300) high *= 2;
  let t = Math.min(Math.max(normalQuantile(1 - tail), low), high);
  for (let step = 0; step < 200; step += 1) {
    const error = studentUpperTail(t, df) - tail;
    if (error > 0) low = t;
    else high = t;
    const next = t + error / studentPdf(t, df);
    const candidate = next > low && next < high ? next : (low + high) / 2;
    if (Math.abs(candidate - t) <= 1e-15 * Math.max(1, Math.abs(t))) return candidate;
    t = candidate;
  }
  return t;
}
