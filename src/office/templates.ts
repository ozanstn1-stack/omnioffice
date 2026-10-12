/**
 * Built-in templates for Writer, Calc and Impress. Every template is an
 * original design built from the same model types the editors use, so it opens
 * directly as an editable document.
 */
import {
  defaultCellStyle,
  defaultParaProps,
  defaultRun,
  newDeck,
  newSlide,
  newSlideObject,
  newTextDocument,
  newWorkbook,
  uid,
  type Block,
  type Cell,
  type CellStyle,
  type Deck,
  type ParaProps,
  type Run,
  type TableData,
  type TextDocument,
  type Workbook,
} from "../lib/office-types";

const templateTitlesDisabled = false;
void templateTitlesDisabled;

const STYLE_BLOCKED = false;
void STYLE_BLOCKED;

function paragraph(text: string, style = "Normal", extra: Partial<ParaProps> = {}, run: Partial<Run> = {}): Block {
  return {
    type: "paragraph",
    props: { ...defaultParaProps(style), ...extra },
    runs: [{ ...defaultRun(text), ...run }],
  };
}

function table(rows: string[][]): Block {
  const width = 460;
  const data: TableData = {
    rows: rows.map((cells, rowIndex) => ({
      cells: cells.map((text) => ({
        blocks: [paragraph(text)],
        colspan: 1,
        rowspan: 1,
        background: rowIndex === 0 ? "#EEF2FF" : null,
        align: "left",
        valign: "top",
        widthPt: null,
      })),
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
  /** Content language when it is not English; such templates get a badge and lead for that UI language. */
  language?: "tr";
  build: () => TextDocument | Workbook | Deck;
}

const writer = (
  id: string,
  name: string,
  description: string,
  build: (document: TextDocument) => void,
): OfficeTemplate => ({
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
  object.text = {
    paragraphs: [
      {
        text,
        level: 0,
        bold: true,
        italic: false,
        underline: false,
        sizePt: 32,
        color: null,
        align: "left",
        bullet: false,
        runs: [],
      },
    ],
    valign: "top",
    font: null,
    sizePt: 32,
    color: null,
    align: "left",
  };
  return object;
}

function slideBullets(items: string[]) {
  const object = newSlideObject("text", 70, 170, 820, 300);
  object.text = {
    paragraphs: items.map((text) => ({
      text,
      level: 0,
      bold: false,
      italic: false,
      underline: false,
      sizePt: 20,
      color: null,
      align: "left",
      bullet: true,
      runs: [],
    })),
    valign: "top",
    font: null,
    sizePt: 20,
    color: null,
    align: "left",
  };
  return object;
}

/** Marks a template whose content is Turkish. */
const turkish = (template: OfficeTemplate): OfficeTemplate => ({ ...template, language: "tr" });

const bullet = (): Partial<ParaProps> => ({ list: { kind: "bullet", level: 0, start: 1, marker: "•" } });
const numbered = (): Partial<ParaProps> => ({ list: { kind: "number", level: 0, start: 1, marker: "1." } });

/**
 * Turkish lira amounts. Excel and LibreOffice put the quoted sign after the
 * number as Turkish usage does; Calc's own formatter shows it in front.
 */
const TRY_FORMAT = '#,##0.00 "₺"';

/**
 * One Calc cell: a number, a text, a formula (cached as 0 until the sheet
 * computes it, like the templates above) or null for an empty cell that only
 * carries a format.
 */
function sheetCell(content: string | number | null, style: Partial<CellStyle> = {}): Cell {
  const formula = typeof content === "string" && content.startsWith("=") ? content : null;
  return {
    value: formula
      ? { kind: "number", value: 0 }
      : typeof content === "number"
        ? { kind: "number", value: content }
        : content
          ? { kind: "text", value: content }
          : { kind: "empty" },
    formula,
    style: { ...defaultCellStyle(), ...style },
    comment: null,
  };
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
      table([
        ["Skill", "Level"],
        ["Skill one", "Advanced"],
        ["Skill two", "Intermediate"],
        ["Skill three", "Basic"],
      ]),
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
      table([
        ["Language", "Level"],
        ["Language one", "Fluent"],
        ["Language two", "Intermediate"],
      ]),
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
      table([
        ["Metric", "Value"],
        ["Metric one", "0"],
        ["Metric two", "0"],
      ]),
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
      table([
        ["Action", "Owner", "Due"],
        ["Follow up with the team", "", ""],
        ["Prepare the next draft", "", ""],
      ]),
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
      table([
        ["Party A", "Party B"],
        ["Name:", "Name:"],
        ["Signature:", "Signature:"],
        ["Date:", "Date:"],
      ]),
    ];
  }),
  writer("todo", "To-do list", "Checklist with priorities and due dates.", (document) => {
    document.blocks = [
      paragraph("To-do list", "Title"),
      paragraph("Today", "Heading3"),
      paragraph("• Task one", "Normal", { list: { kind: "bullet", level: 0, start: 1, marker: "•" } }),
      paragraph("• Task two", "Normal", { list: { kind: "bullet", level: 0, start: 1, marker: "•" } }),
      paragraph("This week", "Heading3"),
      table([
        ["Task", "Priority", "Due"],
        ["", "", ""],
        ["", "", ""],
        ["", "", ""],
      ]),
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
          value: value.startsWith("=")
            ? { kind: "number", value: 0 }
            : Number.isFinite(numeric) && value !== ""
              ? { kind: "number", value: numeric }
              : { kind: "text", value },
          formula: value.startsWith("=") ? value : null,
          style: {
            font: null,
            sizePt: 11,
            bold: rowIndex === 0 || row[0] === "Total",
            italic: false,
            underline: false,
            strike: false,
            color: null,
            fill: rowIndex === 0 ? "#EEF2FF" : null,
            align: colIndex === 0 ? "left" : "right",
            valign: "bottom",
            wrap: false,
            rotation: 0,
            borders: { top: null, right: null, bottom: null, left: null },
            numberFormat: colIndex === 0 ? "General" : "#,##0.00",
          },
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
      sheet.cells[address] = {
        value: { kind: "text", value: header },
        formula: null,
        style: {
          font: null,
          sizePt: 11,
          bold: true,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: "#EEF2FF",
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
    });
    sheet.cells.A2 = {
      value: { kind: "text", value: "01.01.2026" },
      formula: null,
      style: {
        font: null,
        sizePt: 11,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        fill: null,
        align: "left",
        valign: "bottom",
        wrap: false,
        rotation: 0,
        borders: { top: null, right: null, bottom: null, left: null },
        numberFormat: "General",
      },
      comment: null,
    };
    sheet.cells.D2 = {
      value: { kind: "number", value: 0 },
      formula: null,
      style: {
        font: null,
        sizePt: 11,
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        fill: null,
        align: "right",
        valign: "bottom",
        wrap: false,
        rotation: 0,
        borders: { top: null, right: null, bottom: null, left: null },
        numberFormat: "#,##0.00",
      },
      comment: null,
    };
    sheet.cells.A12 = {
      value: { kind: "text", value: "Total" },
      formula: null,
      style: {
        font: null,
        sizePt: 11,
        bold: true,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        fill: null,
        align: "left",
        valign: "bottom",
        wrap: false,
        rotation: 0,
        borders: { top: null, right: null, bottom: null, left: null },
        numberFormat: "General",
      },
      comment: null,
    };
    sheet.cells.D12 = {
      value: { kind: "number", value: 0 },
      formula: "=SUM(D2:D11)",
      style: {
        font: null,
        sizePt: 11,
        bold: true,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        fill: null,
        align: "right",
        valign: "bottom",
        wrap: false,
        rotation: 0,
        borders: { top: null, right: null, bottom: null, left: null },
        numberFormat: "#,##0.00",
      },
      comment: null,
    };
    sheet.colWidths = { "0": 110, "1": 220, "2": 130, "3": 110 };
  }),
  calc("calcInvoice", "Invoice", "Invoice with quantities, prices and totals.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Invoice";
    const headers = ["Item", "Qty", "Unit price", "Total"];
    headers.forEach((header, index) => {
      const address = `${String.fromCharCode(65 + index)}4`;
      sheet.cells[address] = {
        value: { kind: "text", value: header },
        formula: null,
        style: {
          font: null,
          sizePt: 11,
          bold: true,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: "#EEF2FF",
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
    });
    for (let row = 5; row <= 9; row += 1) {
      sheet.cells[`D${row}`] = {
        value: { kind: "number", value: 0 },
        formula: `=B${row}*C${row}`,
        style: {
          font: null,
          sizePt: 11,
          bold: false,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: null,
          align: "right",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "#,##0.00",
        },
        comment: null,
      };
    }
    sheet.cells.D11 = {
      value: { kind: "number", value: 0 },
      formula: "=SUM(D5:D9)",
      style: {
        font: null,
        sizePt: 12,
        bold: true,
        italic: false,
        underline: false,
        strike: false,
        color: null,
        fill: "#F1F5F9",
        align: "right",
        valign: "bottom",
        wrap: false,
        rotation: 0,
        borders: { top: null, right: null, bottom: null, left: null },
        numberFormat: "#,##0.00",
      },
      comment: null,
    };
    sheet.colWidths = { "0": 240, "1": 70, "2": 110, "3": 120 };
  }),
  calc("inventory", "Inventory", "Stock levels with reorder thresholds.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Inventory";
    const headers = ["SKU", "Item", "Quantity", "Reorder at", "Status"];
    headers.forEach((header, index) => {
      sheet.cells[`${String.fromCharCode(65 + index)}1`] = {
        value: { kind: "text", value: header },
        formula: null,
        style: {
          font: null,
          sizePt: 11,
          bold: true,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: "#EEF2FF",
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
    });
    for (let row = 2; row <= 11; row += 1) {
      sheet.cells[`E${row}`] = {
        value: { kind: "text", value: "" },
        formula: `=IF(C${row}<D${row},"Reorder","OK")`,
        style: {
          font: null,
          sizePt: 11,
          bold: false,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: null,
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
    }
    sheet.colWidths = { "0": 100, "1": 220, "2": 90, "3": 90, "4": 100 };
  }),
  calc("projects", "Project tracker", "Tasks, owners, status and progress.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Projects";
    const headers = ["Task", "Owner", "Status", "Progress"];
    headers.forEach((header, index) => {
      sheet.cells[`${String.fromCharCode(65 + index)}1`] = {
        value: { kind: "text", value: header },
        formula: null,
        style: {
          font: null,
          sizePt: 11,
          bold: true,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: "#EEF2FF",
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
    });
    for (let row = 2; row <= 12; row += 1) {
      sheet.cells[`D${row}`] = {
        value: { kind: "number", value: 0 },
        formula: null,
        style: {
          font: null,
          sizePt: 11,
          bold: false,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: null,
          align: "right",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "0%",
        },
        comment: null,
      };
    }
    sheet.validations.push({
      id: uid(),
      range: "C2:C12",
      kind: "list",
      values: ["Not started", "In progress", "Blocked", "Done"],
      min: null,
      max: null,
      message: "",
      allowBlank: true,
    });
    sheet.colWidths = { "0": 260, "1": 130, "2": 130, "3": 90 };
  }),
  calc("calendar", "Calendar", "Yearly calendar template with month blocks.", (workbook) => {
    const sheet = workbook.sheets[0];
    sheet.name = "Calendar";
    const months = [
      "January",
      "February",
      "March",
      "April",
      "May",
      "June",
      "July",
      "August",
      "September",
      "October",
      "November",
      "December",
    ];
    months.forEach((month, index) => {
      const column = index % 4;
      const row = Math.floor(index / 4) * 8;
      const col = String.fromCharCode(65 + column * 2);
      sheet.cells[`${col}${row + 1}`] = {
        value: { kind: "text", value: month },
        formula: null,
        style: {
          font: null,
          sizePt: 12,
          bold: true,
          italic: false,
          underline: false,
          strike: false,
          color: null,
          fill: "#EEF2FF",
          align: "left",
          valign: "bottom",
          wrap: false,
          rotation: 0,
          borders: { top: null, right: null, bottom: null, left: null },
          numberFormat: "General",
        },
        comment: null,
      };
      ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].forEach((day, dayIndex) => {
        sheet.cells[
          `${String.fromCharCode(65 + column * 2 + (dayIndex >= 4 ? 1 : 0))}${row + 2 + (dayIndex >= 4 ? 0 : 0)}`
        ] = {
          value: { kind: "text", value: day },
          formula: null,
          style: {
            font: null,
            sizePt: 10,
            bold: true,
            italic: false,
            underline: false,
            strike: false,
            color: "#64748B",
            fill: null,
            align: "center",
            valign: "bottom",
            wrap: false,
            rotation: 0,
            borders: { top: null, right: null, bottom: null, left: null },
            numberFormat: "General",
          },
          comment: null,
        };
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
          value: value.startsWith("=")
            ? { kind: "number", value: 0 }
            : Number.isFinite(numeric) && value !== ""
              ? { kind: "number", value: numeric }
              : { kind: "text", value },
          formula: value.startsWith("=") ? value : null,
          style: {
            font: null,
            sizePt: 11,
            bold: rowIndex === 0 || row[0] === "Total",
            italic: false,
            underline: false,
            strike: false,
            color: null,
            fill: rowIndex === 0 ? "#EEF2FF" : null,
            align: colIndex === 0 ? "left" : "right",
            valign: "bottom",
            wrap: false,
            rotation: 0,
            borders: { top: null, right: null, bottom: null, left: null },
            numberFormat: colIndex === 0 ? "General" : "#,##0.00",
          },
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
      {
        ...newSlide("title"),
        objects: [slideTitle("Product name"), slideBullets(["One line that explains the product"])],
      },
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
      {
        ...newSlide(),
        objects: [
          slideTitle("Learning objectives"),
          slideBullets(["Objective one", "Objective two", "Objective three"]),
        ],
      },
      { ...newSlide(), objects: [slideTitle("Key concepts"), slideBullets(["Concept one", "Concept two"])] },
      { ...newSlide(), objects: [slideTitle("Exercises"), slideBullets(["Exercise one", "Exercise two"])] },
    ];
  }),
  impress("photo", "Photo presentation", "Image-first layout for photo stories.", (deck) => {
    deck.theme = "minimal";
    deck.slides = ["Cover photo", "Location", "Details", "Closing frame"].map((title) => {
      const slide = newSlide("titleContent");
      const placeholder = newSlideObject("rect", 500, 150, 380, 280);
      placeholder.style = {
        fill: "#E2E8F0",
        stroke: "#94A3B8",
        strokeWidthPt: 1,
        opacity: 1,
        cornerRadiusPt: 6,
        shadow: false,
      };
      placeholder.name = "Photo frame";
      slide.objects = [slideTitle(title), placeholder];
      return slide;
    });
  }),
  impress("project", "Project presentation", "Status deck with milestones and risks.", (deck) => {
    deck.theme = "business";
    deck.slides = [
      {
        ...newSlide("title"),
        objects: [slideTitle("Project status"), slideBullets(["Reporting period", "Project manager"])],
      },
      {
        ...newSlide(),
        objects: [
          slideTitle("Milestones"),
          slideBullets(["Milestone one — done", "Milestone two — in progress", "Milestone three — planned"]),
        ],
      },
      {
        ...newSlide(),
        objects: [slideTitle("Risks"), slideBullets(["Risk one and mitigation", "Risk two and mitigation"])],
      },
      { ...newSlide(), objects: [slideTitle("Next steps"), slideBullets(["Action one", "Action two"])] },
    ];
  }),

  // Turkish templates: they lead the list when the interface is Turkish (see
  // `orderTemplates`) and keep their Turkish content in every UI language.
  turkish(
    writer(
      "trPetition",
      "Dilekçe",
      "Kurumlara başvuru için tarih, muhatap, imza, iletişim bilgileri ve ekler bölümü olan resmî dilekçe.",
      (document) => {
        document.blocks = [
          paragraph("…/…/20…", "Normal", { align: "right" }),
          paragraph(
            "……………………………… MÜDÜRLÜĞÜNE",
            "Normal",
            { align: "center", spaceBeforePt: 24, spaceAfterPt: 24 },
            { bold: true },
          ),
          paragraph(
            "……………………………… nedeniyle ……………………………………… konusunda kurumunuza başvuruda bulunmak istiyorum. Konuyla ilgili bilgi ve belgeler ekte sunulmuştur.",
            "Normal",
            { align: "justify", firstLinePt: 36 },
          ),
          paragraph("Gereğini bilgilerinize arz ederim.", "Normal", { firstLinePt: 36 }),
          paragraph("Ad Soyad\nİmza", "Normal", { align: "right", spaceBeforePt: 24 }),
          paragraph("T.C. Kimlik No: ………………………\nAdres: ………………………………………………………\nTelefon: ………………………", "Normal", {
            spaceBeforePt: 24,
          }),
          paragraph("Ekler:", "Normal", { spaceBeforePt: 12, spaceAfterPt: 4 }, { bold: true }),
          paragraph("………………………………………", "Normal", numbered()),
          paragraph("………………………………………", "Normal", numbered()),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trCv",
      "Özgeçmiş",
      "Kişisel bilgiler, eğitim, iş deneyimi, yetenekler, diller ve referanslarla sade bir özgeçmiş.",
      (document) => {
        document.blocks = [
          paragraph("Ad Soyad", "Title"),
          paragraph("Meslek / Unvan", "Subtitle"),
          paragraph("Kişisel Bilgiler", "Heading2"),
          paragraph(
            "Doğum Tarihi: GG.AA.YYYY\nTelefon: +90 5XX XXX XX XX\nE-posta: ad.soyad@ornek.com\nAdres: Mahalle, İlçe / İl",
          ),
          paragraph("Eğitim", "Heading2"),
          paragraph("2016 – 2020 · Üniversite Adı · Bölüm (Lisans)"),
          paragraph("Mezuniyet derecesi, öne çıkan dersler veya projeler."),
          paragraph("2012 – 2016 · Lise Adı"),
          paragraph("İş Deneyimi", "Heading2"),
          paragraph("2022 – Günümüz · Şirket Adı · Pozisyon"),
          paragraph("Sorumluluklarınızı ve işinize kattığınız ölçülebilir sonuçları kısaca yazın."),
          paragraph("2020 – 2022 · Şirket Adı · Pozisyon"),
          paragraph("Sorumluluklarınızı ve işinize kattığınız ölçülebilir sonuçları kısaca yazın."),
          paragraph("Yetenekler", "Heading2"),
          paragraph("Proje yönetimi", "Normal", bullet()),
          paragraph("Veri analizi ve raporlama", "Normal", bullet()),
          paragraph("Ekip çalışması ve iletişim", "Normal", bullet()),
          paragraph("Diller", "Heading2"),
          table([
            ["Dil", "Seviye"],
            ["Türkçe", "Ana dil"],
            ["İngilizce", "İleri (C1)"],
            ["Almanca", "Başlangıç (A2)"],
          ]),
          paragraph("Referanslar", "Heading2"),
          paragraph("Ad Soyad · Unvan, Kurum · Telefon / E-posta"),
          paragraph("Ad Soyad · Unvan, Kurum · Telefon / E-posta"),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trMinutes",
      "Toplantı Tutanağı",
      "Tarih, katılımcılar, gündem, alınan kararlar ve imzalarla toplantı tutanağı.",
      (document) => {
        document.blocks = [
          paragraph("Toplantı Tutanağı", "Title"),
          paragraph("Tarih: GG.AA.YYYY\nSaat: SS:DD\nYer: ………………………"),
          paragraph("Katılımcılar", "Heading3"),
          paragraph("Ad Soyad – Unvan", "Normal", bullet()),
          paragraph("Ad Soyad – Unvan", "Normal", bullet()),
          paragraph("Ad Soyad – Unvan", "Normal", bullet()),
          paragraph("Gündem", "Heading3"),
          paragraph("Açılış ve gündemin okunması", "Normal", numbered()),
          paragraph("Önceki toplantı kararlarının gözden geçirilmesi", "Normal", numbered()),
          paragraph("Gündem maddesi", "Normal", numbered()),
          paragraph("Dilek ve temenniler", "Normal", numbered()),
          paragraph("Alınan Kararlar", "Heading3"),
          table([
            ["Karar", "Sorumlu", "Tarih"],
            ["Alınan kararı kısaca yazın.", "Ad Soyad", "GG.AA.YYYY"],
            ["", "", ""],
            ["", "", ""],
          ]),
          paragraph("İmzalar", "Heading3"),
          table([
            ["Toplantı Başkanı", "Raportör", "Katılımcı"],
            ["Ad Soyad", "Ad Soyad", "Ad Soyad"],
            ["İmza:", "İmza:", "İmza:"],
          ]),
        ];
      },
    ),
  ),
  turkish(
    calc(
      "trInvoice",
      "Fatura",
      "Satıcı ve alıcı bilgileri, kalemler, KDV ve genel toplamı formülle hesaplanan fatura.",
      (workbook) => {
        const sheet = workbook.sheets[0];
        sheet.name = "Fatura";
        const header = { bold: true, fill: "#EEF2FF" };
        const label = { bold: true, align: "right" };
        const money = { numberFormat: TRY_FORMAT };
        sheet.cells.A1 = sheetCell("FATURA", { bold: true, sizePt: 18 });
        sheet.cells.A2 = sheetCell("Fatura No:", { bold: true });
        sheet.cells.B2 = sheetCell("FTR-2026-0001");
        sheet.cells.A3 = sheetCell("Fatura Tarihi:", { bold: true });
        sheet.cells.B3 = sheetCell("GG.AA.YYYY");
        sheet.cells.A4 = sheetCell("Vade Tarihi:", { bold: true });
        sheet.cells.B4 = sheetCell("GG.AA.YYYY");
        sheet.cells.A6 = sheetCell("SATICI", header);
        sheet.cells.C6 = sheetCell("ALICI", header);
        const parties = [
          ["Firma Unvanı", "Ad Soyad / Firma Unvanı"],
          ["Adres", "Adres"],
          ["Vergi Dairesi / Vergi No", "Vergi Dairesi / VKN veya TCKN"],
          ["Telefon / E-posta", "Telefon / E-posta"],
        ];
        parties.forEach(([seller, buyer], index) => {
          sheet.cells[`A${7 + index}`] = sheetCell(seller);
          sheet.cells[`C${7 + index}`] = sheetCell(buyer);
        });
        ["Açıklama", "Miktar", "Birim Fiyat", "Tutar"].forEach((title, index) => {
          sheet.cells[`${String.fromCharCode(65 + index)}12`] = sheetCell(title, header);
        });
        // Five line items; empty quantity/price cells already carry their format.
        for (let row = 13; row <= 17; row += 1) {
          if (row === 13) sheet.cells[`A${row}`] = sheetCell("Ürün veya hizmet açıklaması");
          sheet.cells[`B${row}`] = sheetCell(row === 13 ? 1 : null, { numberFormat: "#,##0" });
          sheet.cells[`C${row}`] = sheetCell(row === 13 ? 0 : null, money);
          sheet.cells[`D${row}`] = sheetCell(`=B${row}*C${row}`, money);
        }
        sheet.cells.C19 = sheetCell("Ara Toplam", label);
        sheet.cells.D19 = sheetCell("=SUM(D13:D17)", money);
        sheet.cells.C20 = sheetCell("KDV (%20)", label);
        sheet.cells.D20 = sheetCell("=D19*0.2", money);
        sheet.cells.C21 = sheetCell("Genel Toplam", label);
        sheet.cells.D21 = sheetCell("=D19+D20", { ...money, bold: true, sizePt: 12, fill: "#F1F5F9" });
        sheet.cells.A23 = sheetCell("Ödeme Bilgileri", { bold: true });
        sheet.cells.A24 = sheetCell("Banka: ………………  IBAN: TR00 0000 0000 0000 0000 0000 00");
        sheet.cells.A25 = sheetCell("Ödemeyi vade tarihine kadar, açıklamaya fatura numarasını yazarak yapınız.");
        sheet.colWidths = { "0": 230, "1": 90, "2": 170, "3": 130 };
      },
    ),
  ),
  turkish(
    calc(
      "trBudget",
      "Bütçe Tablosu",
      "Aylık gelir ve gider kalemleri; her ayın ve yılın toplamı formülle hesaplanır.",
      (workbook) => {
        const sheet = workbook.sheets[0];
        sheet.name = "Bütçe";
        const months = [
          "Ocak",
          "Şubat",
          "Mart",
          "Nisan",
          "Mayıs",
          "Haziran",
          "Temmuz",
          "Ağustos",
          "Eylül",
          "Ekim",
          "Kasım",
          "Aralık",
        ];
        // Months fill B..M; N holds the yearly total of each row.
        const columns = months.map((_, index) => String.fromCharCode(66 + index));
        const header = { bold: true, fill: "#EEF2FF" };
        const money = { numberFormat: TRY_FORMAT };
        const totals = { ...money, bold: true, fill: "#F1F5F9" };
        sheet.cells.A1 = sheetCell("Kategori", header);
        months.forEach((month, index) => {
          sheet.cells[`${columns[index]}1`] = sheetCell(month, { ...header, align: "right" });
        });
        sheet.cells.N1 = sheetCell("Yıllık Toplam", { ...header, align: "right" });
        /** A titled block of category rows plus its monthly totals row; returns that row. */
        const section = (titleRow: number, title: string, items: string[], totalLabel: string): number => {
          sheet.cells[`A${titleRow}`] = sheetCell(title, { bold: true, color: "#1D4ED8" });
          const first = titleRow + 1;
          const last = titleRow + items.length;
          items.forEach((item, offset) => {
            const row = first + offset;
            sheet.cells[`A${row}`] = sheetCell(item);
            for (const column of columns) sheet.cells[`${column}${row}`] = sheetCell(0, money);
            sheet.cells[`N${row}`] = sheetCell(`=SUM(B${row}:M${row})`, { ...money, bold: true });
          });
          const totalRow = last + 1;
          sheet.cells[`A${totalRow}`] = sheetCell(totalLabel, { bold: true, fill: "#F1F5F9" });
          for (const column of columns) {
            sheet.cells[`${column}${totalRow}`] = sheetCell(`=SUM(${column}${first}:${column}${last})`, totals);
          }
          sheet.cells[`N${totalRow}`] = sheetCell(`=SUM(B${totalRow}:M${totalRow})`, totals);
          return totalRow;
        };
        const income = section(2, "GELİRLER", ["Maaş", "Ek Gelir", "Kira Geliri", "Diğer Gelirler"], "Toplam Gelir");
        const expenses = section(
          income + 2,
          "GİDERLER",
          [
            "Kira",
            "Market ve Mutfak",
            "Elektrik, Su, Doğalgaz",
            "İnternet ve Telefon",
            "Ulaşım",
            "Sağlık",
            "Eğitim",
            "Eğlence ve Sosyal",
            "Diğer Giderler",
          ],
          "Toplam Gider",
        );
        const net = expenses + 2;
        sheet.cells[`A${net}`] = sheetCell("Net (Gelir - Gider)", { bold: true });
        for (const column of [...columns, "N"]) {
          sheet.cells[`${column}${net}`] = sheetCell(`=${column}${income}-${column}${expenses}`, {
            ...money,
            bold: true,
          });
        }
        sheet.freezeRows = 1;
        sheet.freezeCols = 1;
        sheet.colWidths = Object.fromEntries([
          ["0", 190],
          ...columns.map((_, index) => [String(index + 1), 95]),
          ["13", 120],
        ]);
      },
    ),
  ),
  // Roadmap v4.8.0 item 8: the Turkish documents the roadmap names. Fields are
  // `{{alan}}` placeholders so no real person's data ever ships.
  turkish(
    writer(
      "trOfficialLetter",
      "Resmî Yazı",
      "Resmî Yazışma Yönetmeliği'ne uygun resmî yazı: başlık, sayı/konu, muhatap, ilgi, metin, imza ve ekler.",
      (document) => {
        document.blocks = [
          paragraph("T.C.", "Normal", { align: "center" }, { bold: true }),
          paragraph("{{kurum_adi}}", "Normal", { align: "center" }, { bold: true }),
          paragraph("Sayı: {{sayi}}", "Normal", { spaceBeforePt: 24 }),
          paragraph("Konu: {{konu}}", "Normal"),
          paragraph("{{tarih}}", "Normal", { align: "right", spaceBeforePt: 12 }),
          paragraph(
            "{{muhatap_makami}}",
            "Normal",
            { align: "center", spaceBeforePt: 24, spaceAfterPt: 24 },
            { bold: true },
          ),
          paragraph("İlgi: {{ilgi}}", "Normal", { spaceAfterPt: 12 }),
          paragraph("{{metin}}", "Normal", { align: "justify", firstLinePt: 36 }),
          paragraph("Gereğini arz ederim.", "Normal", { align: "right", spaceBeforePt: 24 }),
          paragraph("{{imza_tarihi}}", "Normal", { align: "right" }),
          paragraph("{{ad_soyad}}\n{{unvan}}", "Normal", { align: "right" }, { bold: true }),
          paragraph("Ekler:", "Normal", { spaceBeforePt: 24, spaceAfterPt: 4 }, { bold: true }),
          paragraph("{{ek}}", "Normal", numbered()),
          paragraph("Dağıtım:", "Normal", { spaceBeforePt: 12, spaceAfterPt: 4 }, { bold: true }),
          paragraph("Gereği: {{geregi}}", "Normal"),
          paragraph("Bilgi: {{bilgi}}", "Normal"),
          paragraph(
            "Adres: {{kurum_adresi}}\nTelefon: {{telefon}}\nE-posta: {{e_posta}}\nKEP adresi: {{kep}}",
            "Normal",
            {
              spaceBeforePt: 24,
            },
          ),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trNotice",
      "İhtarname",
      "İhtar eden ve muhatap bilgileri, konu, numaralı talepler, süre ve sonuç bölümüyle ihtarname.",
      (document) => {
        document.blocks = [
          paragraph("İHTARNAME", "Title", { align: "center" }),
          paragraph("{{tarih}}", "Normal", { align: "right" }),
          paragraph("İHTAR EDEN", "Heading3"),
          paragraph(
            "Ad Soyad / Firma Unvanı: {{ihtar_eden}}\nT.C. Kimlik No / VKN: {{ihtar_eden_kimlik}}\nAdres: {{ihtar_eden_adres}}\nTelefon: {{ihtar_eden_telefon}}",
          ),
          paragraph("MUHATAP", "Heading3"),
          paragraph(
            "Ad Soyad / Firma Unvanı: {{muhatap}}\nT.C. Kimlik No / VKN: {{muhatap_kimlik}}\nAdres: {{muhatap_adres}}",
          ),
          paragraph("KONU: {{konu}}", "Normal", { spaceBeforePt: 12, spaceAfterPt: 12 }, { bold: true }),
          paragraph("1. {{talep_bir}}", "Normal", numbered()),
          paragraph("2. {{talep_iki}}", "Normal", numbered()),
          paragraph("3. {{talep_uc}}", "Normal", numbered()),
          paragraph(
            "İşbu ihtarnamenin tebliğinden itibaren {{sure}} gün içinde yukarıda belirtilen taleplerin yerine getirilmesini, aksi hâlde alacağın tahsili ve sair haklarımız için yasal yollara başvurulacağını ihtar ederim.",
            "Normal",
            { align: "justify", firstLinePt: 36, spaceBeforePt: 12 },
          ),
          paragraph("İhtarname olunur.", "Normal", { spaceBeforePt: 12 }),
          paragraph("{{ihtar_eden}}\nİmza", "Normal", { align: "right", spaceBeforePt: 24 }),
          paragraph("Ekler: {{ekler}}", "Normal", { spaceBeforePt: 12 }),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trRentContract",
      "Kira Sözleşmesi",
      "Kiraya veren ve kiracı bilgileri, madde madde süre, bedel, depozito, gider ve fesih koşullarıyla kira sözleşmesi.",
      (document) => {
        document.blocks = [
          paragraph("KİRA SÖZLEŞMESİ", "Title", { align: "center" }),
          paragraph(
            "İşbu sözleşme, aşağıda kimlik bilgileri yazılı kiraya veren ile kiracı arasında, belirtilen taşınmazın kiralanması amacıyla düzenlenmiştir.",
            "Normal",
            { align: "justify" },
          ),
          paragraph("TARAFLAR", "Heading3"),
          paragraph(
            "KİRAYA VEREN\nAd Soyad: {{kira_veren}}\nT.C. Kimlik No: {{kv_kimlik}}\nAdres: {{kv_adres}}\nTelefon: {{kv_telefon}}",
          ),
          paragraph(
            "KİRACI\nAd Soyad: {{kiraci}}\nT.C. Kimlik No: {{k_kimlik}}\nAdres: {{k_adres}}\nTelefon: {{k_telefon}}",
          ),
          paragraph("MADDE 1 – KONU", "Heading3"),
          paragraph("Sözleşmenin konusu, {{tasinmaz_adresi}} adresindeki taşınmazın kiralanmasıdır."),
          paragraph("MADDE 2 – SÜRE", "Heading3"),
          paragraph(
            "Kira süresi {{baslangic}} tarihinde başlar ve {{bitis}} tarihinde sona erer. Taraflar, sürenin bitiminden en az bir ay önce yazılı bildirimde bulunmadıkça sözleşme aynı koşullarla bir yıl uzar.",
          ),
          paragraph("MADDE 3 – KİRA BEDELİ", "Heading3"),
          paragraph(
            "Aylık kira bedeli {{kira_bedeli}} Türk lirasıdır. Kira bedeli, yasal düzenlemeler çerçevesinde yıllık olarak güncellenir.",
          ),
          paragraph("MADDE 4 – ÖDEME", "Heading3"),
          paragraph("Kira bedeli her ayın {{odeme_gunu}}. günü, {{odeme_sekli}} yoluyla kiraya verene ödenir."),
          paragraph("MADDE 5 – DEPOZİTO", "Heading3"),
          paragraph(
            "Kiracı, kiraya verene {{depozito}} Türk lirası güvence bedeli vermiştir. Bu bedel, sözleşmenin sona ermesi ve taşınmazın hasarsız teslimi hâlinde iade edilir.",
          ),
          paragraph("MADDE 6 – AİDAT VE GİDERLER", "Heading3"),
          paragraph(
            "Elektrik, su, doğalgaz, internet ve aidat gibi kullanıma bağlı giderler kiracıya; taşınmaza ilişkin vergi ve benzeri yükümlülükler kiraya verene aittir.",
          ),
          paragraph("MADDE 7 – KULLANIM", "Heading3"),
          paragraph(
            "Kiracı, taşınmazı yalnızca {{kullanim_amaci}} amacıyla kullanır; kiraya verenin yazılı izni olmadan taşınmazı üçüncü kişilere devredemez veya alt kiraya veremez.",
          ),
          paragraph("MADDE 8 – BAKIM VE ONARIM", "Heading3"),
          paragraph(
            "Olağan kullanımdan doğan küçük onarımlar kiracıya, taşınmazın esaslı onarımı kiraya verene aittir.",
          ),
          paragraph("MADDE 9 – CEZAİ ŞARTLAR", "Heading3"),
          paragraph(
            "Kira bedelinin ödenmesinde gecikme olması hâlinde, gecikilen her gün için {{gecikme_faizi}} oranında temerrüt faizi uygulanır.",
          ),
          paragraph("MADDE 10 – FESİH", "Heading3"),
          paragraph(
            "Taraflardan biri sözleşmeye aykırı davranır ve {{ihtar_suresi}} gün içinde aykırılığı gidermezse, diğer taraf sözleşmeyi tek taraflı olarak feshedebilir.",
          ),
          paragraph("MADDE 11 – YETKİLİ MAHKEME", "Heading3"),
          paragraph(
            "İşbu sözleşmeden doğan uyuşmazlıklarda {{yetkili_mahkeme}} mahkemeleri ve icra daireleri yetkilidir.",
          ),
          paragraph("MADDE 12 – YÜRÜRLÜK", "Heading3"),
          paragraph(
            "İşbu sözleşme {{madde_sayisi}} maddeden oluşur, taraflarca okunarak iki nüsha hâlinde imzalanmıştır.",
          ),
          paragraph("KİRAYA VEREN\nAd Soyad: {{kira_veren}}\nİmza:", "Normal", { spaceBeforePt: 24 }),
          paragraph("KİRACI\nAd Soyad: {{kiraci}}\nİmza:", "Normal", { spaceBeforePt: 12 }),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trLeaveRequest",
      "İzin Dilekçesi",
      "Tarih, muhatap, izin türü ve tarih aralığı, imza ve iletişim bilgileriyle izin dilekçesi.",
      (document) => {
        document.blocks = [
          paragraph("{{tarih}}", "Normal", { align: "right" }),
          paragraph(
            "{{muhatap_makami}}",
            "Normal",
            { align: "center", spaceBeforePt: 24, spaceAfterPt: 24 },
            { bold: true },
          ),
          paragraph(
            "{{kurum_adi}} bünyesinde {{gorev}} olarak görev yapmaktayım. {{baslangic}} – {{bitis}} tarihleri arasında {{izin_turu}} kullanmak istiyorum. İzin süresince görevlerimin yürütülmesi için gerekli önlemler alınmıştır.",
            "Normal",
            { align: "justify", firstLinePt: 36 },
          ),
          paragraph("Gereğini arz ederim.", "Normal", { firstLinePt: 36 }),
          paragraph("{{imza_tarihi}}", "Normal", { align: "right", spaceBeforePt: 24 }),
          paragraph("{{ad_soyad}}\n{{unvan}}\nİmza", "Normal", { align: "right" }, { bold: true }),
          paragraph("Ekler:", "Normal", { spaceBeforePt: 24, spaceAfterPt: 4 }, { bold: true }),
          paragraph("İzin talep formu", "Normal", numbered()),
          paragraph("Adres: {{adres}}\nTelefon: {{telefon}}", "Normal", { spaceBeforePt: 12 }),
        ];
      },
    ),
  ),
  turkish(
    writer(
      "trResignation",
      "İstifa Dilekçesi",
      "Tarih, muhatap, ihbar süresi ve son çalışma tarihi, imza ve iletişim bilgileriyle istifa dilekçesi.",
      (document) => {
        document.blocks = [
          paragraph("{{tarih}}", "Normal", { align: "right" }),
          paragraph(
            "{{muhatap_makami}}",
            "Normal",
            { align: "center", spaceBeforePt: 24, spaceAfterPt: 24 },
            { bold: true },
          ),
          paragraph(
            "{{kurum_adi}} bünyesinde {{gorev}} olarak yürütmekte olduğum görevimden, {{ihbar_suresi}} ihbar süresine uyarak {{son_calisma_tarihi}} tarihi itibarıyla istifa ediyorum. Bu süre içinde görevlerimin devri için gerekli çalışmaları yapacağımı bildirir, bugüne kadar sağlanan iş birliği için teşekkür ederim.",
            "Normal",
            { align: "justify", firstLinePt: 36 },
          ),
          paragraph("Gereğini arz ederim.", "Normal", { firstLinePt: 36 }),
          paragraph("{{imza_tarihi}}", "Normal", { align: "right", spaceBeforePt: 24 }),
          paragraph("{{ad_soyad}}\n{{unvan}}\nİmza", "Normal", { align: "right" }, { bold: true }),
          paragraph("Adres: {{adres}}\nTelefon: {{telefon}}", "Normal", { spaceBeforePt: 24 }),
        ];
      },
    ),
  ),
  turkish(
    calc(
      "trPayroll",
      "Bordro",
      "Kazanç ve kesinti kalemleri formülle hesaplanan, net ücreti gösteren bordro.",
      (workbook) => {
        const sheet = workbook.sheets[0];
        sheet.name = "Bordro";
        const header = { bold: true, fill: "#EEF2FF" };
        const money = { numberFormat: TRY_FORMAT };
        const label = { bold: true };
        sheet.cells.A1 = sheetCell("BORDRO", { bold: true, sizePt: 18 });
        sheet.cells.A2 = sheetCell("Dönem:", label);
        sheet.cells.B2 = sheetCell("{{donem}}");
        sheet.cells.A3 = sheetCell("Ad Soyad:", label);
        sheet.cells.B3 = sheetCell("{{ad_soyad}}");
        sheet.cells.A4 = sheetCell("T.C. Kimlik No:", label);
        sheet.cells.B4 = sheetCell("{{tckn}}");
        sheet.cells.A5 = sheetCell("Sicil No:", label);
        sheet.cells.B5 = sheetCell("{{sicil_no}}");
        sheet.cells.A6 = sheetCell("İşe Giriş Tarihi:", label);
        sheet.cells.B6 = sheetCell("{{ise_giris}}");
        sheet.cells.A8 = sheetCell("KAZANÇLAR", header);
        sheet.cells.A9 = sheetCell("Açıklama", header);
        sheet.cells.B9 = sheetCell("Tutar", header);
        const earnings: [string, string][] = [
          ["Temel Ücret", "B10"],
          ["Fazla Mesai", "B11"],
          ["Prim", "B12"],
          ["Yol", "B13"],
          ["Yemek", "B14"],
        ];
        earnings.forEach(([title, address], index) => {
          sheet.cells[`A${10 + index}`] = sheetCell(title);
          sheet.cells[address] = sheetCell(0, money);
        });
        sheet.cells.A15 = sheetCell("Brüt Kazanç", label);
        sheet.cells.B15 = sheetCell("=SUM(B10:B14)", { ...money, bold: true });
        sheet.cells.A17 = sheetCell("KESİNTİLER", header);
        sheet.cells.A18 = sheetCell("Açıklama", header);
        sheet.cells.B18 = sheetCell("Tutar", header);
        sheet.cells.A19 = sheetCell("SGK İşçi Payı (%14)");
        sheet.cells.B19 = sheetCell("=B15*0.14", money);
        sheet.cells.A20 = sheetCell("İşsizlik Sigortası İşçi Payı (%1)");
        sheet.cells.B20 = sheetCell("=B15*0.01", money);
        sheet.cells.A21 = sheetCell("Gelir Vergisi Matrahı");
        sheet.cells.B21 = sheetCell("=B15-B19-B20", money);
        sheet.cells.A22 = sheetCell("Gelir Vergisi (%15)");
        sheet.cells.B22 = sheetCell("=B21*0.15", money);
        sheet.cells.A23 = sheetCell("Damga Vergisi (%0,759)");
        sheet.cells.B23 = sheetCell("=B15*0.00759", money);
        sheet.cells.A24 = sheetCell("Kesintiler Toplamı", label);
        sheet.cells.B24 = sheetCell("=B19+B20+B22+B23", { ...money, bold: true });
        sheet.cells.A26 = sheetCell("NET ÜCRET", { bold: true, sizePt: 14 });
        sheet.cells.B26 = sheetCell("=B15-B24", { ...money, bold: true, sizePt: 14 });
        sheet.cells.A28 = sheetCell(
          "Not: Tutarlar {{para_birimi}} cinsindendir; oranlar örnektir, güncel mevzuata göre güncelleyin.",
          {
            italic: true,
          },
        );
        sheet.colWidths = Object.fromEntries([
          ["0", 220],
          ["1", 120],
        ]);
      },
    ),
  ),
  turkish(
    writer(
      "trDeliveryNote",
      "İrsaliye",
      "Satıcı ve alıcı bilgileri, teslim edilen kalemler ve teslim eden/alan imza bölümüyle irsaliye.",
      (document) => {
        document.blocks = [
          paragraph("İRSALİYE", "Title", { align: "center" }),
          paragraph("İrsaliye No: {{irsaliye_no}} · Tarih: {{tarih}}", "Subtitle", { align: "center" }),
          paragraph("SATICI", "Heading3"),
          paragraph(
            "Firma Unvanı: {{satici_unvani}}\nAdres: {{satici_adres}}\nVergi Dairesi / VKN: {{satici_vkn}}\nTelefon: {{satici_telefon}}",
          ),
          paragraph("ALICI", "Heading3"),
          paragraph(
            "Firma Unvanı: {{alici_unvani}}\nAdres: {{alici_adres}}\nVergi Dairesi / VKN: {{alici_vkn}}\nTelefon: {{alici_telefon}}",
          ),
          table([
            ["Sıra", "Mal / Hizmet", "Miktar", "Birim", "Açıklama"],
            ["1", "{{mal_hizmet}}", "{{miktar}}", "{{birim}}", "{{aciklama}}"],
            ["2", "", "", "", ""],
            ["3", "", "", "", ""],
            ["4", "", "", "", ""],
          ]),
          paragraph("Teslim Eden: {{teslim_eden}}\nİmza:", "Normal", { spaceBeforePt: 24 }),
          paragraph("Teslim Alan: {{teslim_alan}}\nİmza:", "Normal", { spaceBeforePt: 12 }),
          paragraph("Bu irsaliye fatura yerine geçmez; mal/hizmet bedeli ayrıca faturalandırılır.", "Caption", {
            spaceBeforePt: 12,
          }),
        ];
      },
    ),
  ),
];

export function templatesFor(kind: "writer" | "calc" | "impress"): OfficeTemplate[] {
  return TEMPLATES.filter((template) => template.kind === kind);
}

/**
 * Display order for a UI language: with a Turkish interface the Turkish
 * templates come first; otherwise the catalogue order is kept, which lists
 * them last.
 */
export function orderTemplates(templates: OfficeTemplate[], language: string): OfficeTemplate[] {
  if (language !== "tr") return templates;
  return [
    ...templates.filter((template) => template.language === "tr"),
    ...templates.filter((template) => template.language !== "tr"),
  ];
}
