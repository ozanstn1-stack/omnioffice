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
