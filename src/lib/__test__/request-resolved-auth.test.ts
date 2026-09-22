import { describe, expect, it } from "vitest";
import { buildRequest, createDraft } from "../request";
import type { ResolvedRequestContext } from "../authorization";

// Tests for buildRequest accepting a ResolvedRequestContext (resolved auth + tokens)
// The context replaces the old Draft.auth/token/username/password for header building
// when a resolved context is supplied.

describe("buildRequest with ResolvedRequestContext", () => {
  const base = () => ({ ...createDraft(), url: "https://example.test/api" });

  it("uses resolved bearer auth from context, ignoring draft flat auth", () => {
    const d = base();
    // draft has no auth, but resolved context provides bearer
    const ctx: ResolvedRequestContext = {
      auth: { type: "bearer", token: "resolved-tok" },
      definitions: {},
      workspaceDefinitions: {},
    };
    const req = buildRequest(d, ctx);
    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer resolved-tok",
    });
  });

  it("uses resolved basic auth from context", () => {
    const d = base();
    const ctx: ResolvedRequestContext = {
      auth: { type: "basic", username: "alice", password: "pw" },
      definitions: {},
      workspaceDefinitions: {},
    };
    const req = buildRequest(d, ctx);
    const authHeader = req.headers.find((h) => h.key === "Authorization")!;
    expect(authHeader.value).toMatch(/^Basic /);
    const decoded = atob(authHeader.value.slice("Basic ".length));
    expect(decoded).toBe("alice:pw");
  });

  it("resolved auth=none skips Authorization header", () => {
    const d = base();
    const ctx: ResolvedRequestContext = {
      auth: { type: "none" },
      definitions: {},
      workspaceDefinitions: {},
    };
    const req = buildRequest(d, ctx);
    expect(req.headers.some((h) => h.key === "Authorization")).toBe(false);
  });

  it("interpolates tokens from context definitions", () => {
    const d = base();
    d.url = "https://{{host}}/api";
    const ctx: ResolvedRequestContext = {
      auth: { type: "none" },
      definitions: { host: "api.example.com" },
      workspaceDefinitions: {},
    };
    const req = buildRequest(d, ctx);
    expect(req.url).toContain("api.example.com");
  });

  it("bearer token from context is interpolated using definitions", () => {
    const d = base();
    const ctx: ResolvedRequestContext = {
      auth: { type: "bearer", token: "{{myTok}}" },
      definitions: { myTok: "tok-value" },
      workspaceDefinitions: {},
    };
    const req = buildRequest(d, ctx);
    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer tok-value",
    });
  });

  it("backward compat: no context uses draft flat auth fields as before", () => {
    const d = base();
    d.auth = "bearer";
    d.token = "flat-tok";
    const req = buildRequest(d);
    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer flat-tok",
    });
  });

  it("uses the request local auth override when no context is supplied", () => {
    const d = base();
    d.localAuth = { type: "bearer", token: "local-tok" };

    const req = buildRequest(d);

    expect(req.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer local-tok",
    });
  });
});
