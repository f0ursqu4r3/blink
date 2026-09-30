import { describe, expect, it } from "vitest";
import { createDraft } from "../request";
import { createSession } from "../session";
import { decodeWorkspace, encodeWorkspace } from "../workspace";
import type { RequestGroup } from "../groups";
import {
  applyNewRequestDefaults,
  defaultPreferences,
  normalizePreferences,
  resolveNewRequestDefaults,
  transportOptions,
  validPreferences,
} from "../preferences";

describe("new request preferences", () => {
  const groups: RequestGroup[] = [
    {
      id: 1,
      name: "Root",
      parentId: null,
      collapsed: false,
      defaultMethod: "POST",
      defaultUrl: "{{host}}/v1",
    },
    {
      id: 2,
      name: "Child",
      parentId: 1,
      collapsed: false,
      defaultMethod: "PATCH",
    },
    { id: 3, name: "Leaf", parentId: 2, collapsed: false },
  ];

  it("resolves each field from the nearest group independently", () => {
    expect(
      resolveNewRequestDefaults(groups, 3, {
        ...defaultPreferences(),
        defaultMethod: "PUT",
        wrap: true,
      }),
    ).toEqual({
      method: "PATCH",
      url: "{{host}}/v1",
      bodyMode: "none",
      pretty: true,
      wrap: true,
    });
  });

  it("falls back to app defaults and preserves an explicit empty group URL", () => {
    const preferences = {
      ...defaultPreferences(),
      defaultMethod: "DELETE" as const,
      defaultBodyMode: "text" as const,
      pretty: false,
    };
    expect(resolveNewRequestDefaults([], null, preferences)).toEqual({
      method: "DELETE",
      bodyMode: "text",
      url: "",
      pretty: false,
      wrap: false,
    });
    expect(
      resolveNewRequestDefaults(
        [
          ...groups,
          {
            id: 4,
            name: "Blank",
            parentId: 3,
            collapsed: false,
            defaultUrl: "",
          },
        ],
        4,
        preferences,
      ).url,
    ).toBe("");
  });

  it("terminates malformed ancestry without overwriting the nearest defaults", () => {
    expect(
      resolveNewRequestDefaults(
        [{ ...groups[0], parentId: 2 }, groups[1]],
        2,
        defaultPreferences(),
      ).method,
    ).toBe("PATCH");
  });

  it("round trips preferences and group defaults without touching existing requests", () => {
    const session = createSession();
    session.groupId = 3;
    session.draft.url = "https://example.test/keep";
    const preferences = {
      ...defaultPreferences(),
      defaultMethod: "PUT" as const,
      defaultBodyMode: "json" as const,
      pretty: false,
      wrap: true,
      confirmCloseDrafts: false,
    };
    const decoded = decodeWorkspace(
      encodeWorkspace(
        [session],
        session.id,
        groups,
        { host: "https://example.test" },
        preferences,
      ),
    );
    expect(decoded.preferences).toEqual(preferences);
    expect(decoded.groups).toEqual(groups);
    expect(decoded.sessions[0].draft).toEqual(session.draft);
    expect(decoded.sessions[0].view).toEqual(session.view);
  });

  it.each([1, 2, 3])(
    "loads version %i workspaces without preferences",
    (version) => {
      const session = createSession();
      const raw = JSON.parse(encodeWorkspace([session], session.id));
      raw.version = version;
      delete raw.preferences;
      const decoded = decodeWorkspace(JSON.stringify(raw));
      expect(decoded.preferences).toEqual(defaultPreferences());
    },
  );

  it.each([
    null,
    [],
    {},
    { ...defaultPreferences(), defaultMethod: "BAD METHOD" },
    { ...defaultPreferences(), defaultBodyMode: "xml" },
    { ...defaultPreferences(), pretty: "true" },
    { ...defaultPreferences(), wrap: 1 },
    { ...defaultPreferences(), confirmCloseDrafts: null },
  ])("rejects malformed preferences %j", (preferences) => {
    expect(validPreferences(preferences)).toBe(false);
    const session = createSession();
    const raw = JSON.parse(encodeWorkspace([session], session.id));
    raw.preferences = preferences;
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow();
  });

  it.each([
    { defaultMethod: "BAD METHOD" },
    { defaultMethod: null },
    { defaultUrl: false },
    { defaultUrl: null },
    { defaultUrl: "x".repeat(65537) },
  ])("rejects malformed group defaults", (changes) => {
    const session = createSession();
    const raw = JSON.parse(encodeWorkspace([session], session.id, groups));
    Object.assign(raw.groups[0], changes);
    expect(() => decodeWorkspace(JSON.stringify(raw))).toThrow();
  });

  it("duplicates structured authorization without sharing mutations", () => {
    const source = createSession();
    source.draft.localAuth = {
      type: "basic",
      username: "demo",
      password: "synthetic",
    };
    const duplicate = createSession(source.draft);
    expect(duplicate.draft.localAuth).toEqual(source.draft.localAuth);
    if (duplicate.draft.localAuth?.type === "basic")
      duplicate.draft.localAuth.password = "changed";
    expect(source.draft.localAuth.password).toBe("synthetic");
    expect(duplicate.response).toBeNull();
  });

  it("uses the nearest group defaults without changing an existing draft", () => {
    const draft = createDraft();
    const defaults = resolveNewRequestDefaults(
      [
        {
          id: 1,
          name: "API",
          parentId: null,
          collapsed: false,
          defaultMethod: "POST",
          defaultUrl: "https://api.test",
        },
      ],
      1,
      defaultPreferences(),
    );
    const session = createSession(draft);
    applyNewRequestDefaults(session, defaults);
    expect(session.draft).toMatchObject({
      method: "POST",
      url: "https://api.test",
    });
    expect(draft).toMatchObject({ method: "GET", url: "" });
  });
});

