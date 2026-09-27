// Sample plugin: Word Frequency Counter.
//
// This file runs inside the sandboxed Web Worker. It only declares the
// read_document permission, so the host refuses every other capability.
// `self.host.*` is provided by the worker bootstrap; `self.onPluginMessage`
// is called by the host when the user runs one of the manifest commands.

self.onPluginMessage = async function (call) {
  if (!call || call.kind !== "command") {
    return "Unknown call.";
  }
  if (call.command !== "count") {
    return "Unknown command: " + call.command;
  }

  await self.host.log("Reading the active document");

  var text = String((await self.host.doc.getText("document")) || "");
  var words = text.toLowerCase().match(/[\p{L}\p{N}][\p{L}\p{N}'-]*/gu) || [];

  var counts = Object.create(null);
  for (var i = 0; i < words.length; i += 1) {
    var word = words[i];
    counts[word] = (counts[word] || 0) + 1;
  }

  var top = Object.keys(counts)
    .map(function (word) {
      return { word: word, count: counts[word] };
    })
    .sort(function (a, b) {
      return b.count - a.count || a.word.localeCompare(b.word);
    })
    .slice(0, 10);

  if (top.length === 0) {
    await self.host.ui.notify("No words found in the active document.");
    return "0 words";
  }

  var lines = top.map(function (entry, index) {
    return index + 1 + ". " + entry.word + " (" + entry.count + ")";
  });
  await self.host.ui.notify(top.length + " words · " + words.length + " total\n" + lines.join("\n"));
  return words.length + " words; top: " + top.map(function (entry) { return entry.word; }).join(", ");
};
