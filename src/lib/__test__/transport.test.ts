import { afterEach, describe, expect, it, vi } from "vitest";
import { RESPONSE_LIMIT, sendRequest } from "../transport";
import type { RequestInput } from "../request";

const request: RequestInput = {
  method: "GET",
  url: "https://example.test",
  headers: [],
  body: null,
};
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("browser preview transport", () => {
  it("omits ambient credentials, referrers, caching and redirects", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetch);
    const result = await sendRequest(request);
    expect(result).toMatchObject({ status: 204, body: "", sizeBytes: 0 });
    expect(fetch.mock.calls[0][1]).toMatchObject({
      credentials: "omit",
      cache: "no-store",
      redirect: "manual",
      referrerPolicy: "no-referrer",
    });
  });
  it("measures UTF-8 bytes, not string length", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("雪")));
    expect(await sendRequest(request)).toMatchObject({
      body: "雪",
      sizeBytes: 3,
    });
  });
  it("bounds response memory while reading the stream", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(new Response(new Uint8Array(RESPONSE_LIMIT + 1))),
    );
    await expect(sendRequest(request)).rejects.toThrow("4 MiB");
  });
  it("explains opaque redirect limitations instead of showing status zero", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ type: "opaqueredirect" }),
    );
    await expect(sendRequest(request)).rejects.toThrow("desktop app");
  });
  it("aborts a hanging request after 30 seconds and clears its timer", async () => {
    vi.useFakeTimers();
    vi.stubGlobal(
      "fetch",
      vi.fn(
        (_url, options) =>
          new Promise((_resolve, reject) => {
            options.signal.addEventListener("abort", () =>
              reject(new DOMException("Aborted", "AbortError")),
            );
          }),
      ),
    );
    const result = expect(sendRequest(request)).rejects.toThrow("30 seconds");
    await vi.advanceTimersByTimeAsync(30_000);
    await result;
    expect(vi.getTimerCount()).toBe(0);
  });
});
