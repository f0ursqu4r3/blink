import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiResponse, RequestInput } from "./request";
export const nativeTransport = isTauri();
export const RESPONSE_LIMIT = 4 * 1024 * 1024;

export async function sendRequest(request: RequestInput): Promise<ApiResponse> {
  if (nativeTransport) return invoke<ApiResponse>("send_request", { request });
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 30_000);
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
      redirect: "manual",
      referrerPolicy: "no-referrer",
    });
    if (result.type === "opaqueredirect")
      throw new Error(
        "Browser preview cannot inspect redirects. Use the desktop app.",
      );
    const reader = result.body?.getReader();
    let sizeBytes = 0;
    const chunks: Uint8Array[] = [];
    if (reader) {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        sizeBytes += value.byteLength;
        if (sizeBytes > RESPONSE_LIMIT) {
          await reader.cancel();
          throw new Error("Response exceeds the 4 MiB inspection limit.");
        }
        chunks.push(value);
      }
    }
    const bytes = new Uint8Array(sizeBytes);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return {
      status: result.status,
      statusText: result.statusText,
      durationMs: Math.round(performance.now() - start),
      sizeBytes,
      headers: Array.from(result.headers, ([key, value]) => ({ key, value })),
      body: new TextDecoder().decode(bytes),
    };
  } catch (error) {
    if (controller.signal.aborted)
      throw new Error("Request timed out after 30 seconds.");
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}