describe("transport preferences", () => {
  it("fills missing transport fields from defaults", () => {
    expect(
      normalizePreferences({
        defaultMethod: "POST",
        defaultBodyMode: "json",
        pretty: false,
        wrap: true,
        confirmCloseDrafts: false,
      }),
    ).toEqual({
      ...defaultPreferences(),
      defaultMethod: "POST",
      defaultBodyMode: "json",
      pretty: false,
      wrap: true,
      confirmCloseDrafts: false,
    });
  });
  it("rejects wrong types and out-of-range values", () => {
    expect(normalizePreferences(null)).toBeNull();
    expect(normalizePreferences([])).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), timeoutSeconds: "30" }),
    ).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), maxRedirects: 21 }),
    ).toBeNull();
    expect(
      normalizePreferences({ ...defaultPreferences(), followRedirects: 1 }),
    ).toBeNull();
  });
  it("drops unknown keys", () => {
    expect(
      normalizePreferences({ ...defaultPreferences(), extra: true }),
    ).toEqual(defaultPreferences());
  });
  it("strict validation needs every field", () => {
    const partial: Record<string, unknown> = { ...defaultPreferences() };
    delete partial.timeoutSeconds;
    expect(validPreferences(partial)).toBe(false);
    expect(validPreferences(defaultPreferences())).toBe(true);
  });
  it("extracts transport options", () => {
    expect(transportOptions(defaultPreferences())).toEqual({
      timeoutSeconds: 30,
      connectTimeoutSeconds: 10,
      followRedirects: false,
      maxRedirects: 10,
      inspectionLimitMiB: 4,
      verifyTls: true,
      proxyUrl: "",
    });
  });
  it("rejects a proxy URL the desktop transport cannot use", () => {
    expect(
      validPreferences({ ...defaultPreferences(), proxyUrl: "ftp://proxy" }),
    ).toBe(false);
    expect(
      validPreferences({
        ...defaultPreferences(),
        proxyUrl: "socks5://127.0.0.1:1080",
      }),
    ).toBe(true);
  });
  it("loads a v4 workspace saved before the transport fields", () => {
    const session = createSession(createDraft());
    const encoded = JSON.parse(
      encodeWorkspace([session], session.id, [], {}, defaultPreferences()),
    );
    for (const key of [
      "timeoutSeconds",
      "connectTimeoutSeconds",
      "followRedirects",
      "maxRedirects",
      "inspectionLimitMiB",
      "verifyTls",
      "proxyUrl",
    ])
      delete encoded.preferences[key];
    expect(decodeWorkspace(JSON.stringify(encoded)).preferences).toEqual(
      defaultPreferences(),
    );
  });
});
