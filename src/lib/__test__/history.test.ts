import { describe, expect, it } from "vitest";
import {
  addHistory,
  diffText,
  HISTORY_BODY_LIMIT,
  HISTORY_LIMIT,
  historyEntry,
  nextHistoryId,
  relativeTime,
} from "../history";
import type { ApiResponse, RequestInput } from "../request";

const request: RequestInput = {
  method: "GET",
  url: "https://example.test",
  headers: [],
  body: null,
};
const response = (body: string, extra: Partial<ApiResponse> = {}) => ({
  status: 200,
  statusText: "OK",
  durationMs: 5,
  sizeBytes: body.length,
  headers: [{ key: "A", value: "1" }],
  body,
  bodyId: "x",
  ...extra,
});

describe("history", () => {
  it("keeps the response without its body id", () => {
    const entry = historyEntry(1, 100, request, { response: response("{}") });
    expect(entry).toMatchObject({ status: 200, body: "{}", method: "GET" });
    expect(entry).not.toHaveProperty("bodyId");
  });
  it("omits large, truncated, and binary bodies", () => {
    for (const r of [
      response("x".repeat(HISTORY_BODY_LIMIT + 1)),
      response("x", { truncated: true }),
      response("", { binary: true }),
    ]) {
      const entry = historyEntry(1, 0, request, { response: r });
      expect(entry.body).toBe("");
      expect(entry.bodyOmitted).toBe(true);
    }
  });
  it("records errors", () => {
    const entry = historyEntry(2, 0, request, {
      error: "Boom",
      durationMs: 3,
    });
    expect(entry).toMatchObject({ error: "Boom", durationMs: 3 });
    expect(diffText(entry)).toEqual(["Error: Boom"]);
  });
  it("puts the newest first and caps the list", () => {
    let list = undefined as ReturnType<typeof addHistory> | undefined;
    for (let i = 1; i <= HISTORY_LIMIT + 3; i++)
      list = addHistory(
        list,
        historyEntry(i, i, request, { response: response("") }),
      );
    expect(list!.length).toBe(HISTORY_LIMIT);
    expect(list![0].id).toBe(HISTORY_LIMIT + 3);
    expect(nextHistoryId(list)).toBe(HISTORY_LIMIT + 4);
    expect(nextHistoryId(undefined)).toBe(1);
  });
  it("formats JSON bodies for diffs", () => {
    const entry = historyEntry(1, 0, request, {
      response: response('{"a":1}'),
    });
    expect(diffText(entry)).toEqual(["{", '  "a": 1', "}"]);
  });
  it("describes relative times", () => {
    expect(relativeTime(0, 10_000)).toBe("just now");
    expect(relativeTime(0, 5 * 60_000)).toBe("5 min ago");
    expect(relativeTime(0, 3 * 3_600_000)).toBe("3 h ago");
  });
});
