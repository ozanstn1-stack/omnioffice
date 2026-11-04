// The JSX scanner behind `scripts/i18n-audit.mjs`.
//
// It is a pure function of (file name, source text): no file system, no config,
// so the audit keeps its shape and this part can be tested on inline snippets.
//
// What counts as user-visible text:
//   * a JSX text node with at least two letters ("—", "·", "1", "/" are
//     separators the translation table never needs),
//   * one of the ATTRIBUTES or PROPS whose value is a literal - a plain string,
//     a braced string or a template without substitutions. A `t("…")` call or
//     a variable is not a hardcoded string and is skipped.
//
// A string also reaches the screen through an expression, so an expression is
// read the same way in an attribute, a prop, a JSX child and an object value:
// a template counts for its fixed words with each interpolation collapsed to
// `${…}`, both branches of a conditional count, and so does the fallback of
// `??`, `||` and `&&`. Two things explicitly do not count: the condition
// itself (`mode === "edit"` is a comparison, not prose) and `+` concatenation
// (a builder, not a sentence).
import ts from "typescript";

/** Attributes the user reads or hears (aria, alt, tooltips, form hints). */
export const ATTRIBUTES = ["alt", "aria-label", "title", "placeholder"];

/** Component props that hold user-visible text. Only the names a component
 * here really declares: `label` and `hint` (`Field`, `SectionTitle`), and
 * `subtitle` (`Screen`). Adding one is a deliberate change - the test pins the
 * list. `title` stays out on purpose: it is a real attribute too, so counting
 * it as a prop as well would count the same text twice. */
export const PROPS = ["label", "hint", "subtitle"];

const PROSE = /[A-Za-zÇĞİÖŞÜçğıöşü]{2,}/;

/** The literal text of a JSX value, or null when it is not a literal. */
export function literalText(initializer, source) {
  if (!initializer) return null;
  if (ts.isStringLiteral(initializer)) return initializer.text;
  if (ts.isJsxExpression(initializer) && initializer.expression) {
    const expression = initializer.expression;
    if (ts.isStringLiteral(expression)) return expression.text;
    if (ts.isNoSubstitutionTemplateLiteral(expression)) return expression.text;
    // `alt={`Page ${page}`}` is a template, not a plain string, but its fixed
    // words still reach the screen. The interpolation collapses to one
    // placeholder, the same shape the object scan uses.
    if (ts.isTemplateExpression(expression)) return templateText(expression);
  }
  void source;
  return null;
}

/** A template's fixed words with each interpolation collapsed to `${…}`. */
export function templateText(node) {
  return [node.head.text, ...node.templateSpans.map((span) => `\${…}${span.literal.text}`)].join("");
}

/** The strings an expression can put on the screen, each tagged with where it
 * came from: `literal` (the value itself), `span` (inside a template's
 * interpolation) or `branch` (one arm of a conditional or a logical fallback).
 * The tag is what `--expressions-report` lists, so a reader can see which
 * strings the wider rule added. */
export function expressionTexts(node, via = "literal") {
  const current = unwrap(node);
  if (ts.isStringLiteral(current) || ts.isNoSubstitutionTemplateLiteral(current)) {
    return [{ text: current.text, via }];
  }
  if (ts.isTemplateExpression(current)) {
    const rows = [{ text: templateText(current), via }];
    for (const span of current.templateSpans) {
      rows.push(...expressionTexts(span.expression, "span"));
    }
    return rows;
  }
  if (ts.isConditionalExpression(current)) {
    // The condition is a test, not prose: `mode === "edit"` must not count.
    return [
      ...expressionTexts(current.whenTrue, "branch"),
      ...expressionTexts(current.whenFalse, "branch"),
    ];
  }
  if (ts.isBinaryExpression(current)) {
    // `a ?? "Adsız"`, `busy && "Yükleniyor"`, `name || "Anonim"`: the right
    // operand is the one the user reads. `+` builds a string instead of
    // choosing one, so it stays out.
    const fallback = current.operatorToken.kind;
    if (
      fallback === ts.SyntaxKind.QuestionQuestionToken ||
      fallback === ts.SyntaxKind.BarBarToken ||
      fallback === ts.SyntaxKind.AmpersandAmpersandToken
    ) {
      return expressionTexts(current.right, "branch");
    }
  }
  // A call, a variable, a property access, a JSX child, a function: not text.
  return [];
}

