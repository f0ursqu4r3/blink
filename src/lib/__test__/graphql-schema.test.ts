import { afterEach, describe, expect, it, vi } from "vitest";
import { buildSchema, introspectionFromSchema } from "graphql";
import { createDraft, type Draft } from "../request";
import type { ResolvedRequestContext } from "../authorization";

vi.mock("../transport", () => ({ sendRequest: vi.fn() }));
import { sendRequest } from "../transport";
import {
  clearSchemaCache,
  fetchSchema,
  formatSchemaAge,
  getCachedSchema,
  schemaKey,
} from "../graphql-schema";

const send = vi.mocked(sendRequest);
const introspection = introspectionFromSchema(
  buildSchema("type Query { viewer: User } type User { id: ID! }"),
);
const url = "https://api.test/graphql";

function respond(body: string, status = 200, statusText = "OK") {
  send.mockResolvedValueOnce({
    status,
    statusText,
    durationMs: 1,
    sizeBytes: body.length,
    headers: [],
    body,
  });
}

function draft(overrides: Partial<Draft> = {}): Draft {
  return {
    ...createDraft(),
    url,
    method: "GET",
    bodyMode: "json",
    body: '{"ignored":true}',
    variables: '{"x":1}',
    headers: [{ id: 901, key: "X-Api-Key", value: "{{key}}", enabled: true }],
    ...overrides,
  };
}

const ctx: ResolvedRequestContext = {
  definitions: { key: "secret" },
  auth: { type: "bearer", token: "tok" },
};

afterEach(() => {
  send.mockReset();
  clearSchemaCache();
});

describe("fetchSchema", () => {
  it("sends a POST introspection with the draft's headers and auth", async () => {
    respond(JSON.stringify({ data: introspection }));
    await fetchSchema(draft(), ctx);
    const request = send.mock.calls[0][0];
    expect(request.method).toBe("POST");
    expect(request.url).toBe(url);
    const body = JSON.parse(request.body ?? "");
    expect(body.query).toContain("__schema");
    expect(body).not.toHaveProperty("variables");
    expect(request.headers).toContainEqual({
      key: "X-Api-Key",
      value: "secret",
    });
    expect(request.headers).toContainEqual({
      key: "Authorization",
      value: "Bearer tok",
    });
  });

  it("returns the schema and caches it by URL", async () => {
    respond(JSON.stringify({ data: introspection }));
    const schema = await fetchSchema(draft(), ctx);
    expect(schema.getQueryType()?.getFields()).toHaveProperty("viewer");
    const cached = getCachedSchema(url);
    expect(cached?.schema).toBe(schema);
    expect(typeof cached?.fetchedAt).toBe("number");
  });

  it("does not modify the draft", async () => {
    respond(JSON.stringify({ data: introspection }));
    const input = draft();
    const before = JSON.stringify(input);
    await fetchSchema(input, ctx);
    expect(JSON.stringify(input)).toBe(before);
  });

  it("rethrows buildRequest errors", async () => {
    await expect(fetchSchema(draft({ url: "not a url" }), ctx)).rejects.toThrow(
      "Enter an absolute",
    );
    expect(send).not.toHaveBeenCalled();
  });

  it("rethrows transport errors", async () => {
    send.mockRejectedValueOnce(
      new Error("Request timed out after 30 seconds."),
    );
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Request timed out after 30 seconds.",
    );
  });

  it("reports non-2xx status", async () => {
    respond("nope", 401, "Unauthorized");
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Schema request failed: 401 Unauthorized",
    );
  });

  it("reports non-2xx status without status text", async () => {
    respond("nope", 500, "");
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      /^Schema request failed: 500$/,
    );
  });

  it.each([
    ["non-JSON body", "<html>"],
    ["JSON null", "null"],
    ["no __schema", JSON.stringify({ data: {} })],
    [
      "malformed __schema",
      JSON.stringify({ data: { __schema: { types: 1 } } }),
    ],
  ])("reports %s as not an introspection result", async (_, body) => {
    respond(body);
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Endpoint did not return an introspection result. Introspection may be disabled.",
    );
  });

  it("reports the first GraphQL error when there is no schema", async () => {
    respond(
      JSON.stringify({
        errors: [{ message: "Introspection is disabled" }, { message: "x" }],
      }),
    );
    await expect(fetchSchema(draft(), ctx)).rejects.toThrow(
      "Introspection is disabled",
    );
  });

  it("loads the schema when errors accompany a usable schema", async () => {
    respond(
      JSON.stringify({ data: introspection, errors: [{ message: "partial" }] }),
    );
    const schema = await fetchSchema(draft(), ctx);
    expect(schema.getQueryType()).toBeTruthy();
  });

  it("does not cache on failure", async () => {
    respond("nope", 401, "Unauthorized");
    await fetchSchema(draft(), ctx).catch(() => {});
    expect(getCachedSchema(url)).toBeUndefined();
  });
});

describe("schemaKey", () => {
  it("returns the resolved URL including query params", () => {
    const d = draft({
      url: "https://{{host}}/graphql",
      query: [{ id: 902, key: "v", value: "2", enabled: true }],
    });
    expect(
      schemaKey(d, {
        definitions: { host: "api.test", key: "k" },
        auth: { type: "none" },
      }),
    ).toBe("https://api.test/graphql?v=2");
  });

  it("returns null instead of throwing for an invalid URL", () => {
    expect(schemaKey(draft({ url: "" }), ctx)).toBeNull();
  });
});

describe("formatSchemaAge", () => {
  const t = 1_000_000_000;
  it.each([
    [0, "just now"],
    [59_000, "just now"],
    [60_000, "1m ago"],
    [59 * 60_000, "59m ago"],
    [60 * 60_000, "1h ago"],
    [5 * 60 * 60_000, "5h ago"],
  ])("formats %i ms as %s", (elapsed, expected) => {
    expect(formatSchemaAge(t, t + elapsed)).toBe(expected);
  });
});
