import { afterEach, describe, expect, it, vi } from "vitest";
import { decodePreview, sendRequest } from "../transport";
import { defaultTransportOptions, MIB } from "../transport-options";
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
  it("stops reading at the download limit", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response(new Uint8Array(65))),
    );
    await expect(
      sendRequest(request, defaultTransportOptions(), 64),
    ).rejects.toThrow("1 GiB download limit");
  });
  it("truncates the preview at the inspection limit", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("x".repeat(MIB + 5))),
    );
    const result = await sendRequest(request, {
      ...defaultTransportOptions(),
      inspectionLimitMiB: 1,
    });
    expect(result).toMatchObject({
      truncated: true,
      binary: false,
      sizeBytes: MIB + 5,
    });
    expect(result.body.length).toBe(MIB);
    expect(result.bodyId).toMatch(/^browser-/);
  });
  it("follows redirects when the setting is on", async () => {
    const response = new Response("done");
    Object.defineProperty(response, "redirected", { value: true });
    Object.defineProperty(response, "url", { value: "https://final.test/" });
    const fetch = vi.fn().mockResolvedValue(response);
    vi.stubGlobal("fetch", fetch);
    const result = await sendRequest(request, {
      ...defaultTransportOptions(),
      followRedirects: true,
    });
    expect(fetch.mock.calls[0][1]).toMatchObject({ redirect: "follow" });
    expect(result.finalUrl).toBe("https://final.test/");
    expect(result.redirectCount).toBeUndefined();
  });
  it("explains opaque redirect limitations instead of showing status zero", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ type: "opaqueredirect" }),
    );
    await expect(sendRequest(request)).rejects.toThrow("desktop app");
  });
  it("aborts a hanging request after 5 seconds and clears its timer", async () => {
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
    const result = expect(
      sendRequest(request, { ...defaultTransportOptions(), timeoutSeconds: 5 }),
    ).rejects.toThrow("after 5 seconds");
    await vi.advanceTimersByTimeAsync(5_000);
    await result;
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("decodePreview", () => {
  const bytes = (...values: number[]) => new Uint8Array(values);
  it("treats NUL and invalid sequences as binary", () => {
    expect(decodePreview(bytes(0x61, 0x00))).toEqual({
      text: "",
      binary: true,
    });
    expect(decodePreview(bytes(0x61, 0xff, 0x62))).toEqual({
      text: "",
      binary: true,
    });
    expect(decodePreview(bytes(0x61, 0xff))).toEqual({
      text: "",
      binary: true,
    });
  });
  it("drops an incomplete character at the cut", () => {
    // "é" is C3 A9; "€" is E2 82 AC.
    expect(decodePreview(bytes(0x61, 0xc3))).toEqual({
      text: "a",
      binary: false,
    });
    expect(decodePreview(bytes(0x61, 0xe2, 0x82))).toEqual({
      text: "a",
      binary: false,
    });
    expect(decodePreview(bytes(0x61, 0xc3, 0xa9))).toEqual({
      text: "aé",
      binary: false,
    });
  });
});
