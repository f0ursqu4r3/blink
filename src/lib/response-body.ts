import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiResponse } from "./request";

export const BODY_UNAVAILABLE = "The response body is no longer available";

const native = isTauri();
// Browser preview only: raw bodies by id.
const blobs = new Map<string, Blob>();
let nextBlobId = 0;

export function storeBlob(blob: Blob) {
  const id = `browser-${++nextBlobId}`;
  blobs.set(id, blob);
  return id;
}

export function releaseResponse(response: ApiResponse | null | undefined) {
  const id = response?.bodyId;
  if (!id) return;
  if (native) void invoke("release_response", { bodyId: id }).catch(() => {});
  else blobs.delete(id);
}

/** After a restart only complete text previews can be saved. */
export const canSaveResponse = (response: ApiResponse) =>
  Boolean(response.bodyId) || (!response.truncated && !response.binary);

/** Resolves false when the user cancels the save dialog. */
export async function saveResponse(
  response: ApiResponse,
  requestUrl: string,
): Promise<boolean> {
  const suggestedName = suggestedFileName(response, requestUrl);
  if (response.bodyId) {
    if (native)
      return invoke<boolean>("save_response", {
        bodyId: response.bodyId,
        suggestedName,
      });
    const blob = blobs.get(response.bodyId);
    if (!blob) throw new Error(BODY_UNAVAILABLE);
    download(blob, suggestedName);
    return true;
  }
  if (!canSaveResponse(response)) throw new Error(BODY_UNAVAILABLE);
  if (native)
    return invoke<boolean>("save_response_text", {
      text: response.body,
      suggestedName,
    });
  download(new Blob([response.body], { type: "text/plain" }), suggestedName);
  return true;
}

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  // Revoking at once can cancel the download in some browsers.
  setTimeout(() => URL.revokeObjectURL(url), 40_000);
}

const extensions: Record<string, string> = {
  "application/json": ".json",
  "application/xml": ".xml",
  "text/xml": ".xml",
  "text/html": ".html",
  "text/plain": ".txt",
  "image/png": ".png",
  "image/jpeg": ".jpg",
  "application/pdf": ".pdf",
  "application/zip": ".zip",
};

function header(response: ApiResponse, name: string) {
  return response.headers.find(({ key }) => key.toLowerCase() === name)?.value;
}

function decode(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

// No path separators, control characters, or names that mean a directory.
function clean(name: string) {
  // oxlint-disable-next-line no-control-regex
  const safe = name.replace(/[/\\\u0000-\u001f\u007f]/g, "").trim();
  return safe === "." || safe === ".." ? "" : safe;
}

export function suggestedFileName(response: ApiResponse, requestUrl: string) {
  const disposition = header(response, "content-disposition") ?? "";
  const encoded = disposition.match(/filename\*\s*=\s*UTF-8''([^;]+)/i);
  const plain = disposition.match(/filename\s*=\s*"?([^";]+)"?/i);
  const fromHeader = clean(
    encoded ? decode(encoded[1].trim()) : (plain?.[1].trim() ?? ""),
  );
  if (fromHeader) return fromHeader;
  try {
    const segment = new URL(response.finalUrl ?? requestUrl).pathname
      .split("/")
      .filter(Boolean)
      .pop();
    const fromUrl = clean(decode(segment ?? ""));
    if (fromUrl) return fromUrl;
  } catch {
    // Not an absolute URL: fall through to the content type.
  }
  const type =
    header(response, "content-type")?.split(";")[0].trim().toLowerCase() ?? "";
  return `response${extensions[type] ?? ".bin"}`;
}
