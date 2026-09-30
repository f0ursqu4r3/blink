import { describe, expect, it } from "vitest";
import { buildWebSocketRequest, isWebSocketUrl } from "../websocket";
import { createDraft, pair } from "../request";
import { displayMethod, createSession, sessionLabel } from "../session";

describe("WebSocket requests", () => {
  it("detects ws and wss URLs", () => {
    expect(isWebSocketUrl(" wss://a.test/x")).toBe(true);
    expect(isWebSocketUrl("WS://a.test")).toBe(true);
    expect(isWebSocketUrl("https://a.test")).toBe(false);
  });
  it("resolves tokens, query rows, headers, and auth", () => {
    const draft = {
      ...createDraft(),
      url: "{{base}}/chat",
      query: [pair("room", "1")],
      headers: [pair("Accept", "application/json"), pair("X-Id", "{{id}}")],
      localAuth: { type: "bearer" as const, token: "t" },
    };
    const request = buildWebSocketRequest(draft, {
      definitions: { base: "wss://chat.test", id: "7" },
      auth: { type: "bearer", token: "t" },
    } as never);
    expect(request.url).toBe("wss://chat.test/chat?room=1");
    expect(request.headers).toEqual([
      { key: "X-Id", value: "7" },
      { key: "Authorization", value: "Bearer t" },
    ]);
  });
  it("rejects other schemes", () => {
    expect(() =>
      buildWebSocketRequest({ ...createDraft(), url: "https://a.test" }),
    ).toThrow("ws://");
  });
  it("labels a WebSocket request", () => {
    const session = createSession();
    session.draft.url = "ws://chat.test/room";
    expect(displayMethod(session)).toBe("WS");
    expect(sessionLabel(session)).toBe("/room");
  });
});
