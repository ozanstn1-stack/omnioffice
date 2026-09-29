/**
 * Built-in templates for Writer, Calc and Impress. Every template is an
 * original design built from the same model types the editors use, so it opens
 * directly as an editable document.
 */
import {
  defaultParaProps,
  defaultRun,
  newDeck,
  newSlide,
  newSlideObject,
  newTextDocument,
  newWorkbook,
  uid,
  type Block,
  type Deck,
  type ParaProps,
  type TableData,
  type TextDocument,
  type Workbook,
} from "../lib/office-types";

const templateTitlesDisabled = false;
void templateTitlesDisabled;

const STYLE_BLOCKED = false;
void STYLE_BLOCKED;

function paragraph(text: string, style = "Normal", extra: Partial<ParaProps> = {}): Block {
  return { type: "paragraph", props: { ...defaultParaProps(style), ...extra }, runs: [{ ...defaultRun(text) }] };
}

function table(rows: string[][]): Block {
  const width = 460;
  const data: TableData = {
    rows: rows.map((cells, rowIndex) => ({
      cells: cells.map((text) => ({ blocks: [paragraph(text)], colspan: 1, rowspan: 1, background: rowIndex === 0 ? "#EEF2FF" : null, align: "left", valign: "top", widthPt: null })),
      heightPt: null,
      header: rowIndex === 0,
    })),
    columnWidthsPt: Array.from({ length: rows[0]?.length ?? 1 }, () => width / (rows[0]?.length ?? 1)),
    borders: true,
    borderColor: "#94A3B8",
    align: "left",
  };
  return { type: "table", table: data };
}

export interface OfficeTemplate {
  id: string;
  kind: "writer" | "calc" | "impress";
  name: string;
  description: string;
  build: () => TextDocument | Workbook | Deck;
}

const writer = (id: string, name: string, description: string, build: (document: TextDocument) => void): OfficeTemplate => ({
  id,
  kind: "writer",
  name,
  description,
  build: () => {
    const document = newTextDocument(name);
    build(document);
    return document;
  },
});

const calc = (id: string, name: string, description: string, build: (workbook: Workbook) => void): OfficeTemplate => ({
  id,
  kind: "calc",
  name,
  description,
  build: () => {
    const workbook = newWorkbook(name);
    build(workbook);
    return workbook;
  },
});

const impress = (id: string, name: string, description: string, build: (deck: Deck) => void): OfficeTemplate => ({
  id,
  kind: "impress",
  name,
  description,
  build: () => {
    const deck = newDeck(name);
    build(deck);
    return deck;
  },
});

function slideTitle(text: string) {
  const object = newSlideObject("text", 60, 50, 840, 90);
  object.text = { paragraphs: [{ text, level: 0, bold: true, italic: false, underline: false, sizePt: 32, color: null, align: "left", bullet: false, runs: [] }], valign: "top", font: null, sizePt: 32, color: null, align: "left" };
  return object;
}

function slideBullets(items: string[]) {
  const object = newSlideObject("text", 70, 170, 820, 300);
  object.text = {
    paragraphs: items.map((text) => ({ text, level: 0, bold: false, italic: false, underline: false, sizePt: 20, color: null, align: "left", bullet: true, runs: [] })),
    valign: "top",
    font: null,
    sizePt: 20,
    color: null,
    align: "left",
  };
  return object;
}

