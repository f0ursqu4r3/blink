import { describe, expect, it } from "vitest";
import { buildRequest, createDraft, pair } from "../request";
import { interpolate } from "../interpolation";

// ---------------------------------------------------------------------------
// interpolate() — pure function tests
// ---------------------------------------------------------------------------
describe("interpolate", () => {
  it("resolves a local {{name}} reference", () => {
    expect(
      interpolate("Hello {{name}}", { definitions: { name: "World" } }),
    ).toBe("Hello World");
  });

  it("resolves {{_.name}} from workspace definitions only", () => {
    expect(
      interpolate("{{_.token}}", {
        definitions: {},
        workspaceDefinitions: { token: "ws-tok" },
      }),
    ).toBe("ws-tok");
  });

  it("{{_.name}} does NOT resolve from local definitions", () => {
    expect(() =>
      interpolate("{{_.token}}", {
        definitions: { token: "local" },
        workspaceDefinitions: {},
      }),
    ).toThrow("token");
  });

  it("keeps {{!NAME}} environment references for the native transport", () => {
    expect(
      interpolate("Bearer {{!API_TOKEN}} {{nested}}", {
        definitions: { nested: "{{!OTHER}}" },
      }),
    ).toBe("Bearer {{!API_TOKEN}} {{!OTHER}}");
  });

  it("keeps << and >> as literal text", () => {
    expect(interpolate("a << b >> c", { definitions: {} })).toBe("a << b >> c");
  });

  it("local definitions shadow workspace for plain {{name}}", () => {
    expect(
      interpolate("{{x}}", {
        definitions: { x: "local" },
        workspaceDefinitions: { x: "workspace" },
      }),
    ).toBe("local");
  });

  it("falls back to workspace for {{name}} when absent from local", () => {
    expect(
      interpolate("{{x}}", {
        definitions: {},
        workspaceDefinitions: { x: "workspace" },
      }),
    ).toBe("workspace");
  });

  it("handles multiple references and literal text in a single pass", () => {
    expect(
      interpolate("{{a}} and {{b}}", {
        definitions: { a: "foo", b: "bar" },
      }),
    ).toBe("foo and bar");
  });

  it("resolves tokens in token values", () => {
    expect(
      interpolate("{{a}}", { definitions: { a: "{{b}}", b: "expanded" } }),
    ).toBe("expanded");
  });

  it("resolves global tokens referenced by token values", () => {
    expect(
      interpolate("{{requestUrl}}", {
        definitions: { requestUrl: "https://{{_.host}}/v1" },
        workspaceDefinitions: { host: "api.example.test" },
      }),
    ).toBe("https://api.example.test/v1");
  });

  it("rejects recursive token definitions without revealing values", () => {
    expect(() =>
      interpolate("{{a}}", { definitions: { a: "{{b}}", b: "{{a}}" } }),
    ).toThrow("Circular token reference");
  });

  it("throws a clear error naming the missing reference", () => {
    expect(() => interpolate("{{missing}}", { definitions: {} })).toThrow(
      "missing",
    );
  });

  it("error messages never reveal definition values", () => {
    const ctx = { definitions: { secret: "hunter2" } };
    let msg = "";
    try {
      interpolate("{{gone}}", ctx);
    } catch (e: unknown) {
      msg = (e as Error).message;
    }
    expect(msg).toMatch(/gone/);
    expect(msg).not.toContain("hunter2");
  });

  it("returns the original string unchanged when there are no tokens", () => {
    expect(interpolate("plain text", { definitions: {} })).toBe("plain text");
  });

  it("handles an empty template", () => {
    expect(interpolate("", { definitions: {} })).toBe("");
  });
});

