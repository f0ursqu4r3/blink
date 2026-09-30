import { describe, expect, it } from "vitest";
import { browserTiming, formatMs, timingPhases } from "../timing";

describe("timingPhases", () => {
  it("places each phase after the one before", () => {
    const phases = timingPhases({
      dnsMs: 5,
      connectMs: 20,
      waitMs: 100,
      downloadMs: 10,
    });
    expect(phases.map((p) => [p.id, p.offsetMs])).toEqual([
      ["dns", 0],
      ["connect", 5],
      ["wait", 25],
      ["download", 125],
    ]);
    expect(phases[1].label).toBe("Connect");
  });
  it("labels TCP apart when TLS is known", () => {
    const phases = timingPhases({
      connectMs: 3,
      tlsMs: 4,
      waitMs: 1,
      downloadMs: 1,
    });
    expect(phases.map((p) => p.label)).toContain("TCP connect");
    expect(phases.find((p) => p.id === "tls")?.offsetMs).toBe(3);
  });
});

describe("browserTiming", () => {
  it("uses the fallback without a detailed entry", () => {
    const fallback = { waitMs: 50, downloadMs: 5 };
    expect(browserTiming(undefined, fallback)).toBe(fallback);
    expect(
      browserTiming(
        { responseStart: 0, requestStart: 0 } as PerformanceResourceTiming,
        fallback,
      ),
    ).toBe(fallback);
  });
  it("reads phases from Resource Timing", () => {
    const entry = {
      domainLookupStart: 10,
      domainLookupEnd: 12,
      connectStart: 12,
      secureConnectionStart: 15,
      connectEnd: 20,
      requestStart: 21,
      responseStart: 61,
      responseEnd: 70,
    } as PerformanceResourceTiming;
    expect(browserTiming(entry, { waitMs: 0, downloadMs: 0 })).toEqual({
      dnsMs: 2,
      connectMs: 3,
      tlsMs: 5,
      waitMs: 40,
      downloadMs: 9,
    });
  });
});

it("formats durations", () => {
  expect(formatMs(0.123)).toBe("0.12 ms");
  expect(formatMs(4.56)).toBe("4.6 ms");
  expect(formatMs(123.4)).toBe("123 ms");
  expect(formatMs(2345)).toBe("2.35 s");
});
