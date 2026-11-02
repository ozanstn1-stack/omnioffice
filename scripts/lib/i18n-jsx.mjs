// The JSX scanner behind `scripts/i18n-audit.mjs`.
//
// It is a pure function of (file name, source text): no file system, no config,
// so the audit keeps its shape and this part can be tested on inline snippets.
//
// What counts as user-visible text:
//   * a JSX text node with at least two letters ("—", "·", "1", "/" are
//     separators the translation table never needs),
//   * one of the ATTRIBUTES or PROPS whose value is a literal - a plain string,
//     a braced string or a template without substitutions. A `t("…")` call, a
//     variable or a conditional is not a hardcoded string and is skipped.
//
// A template with `${…}` is dynamic and stays out; so does a conditional's
// branches (`label={flag ? "A" : "B"}`) - a known gap, documented in docs/i18n.md.
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
  }
  void source;
  return null;
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
  const visit = (node) => {
    if (ts.isJsxText(node)) {
      const text = node.text.replace(/\s+/g, " ").trim();
      if (PROSE.test(text)) texts.push({ line: lineOf(node), text });
    } else if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(source);
      const value = literalText(node.initializer, source);
      const text = value?.trim() ?? "";
      if (text && PROSE.test(text)) {
        if (ATTRIBUTES.includes(name)) attributes.push({ line: lineOf(node), name, text });
        else if (PROPS.includes(name)) props.push({ line: lineOf(node), name, text });
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return { texts, attributes, props };
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

/** Object keys whose value a user reads. `name`, `value` and `message` stay out
 * on purpose: they carry identifiers, product names and developer errors. */
export const OBJECT_KEYS = ["label", "title", "description", "hint", "subtitle", "placeholder", "tooltip"];

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
  const node = unwrap(initializer);
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return [node.text];
  if (ts.isTemplateExpression(node)) {
    const text = [node.head.text, ...node.templateSpans.map((span) => `\${…}${span.literal.text}`)].join("");
    return [text];
  }
  if (ts.isConditionalExpression(node)) {
    return [...valueTexts(node.whenTrue), ...valueTexts(node.whenFalse)];
  }
  return [];
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
        for (const text of valueTexts(node.initializer)) {
          const row = { line: lineOf(node), key, text };
          if (reason) excluded.push({ ...row, reason });
          else if (TURKISH.test(text)) counted.push(row);
          else unmarked.push(row);
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return { contentFile: null, counted, excluded, unmarked };
}
