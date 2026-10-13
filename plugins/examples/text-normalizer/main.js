// Example plugin (API v2): Text Normalizer.
//
// Demonstrates the `batch-step` capability. The manifest exposes a batch step
// ("normalize") to the recipe engine; the step id doubles as the command id, so
// this single handler serves both the palette command and the recipe engine.
//
// From the palette the plugin normalizes the active document. As a recipe step
// the host passes `params.input = { name, text }` and the plugin returns the
// transformed text.

function normalize(text) {
  return String(text || "")
    .replace(/\r\n?/g, "\n")
    .replace(/[ \t]+/g, " ")
    .replace(/ *\n */g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

self.onPluginMessage = async function (call) {
  if (!call || call.kind !== "command") {
    return "Unknown call.";
  }
  if (call.command !== "normalize") {
    return "Unknown command: " + call.command;
  }

  var input = call.params && call.params.input;

  // Batch step: transform the provided text and return it for the next step.
  if (input && typeof input.text === "string") {
    return { name: String(input.name || "input.txt"), text: normalize(input.text) };
  }

  // Palette command: normalize the active document through the document API.
  var text = String((await self.host.doc.getText("document")) || "");
  var out = normalize(text);
  await self.host.ui.notify("Normalized " + text.length + " → " + out.length + " characters.");
  return out.slice(0, 500);
};