// ---------------------------------------------------------------------------
// buildRequest() with optional InterpolationContext
// ---------------------------------------------------------------------------
describe("buildRequest with interpolation context", () => {
  const baseDraft = () => ({
    ...createDraft(),
    url: "https://example.test/resource",
  });

  // --- URL interpolation ---
  it("interpolates tokens in the base URL", () => {
    const d = baseDraft();
    d.url = "https://{{host}}/path";
    const req = buildRequest(d, { definitions: { host: "api.example.com" } });
    expect(req.url).toContain("https://api.example.com/path");
  });

  it("errors on a missing URL token, naming the reference", () => {
    const d = baseDraft();
    d.url = "https://{{host}}/path";
    expect(() => buildRequest(d, { definitions: {} })).toThrow("host");
  });

  // --- Query interpolation ---
  it("interpolates enabled query row values", () => {
    const d = baseDraft();
    d.query = [pair("filter", "{{val}}")];
    const req = buildRequest(d, { definitions: { val: "active" } });
    expect(req.url).toContain("filter=active");
  });

  it("interpolates enabled query row keys", () => {
    const d = baseDraft();
    d.query = [pair("{{paramName}}", "1")];
    const req = buildRequest(d, { definitions: { paramName: "page" } });
    expect(req.url).toContain("page=1");
  });

  // --- Header value interpolation (names must NOT interpolate) ---
  it("interpolates header values but leaves header names untouched", () => {
    const d = baseDraft();
    d.headers = [pair("X-Api-Key", "{{apiKey}}")];
    const req = buildRequest(d, { definitions: { apiKey: "secret123" } });
    expect(req.headers).toContainEqual({
      key: "X-Api-Key",
      value: "secret123",
    });
  });

  it("errors on a missing header value token", () => {
    const d = baseDraft();
    d.headers = [pair("X-Api-Key", "{{apiKey}}")];
    expect(() => buildRequest(d, { definitions: {} })).toThrow("apiKey");
  });

  // --- Body interpolation ---
  it("interpolates tokens in a text body", () => {
    const d = baseDraft();
    d.method = "POST";
    d.bodyMode = "text";
    d.body = "Hello {{name}}";
    const req = buildRequest(d, { definitions: { name: "world" } });
    expect(req.body).toBe("Hello world");
  });

  it("interpolates tokens in a JSON body, then validates the result", () => {
    const d = baseDraft();
    d.method = "POST";
    d.bodyMode = "json";
    d.body = '{"key":"{{val}}"}';
    const req = buildRequest(d, { definitions: { val: "hello" } });
    expect(req.body).toBe('{"key":"hello"}');
  });

  it("validates JSON body after interpolation, not before", () => {
    const d = baseDraft();
    d.method = "POST";
    d.bodyMode = "json";
    // Before interpolation this is not valid JSON; after it is.
    d.body = "{{jsonFragment}}";
    const req = buildRequest(d, {
      definitions: { jsonFragment: '{"ok":true}' },
    });
    expect(req.body).toBe('{"ok":true}');
  });

  it("rejects an invalid JSON body after interpolation", () => {
    const d = baseDraft();
    d.method = "POST";
    d.bodyMode = "json";
    d.body = "{{fragment}}";
    expect(() =>
      buildRequest(d, { definitions: { fragment: "not-json" } }),
    ).toThrow("Invalid JSON");
  });

  // --- Auth credential interpolation ---
  it("interpolates bearer token before building the Authorization header", () => {
    const d = baseDraft();
    d.auth = "bearer";
    d.token = "{{tok}}";
    const req = buildRequest(d, { definitions: { tok: "mytoken" } });
    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer mytoken",
    });
  });

  it("interpolates basic auth username and password", () => {
    const d = baseDraft();
    d.auth = "basic";
    d.username = "{{user}}";
    d.password = "{{pass}}";
    const req = buildRequest(d, {
      definitions: { user: "alice", pass: "s3cr3t" },
    });
    const auth = req.headers.find((h) => h.key === "Authorization")!.value;
    const expected =
      "Basic " +
      btoa(String.fromCharCode(...new TextEncoder().encode("alice:s3cr3t")));
    expect(auth).toBe(expected);
  });

  it("existing manual Authorization header conflict still triggers error after interpolation resolves auth", () => {
    const d = baseDraft();
    d.auth = "bearer";
    d.token = "{{tok}}";
    d.headers = [pair("authorization", "Basic abc")];
    expect(() => buildRequest(d, { definitions: { tok: "xyz" } })).toThrow(
      "Authorization",
    );
  });

  // --- Workspace-global tokens ---
  it("resolves {{_.name}} from workspaceDefinitions", () => {
    const d = baseDraft();
    d.url = "https://example.test/{{_.version}}/path";
    const req = buildRequest(d, {
      definitions: {},
      workspaceDefinitions: { version: "v2" },
    });
    expect(req.url).toContain("/v2/path");
  });

  // --- Backward compatibility ---
  it("backward compatible — no context argument still works", () => {
    const d = baseDraft();
    d.auth = "bearer";
    d.token = "plain-token";
    const req = buildRequest(d);
    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer plain-token",
    });
  });

  it("backward compatible — undefined context does not throw on literal values", () => {
    const d = baseDraft();
    expect(() => buildRequest(d)).not.toThrow();
  });
});
