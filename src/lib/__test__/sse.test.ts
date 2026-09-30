import { describe, expect, it } from "vitest";
import {
  createSseParser,
  isEventStream,
  parseSse,
  type SseEvent,
} from "../sse";

describe("SSE parser", () => {
  it("parses events, names, ids, and multi-line data", () => {
    expect(
      parseSse(
        ": comment\nevent: tick\nid: 1\ndata: a\ndata: b\n\ndata:no space\n\nretry: 5\n\n",
      ),
    ).toEqual([
      { event: "tick", data: "a\nb", id: "1" },
      { event: "message", data: "no space", id: "1" },
    ]);
  });
  it("handles pieces split anywhere and CRLF", () => {
    const events: SseEvent[] = [];
    const parser = createSseParser((event) => events.push(event));
    for (const piece of ["da", "ta: x\r", "\n\r\n", "data: y\r\n", "\r\n"])
      parser.push(piece);
    expect(events.map((event) => event.data)).toEqual(["x", "y"]);
  });
  it("dispatches an event left open at the end", () => {
    expect(parseSse("data: last")).toEqual([
      { event: "message", data: "last" },
    ]);
  });
  it("detects event-stream content types", () => {
    expect(isEventStream("text/event-stream; charset=utf-8")).toBe(true);
    expect(isEventStream("text/plain")).toBe(false);
    expect(isEventStream(undefined)).toBe(false);
  });
});
