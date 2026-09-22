import { describe, expect, it } from "vitest";
import {
  resolveAuthorization,
  resolveTokenDefinitions,
  buildResolvedRequestContext,
  type AuthorizationConfig,
} from "../authorization";
import type { RequestGroup } from "../groups";
import { createDraft } from "../request";

// ---------------------------------------------------------------------------
// resolveAuthorization — inheritance chain
// ---------------------------------------------------------------------------
describe("resolveAuthorization", () => {
  const group = (
    id: number,
    parentId: number | null,
    localAuth?: AuthorizationConfig,
  ): RequestGroup => ({
    id,
    name: `G${id}`,
    parentId,
    collapsed: false,
    localAuth,
  });

  it("returns none when draft has no localAuth and no groups have auth", () => {
    const groups = [group(1, null)];
    expect(resolveAuthorization(undefined, 1, groups)).toEqual({
      type: "none",
    });
  });

  it("returns draft localAuth when set (overrides groups)", () => {
    const groups = [group(1, null, { type: "bearer", token: "g-tok" })];
    expect(
      resolveAuthorization({ type: "bearer", token: "d-tok" }, 1, groups),
    ).toEqual({
      type: "bearer",
      token: "d-tok",
    });
  });

  it("explicit none on draft blocks group auth", () => {
    const groups = [group(1, null, { type: "bearer", token: "g-tok" })];
    expect(resolveAuthorization({ type: "none" }, 1, groups)).toEqual({
      type: "none",
    });
  });

  it("inherits bearer from direct group when draft has no localAuth", () => {
    const groups = [group(1, null, { type: "bearer", token: "g-tok" })];
    expect(resolveAuthorization(undefined, 1, groups)).toEqual({
      type: "bearer",
      token: "g-tok",
    });
  });

  it("inherits from parent group when direct group has no localAuth", () => {
    const root = group(1, null, {
      type: "basic",
      username: "u",
      password: "p",
    });
    const child = group(2, 1); // no localAuth
    expect(resolveAuthorization(undefined, 2, [root, child])).toEqual({
      type: "basic",
      username: "u",
      password: "p",
    });
  });

  it("nearest ancestor wins over further ancestor", () => {
    const root = group(1, null, { type: "bearer", token: "root-tok" });
    const mid = group(2, 1, { type: "bearer", token: "mid-tok" });
    const leaf = group(3, 2); // no localAuth
    expect(resolveAuthorization(undefined, 3, [root, mid, leaf])).toEqual({
      type: "bearer",
      token: "mid-tok",
    });
  });

  it("returns none when no group has localAuth (ungrouped request)", () => {
    expect(resolveAuthorization(undefined, null, [])).toEqual({ type: "none" });
  });

  it("resolves bearer from group when draft inherits", () => {
    const groups = [group(5, null, { type: "bearer", token: "tok" })];
    expect(resolveAuthorization(undefined, 5, groups)).toEqual({
      type: "bearer",
      token: "tok",
    });
  });
});

// ---------------------------------------------------------------------------
// resolveTokenDefinitions — merging nearest-wins chain
// ---------------------------------------------------------------------------
describe("resolveTokenDefinitions", () => {
  const group = (
    id: number,
    parentId: number | null,
    localDefinitions?: Record<string, string>,
  ): RequestGroup => ({
    id,
    name: `G${id}`,
    parentId,
    collapsed: false,
    localDefinitions,
  });

  it("returns workspace global definitions when not in any group", () => {
    const result = resolveTokenDefinitions(null, [], { baseUrl: "https://ws" });
    expect(result.definitions).toEqual({});
    expect(result.workspaceDefinitions).toEqual({ baseUrl: "https://ws" });
  });

  it("merges group local definitions; group wins over workspace for same key", () => {
    const g = group(1, null, { env: "staging", host: "g-host" });
    const result = resolveTokenDefinitions(1, [g], {
      env: "prod",
      wsOnly: "yes",
    });
    expect(result.definitions).toEqual({ env: "staging", host: "g-host" });
    expect(result.workspaceDefinitions).toEqual({ env: "prod", wsOnly: "yes" });
  });

  it("nearer group definitions win over parent group definitions for same key", () => {
    const parent = group(1, null, { x: "parent-x", y: "parent-y" });
    const child = group(2, 1, { x: "child-x" });
    const result = resolveTokenDefinitions(2, [parent, child], {});
    expect(result.definitions).toEqual({ x: "child-x", y: "parent-y" });
  });

  it("accumulated definitions are passed as local, workspace-global separately", () => {
    const g = group(1, null, { local: "yes" });
    const result = resolveTokenDefinitions(1, [g], { global: "ws" });
    expect(result.definitions).toEqual({ local: "yes" });
    expect(result.workspaceDefinitions).toEqual({ global: "ws" });
  });
});

// ---------------------------------------------------------------------------
// buildResolvedRequestContext — combines auth + token resolution
// ---------------------------------------------------------------------------
describe("buildResolvedRequestContext", () => {
  it("resolves auth and definitions from group ancestry for a draft", () => {
    const group: RequestGroup = {
      id: 1,
      name: "API",
      parentId: null,
      collapsed: false,
      localAuth: { type: "bearer", token: "grp-tok" },
      localDefinitions: { host: "api.example.com" },
    };
    const draft = { ...createDraft(), localAuth: undefined };
    const ctx = buildResolvedRequestContext(draft, 1, [group], {
      apiVersion: "v2",
    });
    expect(ctx.auth).toEqual({ type: "bearer", token: "grp-tok" });
    expect(ctx.definitions).toEqual({ host: "api.example.com" });
    expect(ctx.workspaceDefinitions).toEqual({ apiVersion: "v2" });
  });

  it("draft explicit auth overrides group auth", () => {
    const group: RequestGroup = {
      id: 1,
      name: "API",
      parentId: null,
      collapsed: false,
      localAuth: { type: "bearer", token: "g-tok" },
    };
    const draft = {
      ...createDraft(),
      localAuth: { type: "basic" as const, username: "u", password: "p" },
    };
    const ctx = buildResolvedRequestContext(draft, 1, [group], {});
    expect(ctx.auth).toEqual({ type: "basic", username: "u", password: "p" });
  });

  it("ungrouped draft with no localAuth gets auth:none and empty definitions", () => {
    const draft = { ...createDraft(), localAuth: undefined };
    const ctx = buildResolvedRequestContext(draft, null, [], {});
    expect(ctx.auth).toEqual({ type: "none" });
    expect(ctx.definitions).toEqual({});
    expect(ctx.workspaceDefinitions).toEqual({});
  });
});
