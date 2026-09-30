import { describe, expect, it } from "vitest";
import { createSession } from "../session";
import { decodeWorkspace, encodeWorkspace } from "../workspace";
import type { HistoryEntry } from "../history";

const entry = (id: number): HistoryEntry => ({
  id,
  sentAt: 1_700_000_000_000 + id,
  method: "GET",
  url: "https://example.test",
  status: 200,
  statusText: "OK",
  durationMs: 12,
  sizeBytes: 2,
  headers: [{ key: "a", value: "1" }],
  body: "{}",
  timing: { waitMs: 10, downloadMs: 2 },
});

describe("workspace history", () => {
  it("round-trips request history", () => {
    const session = createSession();
    session.history = [entry(2), entry(1)];
    const [restored] = decodeWorkspace(
      encodeWorkspace([session], session.id),
    ).sessions;
    expect(restored.history).toEqual(session.history);
  });
  it("omits empty history", () => {
    const session = createSession();
    session.history = [];
    expect(encodeWorkspace([session], session.id)).not.toContain("history");
  });
  it("rejects invalid history", () => {
    const session = createSession();
    session.history = [entry(1), entry(1)];
    expect(() =>
      decodeWorkspace(encodeWorkspace([session], session.id)),
    ).toThrow();
    session.history = [{ ...entry(1), status: 42 }];
    expect(() =>
      decodeWorkspace(encodeWorkspace([session], session.id)),
    ).toThrow();
  });
});
