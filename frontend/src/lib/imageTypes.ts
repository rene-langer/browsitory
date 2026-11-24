// Image diff preview helpers. The file extension only decides which files *try* a preview;
// what is rendered is decided by the image type the backend sniffed from the bytes.

const BY_EXTENSION: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
};

/** The image types a preview may render, always through `<img>` (where SVG cannot run script). */
const RENDERABLE: ReadonlySet<string> = new Set(Object.values(BY_EXTENSION));

function extension(path: string): string | null {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : null;
}

/** The image type a file's extension suggests, or `null` for files that get a text diff. */
export function imageMimeType(path: string): string | null {
  const ext = extension(path);
  return ext === null ? null : (BY_EXTENSION[ext] ?? null);
}

/** SVG is text too, so its section can switch to the text diff. */
export function isSvgPath(path: string): boolean {
  return extension(path) === "svg";
}

// One character class, no groups: V8 runs this without a backtrack frame per character, so it
// is safe on ~27 MB of text (a `(?:…{4})*` form overflows the stack from a few MB).
const BASE64 = /^[A-Za-z0-9+/]*={0,2}$/;

/** A `data:` URL for a sniffed, renderable image type and well-formed base64; else `null`. */
export function imageDataUrl(mime: string | null, base64: string): string | null {
  if (mime === null || !RENDERABLE.has(mime) || base64.length % 4 !== 0 || !BASE64.test(base64)) return null;
  return `data:${mime};base64,${base64}`;
}
