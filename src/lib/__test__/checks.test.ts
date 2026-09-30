import { describe, expect, it } from "vitest";
import {
  compare,
  createAssertion,
  createCapture,
  describeAssertion,
  runAssertions,
  runCaptures,
} from "../checks";
import type { ApiResponse } from "../request";

const response: ApiResponse = {
  status: 201,
  statusText: "Created",
  durationMs: 120,
  sizeBytes: 64,
  headers: [{ key: "Content-Type", value: "application/json; charset=utf-8" }],
  body: JSON.stringify({ id: 7, token: "abc", tags: ["a", "b"], none: null }),
};

describe("compare", () => {
  it("compares numbers, JSON, and text", () => {
    expect(compare("201", "equals", "201.0")).toBe(true);
    expect(compare('{"a": 1}', "equals", '{"a":1}')).toBe(true);
    expect(compare("120", "lt", "200")).toBe(true);
    expect(compare("abc", "gt", "1")).toBe(false);
    expect(compare("hello", "contains", "ell")).toBe(true);
    expect(compare("hello", "matches", "^h.*o$")).toBe(true);
    expect(compare(undefined, "exists", "")).toBe(false);
    expect(compare(undefined, "notExists", "")).toBe(true);
    expect(compare(undefined, "equals", "x")).toBe(false);
    expect(compare(undefined, "notEquals", "x")).toBe(true);
  });
});

describe("runAssertions", () => {
  it("reads status, time, headers, and jq values", async () => {
    const results = await runAssertions(
      [
        createAssertion("status", "equals", "201"),
        createAssertion("time", "lt", "100"),
        createAssertion("header", "contains", "json", "content-type"),
        createAssertion("json", "equals", "abc", ".token"),
        createAssertion("json", "equals", '["a","b"]', ".tags"),
        createAssertion("json", "notExists", "", ".none"),
        createAssertion("json", "equals", "1", ".["),
        { ...createAssertion("status", "equals", "500"), enabled: false },
      ],
      response,
    );
    expect(results.map((r) => r.pass)).toEqual([
      true,
      false,
      true,
      true,
      true,
      true,
      false,
    ]);
    expect(results[1].actual).toBe("120");
    expect(results[6].actual).not.toBe("");
  });
  it("fails body checks on a truncated body", async () => {
    const [result] = await runAssertions(
      [createAssertion("body", "contains", "x")],
      { ...response, truncated: true },
    );
    expect(result.pass).toBe(false);
    expect(result.actual).toContain("truncated");
  });
  it("describes an assertion", () => {
    expect(describeAssertion(createAssertion("json", "gte", "2", ".n"))).toBe(
      "JSON (jq) .n ≥ 2",
    );
    expect(describeAssertion(createAssertion("status", "exists", ""))).toBe(
      "Status exists",
    );
  });
});

describe("runCaptures", () => {
  it("captures named values and reports misses", async () => {
    const { values, errors } = await runCaptures(
      [
        createCapture("token", "json", ".token"),
        createCapture("type", "header", "content-type"),
        createCapture("missing", "json", ".nothing"),
        createCapture("bad name", "json", ".id"),
      ],
      response,
    );
    expect(values).toEqual({
      token: "abc",
      type: "application/json; charset=utf-8",
    });
    expect(errors).toEqual(["missing: no value"]);
  });
});
