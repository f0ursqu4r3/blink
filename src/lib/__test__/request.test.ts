import { describe, expect, it } from "vitest";
import { buildRequest, createDraft, pair, toCurl } from "../request";
import { formatJson } from "../json";

describe("request construction", () => {
  const draft = () => ({
    ...createDraft(),
    url: "https://example.test/resource?a=1%20two",
  });
  it("preserves a signed query string without additional rows", () => {
    expect(buildRequest(draft()).url).toBe(
      "https://example.test/resource?a=1%20two",
    );
  });
  it("appends query rows without rewriting the original encoding", () => {
    const request = draft();
    request.url = "https://example.test/%2f?token=a%20b&path=%2f#fragment";
    request.query = [pair("filter", "two words")];
    expect(buildRequest(request).url).toBe(
      "https://example.test/%2f?token=a%20b&path=%2f&filter=two+words",
    );
    for (const url of [
      "https://example.test",
      "https://example.test?",
      "https://example.test?existing=1&",
    ]) {
      request.url = url;
      const result = new URL(buildRequest(request).url);
      expect(result.searchParams.get("filter")).toBe("two words");
      expect(result.search).not.toContain("??");
      expect(result.search).not.toContain("&&");
    }
  });
  it("appends enabled duplicate query parameters", () => {
    const request = draft();
    request.query = [
      pair("a", "three"),
      pair("a", "four"),
      { ...pair("ignored", "x"), enabled: false },
    ];
    expect(new URL(buildRequest(request).url).searchParams.getAll("a")).toEqual(
      ["1 two", "three", "four"],
    );
  });
  it("preserves body whitespace and adds JSON content type", () => {
    const request = draft();
    request.method = "POST";
    request.bodyMode = "json";
    request.body = '  {"ok":true}\n';
    expect(buildRequest(request).body).toBe(request.body);
    expect(buildRequest(request).headers).toContainEqual({
      key: "Content-Type",
      value: "application/json",
    });
  });
  it("gives text bodies the same content type in native, preview and cURL", () => {
    const request = draft();
    request.method = "POST";
    request.bodyMode = "text";
    request.body = "plain text";
    expect(buildRequest(request).headers).toContainEqual({
      key: "Content-Type",
      value: "text/plain; charset=utf-8",
    });
    request.headers.push(pair("Content-Type", "application/xml"));
    expect(
      buildRequest(request).headers.filter(
        (h) => h.key.toLowerCase() === "content-type",
      ),
    ).toEqual([{ key: "Content-Type", value: "application/xml" }]);
  });
  it("wraps GraphQL query and variables in a JSON payload", () => {
    const request = draft();
    request.method = "POST";
    request.bodyMode = "graphql";
    request.body = "query Q($id: ID!) { node(id: $id) { id } }";
    request.variables = '{"id":"{{id}}"}';
    const built = buildRequest(request, { definitions: { id: "42" } });
    expect(JSON.parse(built.body!)).toEqual({
      query: request.body,
      variables: { id: "42" },
    });
    expect(built.headers).toContainEqual({
      key: "Content-Type",
      value: "application/json",
    });
    request.variables = "  ";
    expect(JSON.parse(buildRequest(request).body!)).toEqual({
      query: request.body,
    });
  });
  it("rejects empty GraphQL queries and invalid variables", () => {
    const request = draft();
    request.method = "POST";
    request.bodyMode = "graphql";
    request.body = "  ";
    expect(() => buildRequest(request)).toThrow("GraphQL query");
    request.body = "{ ok }";
    request.variables = "{";
    expect(() => buildRequest(request)).toThrow("GraphQL variables");
    request.variables = "[1]";
    expect(() => buildRequest(request)).toThrow("GraphQL variables");
  });
  it("rejects relative or malformed HTTP URLs", () => {
    for (const url of [
      "https:example.test",
      "http:/example.test",
      "http:///example.test",
      "/resource",
    ]) {
      expect(() => buildRequest({ ...draft(), url })).toThrow();
    }
  });
  it("omits GET and HEAD bodies without destroying the draft", () => {
    const request = draft();
    request.bodyMode = "json";
    request.body = "invalid retained draft";
    expect(buildRequest(request).body).toBeNull();
    request.method = "HEAD";
    expect(buildRequest(request).body).toBeNull();
    expect(request.body).toBe("invalid retained draft");
  });
  it("rejects invalid JSON, unsupported schemes, and header injection", () => {
    const request = draft();
    request.method = "POST";
    request.bodyMode = "json";
    request.body = "{";
    expect(() => buildRequest(request)).toThrow("Invalid JSON");
    request.url = "file:///etc/hosts";
    expect(() => buildRequest(request)).toThrow("HTTP");
    request.url = "https://example.test";
    request.headers = [pair("X-Test", "a\r\nInjected: b")];
    expect(() => buildRequest(request)).toThrow("Line breaks");
  });
  it("detects conflicting authorization headers", () => {
    const request = draft();
    request.auth = "bearer";
    request.token = "synthetic";
    request.headers = [pair("authorization", "Basic abc")];
    expect(() => buildRequest(request)).toThrow("Authorization");
  });
  it("encodes Unicode basic authentication", () => {
    const request = draft();
    request.auth = "basic";
    request.username = "tést";
    request.password = "value";
    const authorization = buildRequest(request).headers.find(
      (h) => h.key === "Authorization",
    )!.value;
    expect(authorization).toBe(
      "Basic " +
        btoa(String.fromCharCode(...new TextEncoder().encode("tést:value"))),
    );
  });
  it("quotes shell text and uses HEAD correctly", () => {
    const request = buildRequest(draft());
    request.method = "HEAD";
    request.headers = [{ key: "X-Test", value: "a'b $(echo bad)" }];
    const command = toCurl(request);
    expect(command).toContain("--head");
    expect(command).not.toContain("--request HEAD");
    expect(command).toContain("'X-Test: a'\"'\"'b $(echo bad)'");
    expect(command).toContain(" \\\n  ");
  });
  it("formats JSON without rounding large identifiers", () => {
    expect(
      formatJson('{"id":9223372036854775807,"decimal":0.1234567890123456789}'),
    ).toContain("9223372036854775807");
    expect(formatJson('{"v":0.1234567890123456789}')).toContain(
      "0.1234567890123456789",
    );
  });
});
