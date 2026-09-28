import { describe, expect, it } from "vitest";
import { createSession } from "../session";
import { defaultPreferences } from "../preferences";
import { encodeWorkspace, decodeWorkspace } from "../workspace";

// V4 separates the request tree (tabs array) from the open tab list (openIds).

describe("workspace v4 open tabs", () => {
  const encode = (
    sessions: ReturnType<typeof createSession>[],
    activeId: number | null,
    openIds: number[],
  ) =>
    encodeWorkspace(sessions, activeId, [], {}, defaultPreferences(), openIds);

  it("round-trips open tabs separately from requests", () => {
    const [a, b, c] = [createSession(), createSession(), createSession()];
    const result = decodeWorkspace(encode([a, b, c], c.id, [c.id, a.id]));
    expect(result.sessions.map((session) => session.id)).toEqual([
      a.id,
      b.id,
      c.id,
    ]);
    expect(result.openIds).toEqual([c.id, a.id]);
    expect(result.activeId).toBe(c.id);
  });

  it("allows no open tabs with a null active id", () => {
    const session = createSession();
    const result = decodeWorkspace(encode([session], null, []));
    expect(result.openIds).toEqual([]);
    expect(result.activeId).toBeNull();
  });

  it("opens every request when migrating from v3", () => {
    const [a, b] = [createSession(), createSession()];
    const raw = JSON.parse(encode([a, b], a.id, [a.id]));
    raw.version = 3;
    delete raw.openIds;
    expect(decodeWorkspace(JSON.stringify(raw)).openIds).toEqual([a.id, b.id]);
  });

  it("rejects an active id that is not open", () => {
    const [a, b] = [createSession(), createSession()];
    expect(() => decodeWorkspace(encode([a, b], b.id, [a.id]))).toThrow();
  });

  it("rejects open ids for unknown or duplicate requests", () => {
    const session = createSession();
    expect(() =>
      decodeWorkspace(encode([session], session.id, [session.id, 999])),
    ).toThrow();
    expect(() =>
      decodeWorkspace(encode([session], session.id, [session.id, session.id])),
    ).toThrow();
  });

  it("rejects a null active id while tabs are open", () => {
    const session = createSession();
    expect(() =>
      decodeWorkspace(encode([session], null, [session.id])),
    ).toThrow();
  });
});
