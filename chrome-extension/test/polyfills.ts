/**
 * pdf.js 6 uses the ES2025 typed-array helpers (`Uint8Array.prototype.toHex`,
 * `fromBase64`, ...). Chrome 130+ has them, but the Node test runner may not,
 * so the same modules would run in the browser and fail here. Shim only what is
 * missing; a runtime that already has them is untouched.
 */
type HexHelpers = {
  toHex?: () => string;
  toBase64?: (options?: { alphabet?: "base64" | "base64url" }) => string;
};

type HexConstructor = {
  fromHex?: (value: string) => Uint8Array;
  fromBase64?: (value: string, options?: { alphabet?: "base64" | "base64url" }) => Uint8Array;
};

function define(target: object, name: string, value: unknown): void {
  Object.defineProperty(target, name, { value, writable: true, configurable: true });
}

const helpers = Uint8Array.prototype as HexHelpers;
const constructor = Uint8Array as unknown as HexConstructor;

if (typeof helpers.toHex !== "function") {
  define(Uint8Array.prototype, "toHex", function toHex(this: Uint8Array): string {
    let out = "";
    for (const byte of this) out += byte.toString(16).padStart(2, "0");
    return out;
  });
}

if (typeof constructor.fromHex !== "function") {
  define(Uint8Array, "fromHex", function fromHex(value: string): Uint8Array {
    const clean = String(value).replace(/[^0-9a-fA-F]/g, "");
    const out = new Uint8Array(Math.floor(clean.length / 2));
    for (let index = 0; index < out.length; index += 1) {
      out[index] = Number.parseInt(clean.slice(index * 2, index * 2 + 2), 16);
    }
    return out;
  });
}

if (typeof helpers.toBase64 !== "function") {
  define(Uint8Array.prototype, "toBase64", function toBase64(this: Uint8Array): string {
    let binary = "";
    for (const byte of this) binary += String.fromCharCode(byte);
    return btoa(binary);
  });
}

if (typeof constructor.fromBase64 !== "function") {
  define(Uint8Array, "fromBase64", function fromBase64(value: string): Uint8Array {
    const binary = atob(String(value).replace(/-/g, "+").replace(/_/g, "/"));
    const out = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) out[index] = binary.charCodeAt(index);
    return out;
  });
}

/**
 * pdf.js 6 also calls `Promise.try` (used by its cancellation/promise helpers).
 * Chrome has it; Node 22 does not, so the Node test runner would fail on the
 * first document operation.
 */
type PromiseTry = {
  try?: (callback: (...args: unknown[]) => unknown, ...args: unknown[]) => Promise<unknown>;
};

const promiseConstructor = Promise as unknown as PromiseTry;
if (typeof promiseConstructor.try !== "function") {
  define(Promise, "try", function promiseTry(callback: (...args: unknown[]) => unknown, ...args: unknown[]) {
    // Calling inside the executor captures a synchronous throw as a rejection,
    // which is what `Promise.try` promises.
    return new Promise((resolve) => resolve(callback(...args)));
  });
}
