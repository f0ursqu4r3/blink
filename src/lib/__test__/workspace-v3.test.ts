import { describe, expect, it } from "vitest";
import { createSession } from "../session";
import { encodeWorkspace, decodeWorkspace } from "../workspace";

// These tests exercise V3 persistence, V1/V2 migration of flat auth fields
// to explicit localAuth, and strict V3 validation.

describe("workspace v3 persistence and migration", () => {
  it("round-trips v3 with group localAuth and localDefinitions", () => {
    const session = createSession();
    const groups = [
      {
        id: 1,
        name: "API",
        parentId: null,
        collapsed: false,
        localAuth: { type: "bearer" as const, token: "g-tok" },
        localDefinitions: { host: "api.example.com" },
      },
    ];
    session.groupId = 1;
    const encoded = encodeWorkspace([session], session.id, groups);
    const data = JSON.parse(encoded);
    expect(data.version).toBe(3);
    const result = decodeWorkspace(encoded);
    expect(result.groups[0]).toMatchObject({
      localAuth: { type: "bearer", token: "g-tok" },
      localDefinitions: { host: "api.example.com" },
    });
  });

  it("round-trips v3 with workspace globalDefinitions", () => {
    const session = createSession();
    const globalDefs = { apiVersion: "v2" };
    const encoded = encodeWorkspace([session], session.id, [], globalDefs);
    const result = decodeWorkspace(encoded);
    expect(result.globalDefinitions).toEqual({ apiVersion: "v2" });
  });

  it("migrates v2 flat auth=bearer to explicit localAuth on draft", () => {
    const session = createSession();
    session.draft.url = "https://example.test";
    const v2 = JSON.parse(encodeWorkspace([session], session.id));
    // Patch to simulate v2 flat auth
    v2.version = 2;
    v2.tabs[0].draft.auth = "bearer";
    v2.tabs[0].draft.token = "tok123";
    v2.tabs[0].draft.username = "";
    v2.tabs[0].draft.password = "";

    const result = decodeWorkspace(JSON.stringify(v2));
    const draft = result.sessions[0].draft;
    // Flat fields should still exist for backward compat
    expect(draft.auth).toBe("bearer");
    expect(draft.token).toBe("tok123");
    // localAuth should be populated as an explicit local config
    expect(draft.localAuth).toEqual({ type: "bearer", token: "tok123" });
  });

  it("migrates v2 flat auth=basic to explicit localAuth on draft", () => {
    const session = createSession();
    const v2 = JSON.parse(encodeWorkspace([session], session.id));
    v2.version = 2;
    v2.tabs[0].draft.auth = "basic";
    v2.tabs[0].draft.token = "";
    v2.tabs[0].draft.username = "alice";
    v2.tabs[0].draft.password = "secret";

    const result = decodeWorkspace(JSON.stringify(v2));
    const draft = result.sessions[0].draft;
    expect(draft.localAuth).toEqual({
      type: "basic",
      username: "alice",
      password: "secret",
    });
  });

  it("migrates v2 flat auth=none to explicit localAuth none", () => {
    const session = createSession();
    const v2 = JSON.parse(encodeWorkspace([session], session.id));
    v2.version = 2;
    // auth=none means explicit none (not inherit)
    v2.tabs[0].draft.auth = "none";
    v2.tabs[0].draft.token = "";
    v2.tabs[0].draft.username = "";
    v2.tabs[0].draft.password = "";

    const result = decodeWorkspace(JSON.stringify(v2));
    const draft = result.sessions[0].draft;
    // Migrated: since v2 was always explicit, localAuth should be explicit none
    expect(draft.localAuth).toEqual({ type: "none" });
  });

  it("new sessions created without localAuth inherit by default (localAuth undefined)", () => {
    const session = createSession();
    expect(session.draft.localAuth).toBeUndefined();
  });

  it("v3 encodes localAuth on draft (not flat fields)", () => {
    const session = createSession();
    session.draft.localAuth = { type: "bearer", token: "d-tok" };
    const encoded = encodeWorkspace([session], session.id);
    const raw = JSON.parse(encoded);
    expect(raw.version).toBe(3);
    expect(raw.tabs[0].draft.localAuth).toEqual({
      type: "bearer",
      token: "d-tok",
    });
  });

  it("v3 draft localAuth=undefined means inherit, not stored in JSON", () => {
    const session = createSession();
    // No localAuth on draft
    const encoded = encodeWorkspace([session], session.id);
    const raw = JSON.parse(encoded);
    // localAuth absent or undefined in encoded form
    expect(raw.tabs[0].draft.localAuth).toBeUndefined();
  });

  it("v3 strict validation rejects unknown auth type in localAuth", () => {
    const session = createSession();
    const raw = JSON.parse(encodeWorkspace([session], session.id));
    raw.tabs[0].draft.localAuth = { type: "oauth" }; // invalid
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow();
  });

  it("migrates v1 snapshots with no groups and flat auth", () => {
    const session = createSession();
    const v1 = JSON.parse(encodeWorkspace([session], session.id));
    v1.version = 1;
    delete v1.groups;
    delete v1.tabs[0].groupId;
    v1.tabs[0].draft.auth = "bearer";
    v1.tabs[0].draft.token = "v1tok";

    const result = decodeWorkspace(JSON.stringify(v1));
    expect(result.groups).toEqual([]);
    expect(result.sessions[0].draft.localAuth).toEqual({
      type: "bearer",
      token: "v1tok",
    });
  });

  it("rejects v3 snapshot with invalid globalDefinitions", () => {
    const session = createSession();
    const raw = JSON.parse(encodeWorkspace([session], session.id));
    raw.globalDefinitions = "not-an-object";
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow();
  });
});
