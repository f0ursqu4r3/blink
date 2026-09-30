import type { ResponseTiming } from "./request";

export type TimingPhase = {
  id: "dns" | "connect" | "tls" | "wait" | "download";
  label: string;
  ms: number;
  /** Start, from the request start. */
  offsetMs: number;
};

/** The phases in order, each starting when the one before it ends. */
export function timingPhases(timing: ResponseTiming): TimingPhase[] {
  const entries: [TimingPhase["id"], string, number | undefined][] = [
    ["dns", "DNS lookup", timing.dnsMs],
    [
      "connect",
      timing.tlsMs === undefined ? "Connect" : "TCP connect",
      timing.connectMs,
    ],
    ["tls", "TLS handshake", timing.tlsMs],
    ["wait", "Waiting (TTFB)", timing.waitMs],
    ["download", "Content download", timing.downloadMs],
  ];
  let offsetMs = 0;
  const phases: TimingPhase[] = [];
  for (const [id, label, ms] of entries) {
    if (ms === undefined) continue;
    phases.push({ id, label, ms, offsetMs });
    offsetMs += ms;
  }
  return phases;
}

export function formatMs(ms: number) {
  if (ms < 1) return `${ms.toFixed(2)} ms`;
  if (ms < 10) return `${ms.toFixed(1)} ms`;
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

/**
 * Browser phases from a Resource Timing entry. Without Timing-Allow-Origin
 * a cross-origin entry has zeros, so only `fallback` (measured around
 * fetch) is known.
 */
export function browserTiming(
  entry: PerformanceResourceTiming | undefined,
  fallback: { waitMs: number; downloadMs: number },
): ResponseTiming {
  if (!entry || !entry.responseStart || !entry.requestStart) return fallback;
  const secure = entry.secureConnectionStart > 0;
  return {
    dnsMs: entry.domainLookupEnd - entry.domainLookupStart,
    connectMs:
      (secure ? entry.secureConnectionStart : entry.connectEnd) -
      entry.connectStart,
    ...(secure
      ? { tlsMs: entry.connectEnd - entry.secureConnectionStart }
      : {}),
    waitMs: entry.responseStart - entry.requestStart,
    downloadMs: Math.max(0, entry.responseEnd - entry.responseStart),
  };
}
