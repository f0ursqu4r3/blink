import { describe, expect, it } from "vitest";
import { definitionsToRows, rowsToDefinitions } from "../definitions";
import { pair } from "../request";

describe("definitionsToRows", () => {
  it("gives one blank row when there are no definitions", () => {
    const rows = definitionsToRows({});
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ key: "", value: "" });
  });

  it("gives one row for each definition", () => {
    expect(definitionsToRows({ a: "1", b: "2" })).toMatchObject([
      { key: "a", value: "1" },
      { key: "b", value: "2" },
    ]);
  });
});

describe("rowsToDefinitions", () => {
  it("skips blank rows and trims names", () => {
    expect(rowsToDefinitions([pair(" host ", "x"), pair()])).toEqual({
      definitions: { host: "x" },
    });
  });

  it("keeps an empty value when the name is set", () => {
    expect(rowsToDefinitions([pair("empty", "")])).toEqual({
      definitions: { empty: "" },
    });
  });

  it.each([
    [[pair("", "x")], "Enter a name"],
    [[pair("_x", "1")], "must not start with _"],
    [[pair("a{b", "1")], "must not contain"],
    [[pair("a", "1"), pair("a", "2")], "more than once"],
  ])("rejects invalid rows", (rows, message) => {
    const result = rowsToDefinitions(rows);
    expect("error" in result && result.error).toContain(message);
  });
});
