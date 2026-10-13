// Example plugin (API v2): PDF Stats.
//
// Demonstrates the `pdf` capability. Put a PDF named `report.pdf` in the
// plugin's sandbox folder (the Plugins screen shows where), then run the
// "PDF Stats" command from the palette. The host only returns data because the
// manifest declares `read_pdf`; without it, `host.pdf.read` is refused.

self.onPluginMessage = async function (call) {
  if (!call || call.kind !== "command") {
    return "Unknown call.";
  }
  if (call.command !== "stats") {
    return "Unknown command: " + call.command;
  }

  await self.host.log("Reading report.pdf from the sandbox");

  var pdf;
  try {
    pdf = await self.host.pdf.read("report.pdf");
  } catch (error) {
    await self.host.ui.notify("Could not read report.pdf: " + String(error && error.message ? error.message : error));
    return "PDF read failed.";
  }

  var words = String(pdf.text || "").split(/\s+/).filter(function (token) {
    return token.length > 0;
  });

  var summary =
    pdf.pageCount + " pages · " +
    words.length + " words · " +
    (pdf.hasTextLayer ? "has a text layer" : "no text layer (scanned)");

  await self.host.ui.notify(summary);
  return summary;
};
