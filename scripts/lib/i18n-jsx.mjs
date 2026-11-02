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
