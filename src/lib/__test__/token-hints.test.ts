import { describe, expect, it } from "vitest";
import {
  applyTokenOption,
  resolveForDisplay,
  matchTokenOptions,
  tokenOptions,
  tokenQueryAt,
  tokenHint,
  tokenSpans,
  tokenValue,
} from "../token-hints";

const ctx = {
  definitions: { endpoint: "users", host: "local" },
  workspaceDefinitions: { host: "global", apiKey: "secret" },
};

describe("tokenOptions", () => {
  it("lists local names, unshadowed global names, then _. names", () => {
    expect(tokenOptions(ctx)).toEqual([
      { name: "endpoint", scope: "local", value: "users" },
      { name: "host", scope: "local", value: "local" },
      { name: "apiKey", scope: "global", value: "secret" },
      { name: "_.host", scope: "global", value: "global" },
      { name: "_.apiKey", scope: "global", value: "secret" },
    ]);
  });

  it("gives no options without a context", () => {
    expect(tokenOptions()).toEqual([]);
  });
});

describe("tokenSpans", () => {
  it("marks resolved, unresolved and environment references", () => {
    expect(
      tokenSpans(
        "https://x/{{endpoint}}/{{nope}}?k={{_.apiKey}}&e={{!HOME}}",
        ctx,
      ),
    ).toEqual([
      { text: "https://x/" },
      { text: "{{endpoint}}", token: "resolved", name: "endpoint" },
      { text: "/" },
      { text: "{{nope}}", token: "unresolved", name: "nope" },
      { text: "?k=" },
      { text: "{{_.apiKey}}", token: "resolved", name: "_.apiKey" },
      { text: "&e=" },
      { text: "{{!HOME}}", token: "env" },
    ]);
  });

  it("treats a _. reference to a local-only name as unresolved", () => {
    expect(tokenSpans("{{_.endpoint}}", ctx)[0].token).toBe("unresolved");
  });
});

describe("tokenValue", () => {
  it("resolves nested references", () => {
    const nested = {
      definitions: { url: "https://{{_.host}}/{{path}}", path: "v1" },
      workspaceDefinitions: { host: "api.test" },
    };
    expect(tokenValue("url", nested)).toBe("https://api.test/v1");
  });

  it("gives the raw value when a nested reference is missing", () => {
    const broken = { definitions: { url: "{{nope}}/x" } };
    expect(tokenValue("url", broken)).toBe("{{nope}}/x");
  });

  it("gives undefined for an unknown name", () => {
    expect(tokenValue("nope", ctx)).toBeUndefined();
  });
});

describe("tokenHint", () => {
  it("shows the value, or says the token is not defined", () => {
    const [ok, , missing] = tokenSpans("{{host}}/{{nope}}", ctx);
    expect(tokenHint(ok, ctx)).toBe("host = local");
    expect(tokenHint(missing, ctx)).toBe("nope is not defined");
  });
});

describe("tokenQueryAt", () => {
  it("finds the open reference before the cursor", () => {
    expect(tokenQueryAt("https://x/{{end", 15)).toEqual({
      from: 12,
      query: "end",
    });
  });

  it("ignores closed references", () => {
    expect(tokenQueryAt("{{endpoint}}/", 13)).toBeNull();
  });
});

describe("matchTokenOptions", () => {
  it("puts prefix matches first", () => {
    const names = matchTokenOptions(tokenOptions(ctx), "host").map(
      (o) => o.name,
    );
    expect(names).toEqual(["host", "_.host"]);
  });
});

describe("applyTokenOption", () => {
  it("completes the name and closes the braces", () => {
    expect(applyTokenOption("a/{{en", 4, 6, "endpoint")).toEqual({
      text: "a/{{endpoint}}",
      cursor: 14,
    });
  });

  it("reuses braces already after the cursor", () => {
    expect(applyTokenOption("a/{{en}}/b", 4, 6, "endpoint")).toEqual({
      text: "a/{{endpoint}}/b",
      cursor: 14,
    });
  });
});

describe("resolveForDisplay", () => {
  it("replaces defined tokens and keeps the rest as typed", () => {
    expect(
      resolveForDisplay(
        "https://{{_.host}}/{{endpoint}}/{{nope}}/{{!HOME}}",
        ctx,
      ),
    ).toBe("https://global/users/{{nope}}/{{!HOME}}");
  });
});
