import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiResponse, RequestInput } from "./request";
import { storeBlob } from "./response-body";
import {
  defaultTransportOptions,
  DOWNLOAD_LIMIT,
  MIB,
  type TransportOptions,
} from "./transport-options";
export const nativeTransport = isTauri();

// Bytes of a UTF-8 character that the preview cut in two. 0 when the preview
// ends on a character boundary.
function incompleteTail(bytes: Uint8Array) {
  for (let cut = 1; cut <= Math.min(3, bytes.length); cut++) {
    const byte = bytes[bytes.length - cut];
    if (byte >= 0x80 && byte <= 0xbf) continue;
    // Not a lead byte: the decoder reports it as invalid.
    if (byte < 0xc2 || byte > 0xf4) return 0;
    const length = byte >= 0xf0 ? 4 : byte >= 0xe0 ? 3 : byte >= 0xc0 ? 2 : 1;
    return length > cut ? cut : 0;
  }
  return 0;
}

/** Same rule as the desktop transport: NUL or invalid UTF-8 is binary. */
export function decodePreview(bytes: Uint8Array) {
  if (bytes.includes(0)) return { text: "", binary: true };
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(
      bytes.subarray(0, bytes.length - incompleteTail(bytes)),
    );
    return { text, binary: false };
  } catch {
    return { text: "", binary: true };
  }
}

export async function sendRequest(
  request: RequestInput,
  options: TransportOptions = defaultTransportOptions(),
  downloadLimit = DOWNLOAD_LIMIT,
): Promise<ApiResponse> {
  if (nativeTransport)
    return invoke<ApiResponse>("send_request", { request, options });
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    options.timeoutSeconds * 1000,
  );
  const start = performance.now();
  try {
    const headers = new Headers();
    request.headers.forEach(({ key, value }) => headers.append(key, value));
    const result = await fetch(request.url, {
      method: request.method,
      headers,
      body: request.body,
      signal: controller.signal,
      credentials: "omit",
      cache: "no-store",
      redirect: options.followRedirects ? "follow" : "manual",
      referrerPolicy: "no-referrer",
    });
    if (result.type === "opaqueredirect")
      throw new Error(
        "Browser preview cannot inspect redirects. Use the desktop app.",
      );
    const previewLimit = options.inspectionLimitMiB * MIB;
    const reader = result.body?.getReader();
    let sizeBytes = 0;
    const chunks: Uint8Array[] = [];
    if (reader) {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        sizeBytes += value.byteLength;
        if (sizeBytes > downloadLimit) {
          await reader.cancel();
          throw new Error("Response exceeds the 1 GiB download limit.");
        }
        chunks.push(value);
      }
    }
    const preview = new Uint8Array(Math.min(sizeBytes, previewLimit));
    let offset = 0;
    for (const chunk of chunks) {
      if (offset >= preview.length) break;
      const part = chunk.subarray(0, preview.length - offset);
      preview.set(part, offset);
      offset += part.byteLength;
    }
    const { text, binary } = decodePreview(preview);
    const contentType = result.headers.get("content-type") ?? "";
    return {
      status: result.status,
      statusText: result.statusText,
      durationMs: Math.round(performance.now() - start),
      sizeBytes,
      headers: Array.from(result.headers, ([key, value]) => ({ key, value })),
      body: text,
      bodyId: storeBlob(new Blob(chunks as BlobPart[], { type: contentType })),
      truncated: sizeBytes > preview.length,
      binary,
      ...(result.redirected ? { finalUrl: result.url } : {}),
    };
  } catch (error) {
    if (controller.signal.aborted)
      throw new Error(
        `Request timed out after ${options.timeoutSeconds} seconds.`,
      );
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}