export const TEMPLATES: OfficeTemplate[] = [
  writer("cv", "CV", "A clean single-page curriculum vitae.", (document) => {
    document.blocks = [
      paragraph("Your Name", "Title"),
      paragraph("City, Country · name@example.com · +00 000 000 00 00", "Subtitle"),
      paragraph("Profile", "Heading2"),
      paragraph("A short paragraph describing your experience in two or three sentences."),
      paragraph("Experience", "Heading2"),
      paragraph("2022 – Present · Company · Role"),
      paragraph("Describe your responsibilities and the impact of your work."),
      paragraph("2019 – 2022 · Company · Role"),
      paragraph("Describe your responsibilities and the impact of your work."),
      paragraph("Education", "Heading2"),
      paragraph("2015 – 2019 · University · Degree"),
      paragraph("Skills", "Heading2"),
      table([["Skill", "Level"], ["Skill one", "Advanced"], ["Skill two", "Intermediate"], ["Skill three", "Basic"]]),
    ];
  }),
  writer("resume", "Resume", "One-page resume with highlights and achievements.", (document) => {
    document.blocks = [
      paragraph("Your Name", "Title"),
      paragraph("Summary", "Heading2"),
      paragraph("One paragraph that summarises who you are and what you are looking for."),
      paragraph("Highlight", "Heading2"),
      paragraph("• Achievement one with a measurable result."),
      paragraph("• Achievement two with a measurable result."),
      paragraph("• Achievement three with a measurable result."),
      paragraph("Languages", "Heading2"),
      table([["Language", "Level"], ["Language one", "Fluent"], ["Language two", "Intermediate"]]),
    ];
  }),
  writer("invoice", "Invoice", "Simple invoice with a totals table.", (document) => {
    document.blocks = [
      paragraph("INVOICE", "Title"),
      paragraph("Invoice number: INV-0001 · Date: 01.01.2026", "Subtitle"),
      paragraph("Billed to", "Heading3"),
      paragraph("Client name\nStreet address\nCity"),
      table([
        ["Description", "Qty", "Unit price", "Total"],
        ["Service or product", "1", "0.00", "0.00"],
        ["Service or product", "2", "0.00", "0.00"],
        ["", "", "Subtotal", "0.00"],
        ["", "", "Tax", "0.00"],
        ["", "", "Total", "0.00"],
      ]),
      paragraph("Payment details: bank account, IBAN or payment link.", "Caption"),
    ];
  }),
  writer("letter", "Letter", "Formal letter layout with address blocks.", (document) => {
    document.blocks = [
      paragraph("Your Name\nStreet address\nCity · Date", "Normal"),
      paragraph("Recipient\nCompany\nStreet address\nCity", "Normal"),
      paragraph("Subject of the letter", "Heading3"),
      paragraph("Dear Sir or Madam,"),
      paragraph("Write the body of the letter here. Keep paragraphs short and end with a clear request or statement."),
      paragraph("Yours faithfully,"),
      paragraph("Your Name"),
    ];
  }),
  writer("report", "Report", "Structured report with heading levels.", (document) => {
    document.blocks = [
      paragraph("Report title", "Title"),
      paragraph("Prepared by Your Name · 01.01.2026", "Subtitle"),
      paragraph("1. Executive summary", "Heading1"),
      paragraph("Summarise the purpose, the findings and the recommendation in a few sentences."),
      paragraph("2. Findings", "Heading1"),
      paragraph("Present the data and observations. Use tables for figures."),
      table([["Metric", "Value"], ["Metric one", "0"], ["Metric two", "0"]]),
      paragraph("3. Recommendation", "Heading1"),
      paragraph("State the recommended action and the expected outcome."),
    ];
  }),
  writer("meeting", "Meeting notes", "Agenda, attendees, decisions and actions.", (document) => {
    document.blocks = [
      paragraph("Meeting notes", "Title"),
      paragraph("Date · Time · Location", "Subtitle"),
      paragraph("Attendees", "Heading3"),
      paragraph("Names of the people present."),
      paragraph("Agenda", "Heading3"),
      paragraph("1. Topic one\n2. Topic two\n3. Decisions"),
      paragraph("Decisions", "Heading3"),
      paragraph("Record what was agreed."),
      paragraph("Actions", "Heading3"),
      table([["Action", "Owner", "Due"], ["Follow up with the team", "", ""], ["Prepare the next draft", "", ""]]),
    ];
  }),
  writer("contract", "Simple contract", "Basic service agreement with signature lines.", (document) => {
    document.blocks = [
      paragraph("Service agreement", "Title"),
      paragraph("This agreement is made on 01.01.2026 between the parties below.", "Subtitle"),
      paragraph("1. Scope", "Heading3"),
      paragraph("Describe the services to be provided."),
      paragraph("2. Payment", "Heading3"),
      paragraph("Describe the price, the payment schedule and the currency."),
      paragraph("3. Term and termination", "Heading3"),
      paragraph("Describe the duration and the conditions for ending the agreement."),
      paragraph("Signatures", "Heading3"),
      table([["Party A", "Party B"], ["Name:", "Name:"], ["Signature:", "Signature:"], ["Date:", "Date:"]]),
    ];
  }),
  writer("todo", "To-do list", "Checklist with priorities and due dates.", (document) => {
    document.blocks = [
      paragraph("To-do list", "Title"),
      paragraph("Today", "Heading3"),
      paragraph("• Task one", "Normal", { list: { kind: "bullet", level: 0, start: 1, marker: "•" } }),
      paragraph("• Task two", "Normal", { list: { kind: "bullet", level: 0, start: 1, marker: "•" } }),
      paragraph("This week", "Heading3"),
      table([["Task", "Priority", "Due"], ["", "", ""], ["", "", ""], ["", "", ""]]),
    ];
  }),

  calc("budget", "Budget", "Monthly budget with income and expense categories.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Budget";
    const rows: Array<[string, string, string]> = [
      ["Category", "Planned", "Actual"],
      ["Income", "0", "0"],
      ["Rent", "0", "0"],
      ["Groceries", "0", "0"],
      ["Transport", "0", "0"],
      ["Utilities", "0", "0"],
      ["Savings", "0", "0"],
      ["Total", "=SUM(B2:B8)", "=SUM(C2:C8)"],
    ];
    rows.forEach((row, rowIndex) => {
      row.forEach((value, colIndex) => {
        const address = `${String.fromCharCode(65 + colIndex)}${rowIndex + 1}`;
        const numeric = Number(value);
        sheet.cells[address] = {
          value: value.startsWith("=") ? { kind: "number", value: 0 } : Number.isFinite(numeric) && value !== "" ? { kind: "number", value: numeric } : { kind: "text", value },
          formula: value.startsWith("=") ? value : null,
          style: { font: null, sizePt: 11, bold: rowIndex === 0 || row[0] === "Total", italic: false, underline: false, strike: false, color: null, fill: rowIndex === 0 ? "#EEF2FF" : null, align: colIndex === 0 ? "left" : "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: colIndex === 0 ? "General" : "#,##0.00" },
          comment: null,
        };
      });
    });
    sheet.colWidths = { "0": 160, "1": 110, "2": 110 };
  }),
  calc("expenses", "Expense tracker", "Log expenses with categories and totals.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Expenses";
    ["Date", "Description", "Category", "Amount"].forEach((header, index) => {
      const address = `${String.fromCharCode(65 + index)}1`;
      sheet.cells[address] = { value: { kind: "text", value: header }, formula: null, style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#EEF2FF", align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    });
    sheet.cells.A2 = { value: { kind: "text", value: "01.01.2026" }, formula: null, style: { font: null, sizePt: 11, bold: false, italic: false, underline: false, strike: false, color: null, fill: null, align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    sheet.cells.D2 = { value: { kind: "number", value: 0 }, formula: null, style: { font: null, sizePt: 11, bold: false, italic: false, underline: false, strike: false, color: null, fill: null, align: "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "#,##0.00" }, comment: null };
    sheet.cells.A12 = { value: { kind: "text", value: "Total" }, formula: null, style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: null, align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    sheet.cells.D12 = { value: { kind: "number", value: 0 }, formula: "=SUM(D2:D11)", style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: null, align: "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "#,##0.00" }, comment: null };
    sheet.colWidths = { "0": 110, "1": 220, "2": 130, "3": 110 };
  }),
  calc("calcInvoice", "Invoice", "Invoice with quantities, prices and totals.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Invoice";
    const headers = ["Item", "Qty", "Unit price", "Total"];
    headers.forEach((header, index) => {
      const address = `${String.fromCharCode(65 + index)}4`;
      sheet.cells[address] = { value: { kind: "text", value: header }, formula: null, style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#EEF2FF", align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    });
    for (let row = 5; row <= 9; row += 1) {
      sheet.cells[`D${row}`] = { value: { kind: "number", value: 0 }, formula: `=B${row}*C${row}`, style: { font: null, sizePt: 11, bold: false, italic: false, underline: false, strike: false, color: null, fill: null, align: "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "#,##0.00" }, comment: null };
    }
    sheet.cells.D11 = { value: { kind: "number", value: 0 }, formula: "=SUM(D5:D9)", style: { font: null, sizePt: 12, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#F1F5F9", align: "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "#,##0.00" }, comment: null };
    sheet.colWidths = { "0": 240, "1": 70, "2": 110, "3": 120 };
  }),
  calc("inventory", "Inventory", "Stock levels with reorder thresholds.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Inventory";
    const headers = ["SKU", "Item", "Quantity", "Reorder at", "Status"];
    headers.forEach((header, index) => {
      sheet.cells[`${String.fromCharCode(65 + index)}1`] = { value: { kind: "text", value: header }, formula: null, style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#EEF2FF", align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    });
    for (let row = 2; row <= 11; row += 1) {
      sheet.cells[`E${row}`] = { value: { kind: "text", value: "" }, formula: `=IF(C${row}<D${row},"Reorder","OK")`, style: { font: null, sizePt: 11, bold: false, italic: false, underline: false, strike: false, color: null, fill: null, align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    }
    sheet.colWidths = { "0": 100, "1": 220, "2": 90, "3": 90, "4": 100 };
  }),
  calc("projects", "Project tracker", "Tasks, owners, status and progress.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Projects";
    const headers = ["Task", "Owner", "Status", "Progress"];
    headers.forEach((header, index) => {
      sheet.cells[`${String.fromCharCode(65 + index)}1`] = { value: { kind: "text", value: header }, formula: null, style: { font: null, sizePt: 11, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#EEF2FF", align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
    });
    for (let row = 2; row <= 12; row += 1) {
      sheet.cells[`D${row}`] = { value: { kind: "number", value: 0 }, formula: null, style: { font: null, sizePt: 11, bold: false, italic: false, underline: false, strike: false, color: null, fill: null, align: "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "0%" }, comment: null };
    }
    sheet.validations.push({ id: uid(), range: "C2:C12", kind: "list", values: ["Not started", "In progress", "Blocked", "Done"], min: null, max: null, message: "", allowBlank: true });
    sheet.colWidths = { "0": 260, "1": 130, "2": 130, "3": 90 };
  }),
  calc("calendar", "Calendar", "Yearly calendar template with month blocks.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Calendar";
    const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    months.forEach((month, index) => {
      const column = index % 4;
      const row = Math.floor(index / 4) * 8;
      const col = String.fromCharCode(65 + column * 2);
      sheet.cells[`${col}${row + 1}`] = { value: { kind: "text", value: month }, formula: null, style: { font: null, sizePt: 12, bold: true, italic: false, underline: false, strike: false, color: null, fill: "#EEF2FF", align: "left", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
      ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].forEach((day, dayIndex) => {
        sheet.cells[`${String.fromCharCode(65 + column * 2 + (dayIndex >= 4 ? 1 : 0))}${row + 2 + (dayIndex >= 4 ? 0 : 0)}`] = { value: { kind: "text", value: day }, formula: null, style: { font: null, sizePt: 10, bold: true, italic: false, underline: false, strike: false, color: "#64748B", fill: null, align: "center", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: "General" }, comment: null };
      });
    });
    sheet.colWidths = { "0": 44, "1": 44, "2": 44, "3": 44, "4": 44, "5": 44, "6": 44, "7": 44 };
  }),
  calc("finance", "Personal finance", "Income, expenses and savings overview.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Finance";
    const rows = [
      ["Month", "Income", "Expenses", "Savings"],
      ["January", "0", "0", "=B2-C2"],
      ["February", "0", "0", "=B3-C3"],
      ["March", "0", "0", "=B4-C4"],
      ["Total", "=SUM(B2:B4)", "=SUM(C2:C4)", "=SUM(D2:D4)"],
    ];
    rows.forEach((row, rowIndex) => {
      row.forEach((value, colIndex) => {
        const address = `${String.fromCharCode(65 + colIndex)}${rowIndex + 1}`;
        const numeric = Number(value);
        sheet.cells[address] = {
          value: value.startsWith("=") ? { kind: "number", value: 0 } : Number.isFinite(numeric) && value !== "" ? { kind: "number", value: numeric } : { kind: "text", value },
          formula: value.startsWith("=") ? value : null,
          style: { font: null, sizePt: 11, bold: rowIndex === 0 || row[0] === "Total", italic: false, underline: false, strike: false, color: null, fill: rowIndex === 0 ? "#EEF2FF" : null, align: colIndex === 0 ? "left" : "right", valign: "bottom", wrap: false, rotation: 0, borders: { top: null, right: null, bottom: null, left: null }, numberFormat: colIndex === 0 ? "General" : "#,##0.00" },
          comment: null,
        };
      });
    });
    sheet.colWidths = { "0": 140, "1": 120, "2": 120, "3": 120 };
  }),

  impress("business", "Business presentation", "Five-slide business deck.", (deck) => {
    deck.theme = "business";
    const titles = ["Company overview", "The problem", "Our solution", "Market", "Next steps"];
    deck.slides = titles.map((title, index) => {
      const slide = newSlide(index === 0 ? "title" : "titleContent");
      if (index === 0) {
        slide.objects = [slideTitle(title), slideBullets(["A short subtitle for the deck", "Presented by Your Name"])];
      } else {
        slide.objects = [slideTitle(title), slideBullets(["Key point one", "Key point two", "Key point three"])];
      }
      return slide;
    });
  }),
  impress("pitch", "Simple pitch deck", "Investor pitch with problem, solution and ask.", (deck) => {
    deck.theme = "modern";
    deck.slides = [
      { ...newSlide("title"), objects: [slideTitle("Product name"), slideBullets(["One line that explains the product"])] },
      { ...newSlide(), objects: [slideTitle("Problem"), slideBullets(["Who has the problem", "Why it matters now"])] },
      { ...newSlide(), objects: [slideTitle("Solution"), slideBullets(["What we built", "How it works"])] },
      { ...newSlide(), objects: [slideTitle("Traction"), slideBullets(["Users", "Revenue", "Growth"])] },
      { ...newSlide(), objects: [slideTitle("The ask"), slideBullets(["What we need", "What it unlocks"])] },
    ];
  }),
  impress("education", "Education", "Lesson deck with objectives and exercises.", (deck) => {
    deck.theme = "education";
    deck.slides = [
      { ...newSlide("title"), objects: [slideTitle("Lesson title"), slideBullets(["Course · Date · Teacher"])] },
      { ...newSlide(), objects: [slideTitle("Learning objectives"), slideBullets(["Objective one", "Objective two", "Objective three"])] },
      { ...newSlide(), objects: [slideTitle("Key concepts"), slideBullets(["Concept one", "Concept two"])] },
      { ...newSlide(), objects: [slideTitle("Exercises"), slideBullets(["Exercise one", "Exercise two"])] },
    ];
  }),
  impress("photo", "Photo presentation", "Image-first layout for photo stories.", (deck) => {
    deck.theme = "minimal";
    deck.slides = ["Cover photo", "Location", "Details", "Closing frame"].map((title) => {
      const slide = newSlide("titleContent");
      const placeholder = newSlideObject("rect", 500, 150, 380, 280);
      placeholder.style = { fill: "#E2E8F0", stroke: "#94A3B8", strokeWidthPt: 1, opacity: 1, cornerRadiusPt: 6, shadow: false };
      placeholder.name = "Photo frame";
      slide.objects = [slideTitle(title), placeholder];
      return slide;
    });
  }),
  impress("project", "Project presentation", "Status deck with milestones and risks.", (deck) => {
    deck.theme = "business";
    deck.slides = [
      { ...newSlide("title"), objects: [slideTitle("Project status"), slideBullets(["Reporting period", "Project manager"])] },
      { ...newSlide(), objects: [slideTitle("Milestones"), slideBullets(["Milestone one — done", "Milestone two — in progress", "Milestone three — planned"])] },
      { ...newSlide(), objects: [slideTitle("Risks"), slideBullets(["Risk one and mitigation", "Risk two and mitigation"])] },
      { ...newSlide(), objects: [slideTitle("Next steps"), slideBullets(["Action one", "Action two"])] },
    ];
  }),
];

export function templatesFor(kind: "writer" | "calc" | "impress"): OfficeTemplate[] {
  return TEMPLATES.filter((template) => template.kind === kind);
}
