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

  it("restores more than 128 requests", () => {
    const sessions = Array.from({ length: 300 }, () => createSession());
    const ids = sessions.map((session) => session.id);
    const result = decodeWorkspace(encode(sessions, ids[0], ids));
    expect(result.sessions).toHaveLength(300);
    expect(result.openIds).toHaveLength(300);
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

  it("keeps optional response fields but never saves the body id", () => {
    const session = createSession();
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 9,
      headers: [],
      body: "",
      bodyId: "body-1",
      truncated: true,
      binary: true,
      finalUrl: "https://final.test/",
      redirectCount: 2,
    };
    const encoded = encodeWorkspace([session], session.id);
    expect(encoded).not.toContain("body-1");
    const restored = decodeWorkspace(encoded).sessions[0].response!;
    expect(restored).toMatchObject({
      truncated: true,
      binary: true,
      finalUrl: "https://final.test/",
      redirectCount: 2,
    });
    expect(restored.bodyId).toBeUndefined();
  });

  it("drops a body id found in a saved workspace", () => {
    const session = createSession();
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 0,
      headers: [],
      body: "",
    };
    const data = JSON.parse(encodeWorkspace([session], session.id));
    data.tabs[0].response.bodyId = "stale";
    expect(
      decodeWorkspace(JSON.stringify(data)).sessions[0].response!.bodyId,
    ).toBeUndefined();
  });

  it("empties the body of a truncated preview so it does not blow past the save limit", () => {
    const session = createSession();
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 5 * 1024 * 1024,
      headers: [],
      body: "x".repeat(4 * 1024 * 1024),
      truncated: true,
      binary: false,
    };
    const encoded = JSON.parse(encodeWorkspace([session], session.id));
    expect(encoded.tabs[0].response.body).toBe("");
    const restored = decodeWorkspace(JSON.stringify(encoded)).sessions[0]
      .response!;
    expect(restored).toMatchObject({
      body: "",
      truncated: true,
      binary: false,
      sizeBytes: 5 * 1024 * 1024,
    });
  });

  it("empties the body of a binary response", () => {
    const session = createSession();
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 2048,
      headers: [],
      body: "",
      binary: true,
      truncated: true,
    };
    const encoded = JSON.parse(encodeWorkspace([session], session.id));
    expect(encoded.tabs[0].response.body).toBe("");
    expect(encoded.tabs[0].response.binary).toBe(true);
    expect(encoded.tabs[0].response.truncated).toBe(true);
  });

  it("keeps the body of a complete text response", () => {
    const session = createSession();
    session.response = {
      status: 200,
      statusText: "OK",
      durationMs: 1,
      sizeBytes: 5,
      headers: [],
      body: "hello",
    };
    const encoded = JSON.parse(encodeWorkspace([session], session.id));
    expect(encoded.tabs[0].response.body).toBe("hello");
  });
});