/** The strings a JSX attribute (or prop) value holds. */
export function attributeTexts(initializer) {
  if (!initializer) return [];
  if (ts.isStringLiteral(initializer)) return [{ text: initializer.text, via: "literal" }];
  if (ts.isJsxExpression(initializer) && initializer.expression) {
    return expressionTexts(initializer.expression);
  }
  return [];
}

/**
 * Every hardcoded string in one TSX source, with the line it sits on.
 * The three lists keep their order: text nodes, attributes, props.
 */
export function scanSource(name, body) {
  const source = ts.createSourceFile(name, body, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const lineOf = (node) => source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
  const texts = [];
  const attributes = [];
  const props = [];
  const prose = (line, text, via) => ({ line, text: text.trim(), via });
  const visit = (node) => {
    if (ts.isJsxText(node)) {
      const text = node.text.replace(/\s+/g, " ").trim();
      if (PROSE.test(text)) texts.push({ line: lineOf(node), text, via: "literal" });
    } else if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(source);
      for (const row of attributeTexts(node.initializer)) {
        if (!PROSE.test(row.text)) continue;
        const entry = prose(lineOf(node), row.text, row.via);
        if (ATTRIBUTES.includes(name)) attributes.push({ ...entry, name });
        else if (PROPS.includes(name)) props.push({ ...entry, name });
      }
    } else if (ts.isJsxExpression(node) && node.expression) {
      // `<p>{ok ? "Tamam" : "Hata"}</p>` and `<p>{"Merhaba"}</p>` are text
      // nodes to the user. Only when the expression is a child: an attribute
      // value is read above, and counting it here too would double it.
      if (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent)) {
        for (const row of expressionTexts(node.expression)) {
          if (PROSE.test(row.text)) texts.push(prose(lineOf(node), row.text, row.via));
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  const findings = [
    ...texts.map((row) => ({ ...row, kind: "text" })),
    ...attributes.map((row) => ({ ...row, kind: "attribute" })),
    ...props.map((row) => ({ ...row, kind: "prop" })),
  ];
  const { kept, ignored, invalid } = bindLiteralMarkers(findings, findLiteralMarkers(body));
  const of = (kind) => kept.filter((row) => row.kind === kind);
  return {
    texts: of("text"),
    attributes: of("attribute"),
    props: of("prop"),
    ignored,
    invalidMarkers: invalid,
  };
}

/** The per-file counts the baseline records. */
export function countByFile(entries) {
  const counts = {};
  for (const [file, found] of entries) {
    const total = found.texts.length + found.attributes.length + found.props.length;
    if (total) counts[file] = total;
  }
  return counts;
}

// ---------------------------------------------------------------------------
// Object literals and `.ts` modules (E4.6).
//
// The JSX scan above reads `.tsx` files only, so interface text that reaches
// the screen through an object literal (`{ value: "employee", label: "Çalışan" }`)
// or is built inside a `.ts` module never showed up. This second reading counts
// those strings in their own `objects` section. It measures; it does not fix.
//
// Two deliberate narrowings keep the baseline honest:
//   * only a value that carries a Turkish-specific letter is counted, so product
//     names and identifiers that share these keys (`DeepSeek`, `Letter`) stay
//     out. ASCII-only Turkish ("Belge") slips through - a stated, documented
//     gap rather than a silent one.
//   * a file that writes the document itself (`document content`) is left alone
//     as a whole: translating an official letter would change what it says.
// ---------------------------------------------------------------------------

/** Object keys whose value a user reads. `detail` is the second line a toast
 * shows, so it reaches the screen like a title does. `name`, `value` and
 * `message` stay out on purpose: they carry identifiers, product names and
 * developer errors. */
export const OBJECT_KEYS = [
  "label",
  "title",
  "description",
  "hint",
  "subtitle",
  "placeholder",
  "tooltip",
  "detail",
];

/** Files that write the artefact rather than the interface, with the reason
 * each one is out of scope. Kept in code, not in the generated baseline, so the
 * justification goes through review. */
export const CONTENT_PATHS = [
  {
    path: "src/lib/official-letter.ts",
    reason: "writes the official letter body, which is Turkish by definition",
  },
  {
    path: "src/lib/official-letter-template.ts",
    reason: "same letter, as the block template the editor shows (Dağıtım / Gereği / Bilgi)",
  },
  {
    path: "src/lib/ai-task-pack-eval.ts",
    reason: "sample AI answers used as scoring fixtures, never rendered",
  },
];

const TURKISH = /[çğıöşüÇĞİÖŞÜ]/;
const CONTENT_PRAGMA = /i18n-content:\s*(\S[^\n]*)/;

/** A deliberate literal on the JSX side: syntax, a unit, a shortcut. The reason
 * is required - a marker without one is an error, not a pass. */
export const LITERAL_PRAGMA = "i18n-literal";

/** The absolute ceiling on markers. Raising it is a code change with review,
 * which is the friction the check exists for; the ratchet below it is the
 * baseline count. */
export const LITERAL_HARD_CAP = 5;

/** Why a reason is not good enough, or null when it is. A reason has to say
 * something: "todo" and "fixme" are placeholders, not justifications. */
export function literalReasonProblem(reason) {
  const text = String(reason ?? "").trim();
  if (!text) return "missing-reason";
  if (text.length < 12 || /^(todo|fixme|tmp|temp|ignore|skip|later|wip|n\/?a|-)\b/i.test(text)) {
    return "weak-reason";
  }
  return null;
}

/**
 * The marker comments in one source, by line. Two shapes are accepted:
 *
 *   // i18n-literal: reason            (above an attribute, or trailing it)
 *   {/* i18n-literal: reason *\/}       (above a JSX child)
 *
 * A standalone comment binds to the next line that is neither blank nor a
 * comment; a trailing one binds to its own line.
 */
export function findLiteralMarkers(body) {
  const lines = body.split("\n");
  const markers = [];
  const commentOnly = /^\s*(?:(?:\/\/|\{\s*\/\*)\s*i18n-literal:\s*([^\n]*?)(?:\*\/\s*\})?)\s*$/;
  const trailing = /(?:\/\/|\/\*)\s*i18n-literal:\s*([^\n]*?)(?:\*\/)?\s*$/;
  lines.forEach((line, index) => {
    const number = index + 1;
    const standalone = commentOnly.exec(line);
    if (standalone) {
      let bound = number + 1;
      while (bound <= lines.length && (lines[bound - 1].trim() === "" || commentOnly.test(lines[bound - 1]))) {
        bound += 1;
      }
      markers.push({ line: number, boundLine: bound, reason: standalone[1].trim(), standalone: true });
      return;
    }
    const tail = trailing.exec(line);
    if (tail) markers.push({ line: number, boundLine: number, reason: tail[1].trim(), standalone: false });
  });
  return markers;
}

/** Groups the findings of one line: an attribute's template and its expression
 * are one decision, not two. */
function groupKey(row) {
  return `${row.kind}\u0000${row.name ?? ""}`;
}

/**
 * Splits the findings into what stays counted and what a marker silences.
 * `invalid` carries the markers that are not usable: a missing or weak reason,
 * one that binds to nothing (a stale marker), or one whose line holds findings
 * from two different attributes (which of them did the author mean?).
 */
export function bindLiteralMarkers(findings, markers) {
  const byLine = new Map();
  for (const row of findings) {
    const list = byLine.get(row.line) ?? [];
    list.push(row);
    byLine.set(row.line, list);
  }
  const silenced = new Set();
  const ignored = [];
  const invalid = [];
  for (const marker of markers) {
    const problem = literalReasonProblem(marker.reason);
    if (problem) {
      invalid.push({ line: marker.line, problem });
      continue;
    }
    // `// i18n-literal:` inside JSX children is text on screen, not a comment.
    if ((byLine.get(marker.line) ?? []).some((row) => row.kind === "text")) {
      invalid.push({ line: marker.line, problem: "in-jsx-text" });
      continue;
    }
    const rows = byLine.get(marker.boundLine) ?? [];
    if (rows.length === 0) {
      invalid.push({ line: marker.line, problem: "orphan" });
      continue;
    }
    const groups = new Map();
    for (const row of rows) groups.set(groupKey(row), [...(groups.get(groupKey(row)) ?? []), row]);
    if (groups.size > 1) {
      invalid.push({ line: marker.line, problem: "ambiguous" });
      continue;
    }
    for (const row of rows) {
      silenced.add(row);
      ignored.push({ ...row, reason: marker.reason, markerLine: marker.line });
    }
  }
  return {
    kept: findings.filter((row) => !silenced.has(row)),
    ignored: ignored.sort((left, right) => left.line - right.line),
    invalid,
  };
}

/** A property name, or null when it is computed. Shorthand assignments are a
 * different node type and never reach here. */
function propertyKey(node) {
  if (ts.isIdentifier(node) || ts.isStringLiteral(node)) return node.text;
  return null;
}

/** The value with `as`, `satisfies` and parentheses peeled off. */
function unwrap(node) {
  let current = node;
  for (;;) {
    if (ts.isAsExpression(current) || ts.isParenthesizedExpression(current)) current = current.expression;
    else if (typeof ts.isSatisfiesExpression === "function" && ts.isSatisfiesExpression(current))
      current = current.expression;
    else return current;
  }
}

/** The strings one property value holds, as the user would read them. */
function valueTexts(initializer) {
  return expressionTexts(initializer);
}

/** The justification written above a property, or null when there is none. The
 * reason may not be empty - an empty pragma hides nothing. */
function contentReason(node, body) {
  // A pragma above the property counts, and so does one above the declaration
  // that holds it: `// i18n-content:` over a whole table is easier to keep than
  // one per entry. Every use is still listed in the report, so nothing hides.
  for (let current = node; current && !ts.isSourceFile(current); current = current.parent) {
    for (const range of ts.getLeadingCommentRanges(body, current.getFullStart()) ?? []) {
      const match = CONTENT_PRAGMA.exec(body.slice(range.pos, range.end));
      if (match) return match[1].trim();
    }
  }
  return null;
}

/**
 * Every user-visible string an object literal or a `.ts` module holds.
 *
 * `counted` holds what the baseline measures, `excluded` what a `i18n-content:`
 * pragma or a content whole-file rule keeps out (with its reason), and
 * `unmarked` the matching keys whose value has no Turkish letter - the info
 * list that shows what the narrow rule lets through.
 */
export function scanObjects(name, body) {
  const counted = [];
  const excluded = [];
  const unmarked = [];
  const contentFile = CONTENT_PATHS.find((entry) => entry.path === name);
  if (contentFile) return { contentFile, counted, excluded, unmarked };

  const kind = name.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const source = ts.createSourceFile(name, body, ts.ScriptTarget.Latest, true, kind);
  const lineOf = (node) => source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
  const visit = (node) => {
    if (ts.isPropertyAssignment(node)) {
      const key = propertyKey(node.name);
      if (key && OBJECT_KEYS.includes(key)) {
        const reason = contentReason(node, body);
        for (const found of valueTexts(node.initializer)) {
          const row = { line: lineOf(node), key, text: found.text, via: found.via };
          if (reason) excluded.push({ ...row, reason });
          else if (TURKISH.test(row.text)) counted.push(row);
          else unmarked.push(row);
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return { contentFile: null, counted, excluded, unmarked };
}
