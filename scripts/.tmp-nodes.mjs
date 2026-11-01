import { readFileSync } from "node:fs";
import ts from "typescript";
const file = process.argv[2];
const body = readFileSync(file, "utf8");
const sf = ts.createSourceFile(file, body, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
const visit = (node) => {
  if (ts.isJsxText(node)) {
    const text = node.text.replace(/\s+/g, " ").trim();
    if (/[A-Za-zÇĞİÖŞÜçğıöşü]{2,}/.test(text)) {
      const { line } = sf.getLineAndCharacterOfPosition(node.getStart(sf));
      console.log(`TEXT ${line + 1}: ${JSON.stringify(text)}`);
    }
  }
  if (ts.isJsxAttribute(node) && node.initializer && ts.isStringLiteral(node.initializer)) {
    const name = node.name.getText(sf);
    const value = node.initializer.text.trim();
    if (["alt", "aria-label", "title", "placeholder"].includes(name) && /[A-Za-zÇĞİÖŞÜçğıöşü]{2,}/.test(value)) {
      const { line } = sf.getLineAndCharacterOfPosition(node.getStart(sf));
      console.log(`ATTR ${line + 1} ${name}: ${JSON.stringify(value)}`);
    }
  }
  ts.forEachChild(node, visit);
};
visit(sf);
