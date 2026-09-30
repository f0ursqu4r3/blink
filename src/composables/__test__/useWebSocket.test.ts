import { describe, expect, it, vi } from "vitest";
import { effectScope, reactive } from "vue";
import type { SocketEvent } from "@/lib/websocket";

const sent: string[] = [];
let emit: (event: SocketEvent) => void = () => {};
const close = vi.fn();
vi.mock("@/lib/transport", () => ({ nativeTransport: true }));
vi.mock("@/lib/websocket", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/websocket")>()),
  openSocket: vi.fn((_url, _headers, _timeout, onEvent) => {
    emit = onEvent;
    return {
      send: async (text: string) => void sent.push(text),
      close,
    };
  }),
}));

import { useWebSocket } from "../useWebSocket";
import { createSession } from "@/lib/session";

describe("useWebSocket", () => {
  it("connects, logs messages both ways, and closes", async () => {
    const session = reactive(createSession());
    session.draft.url = "ws://chat.test";
    const scope = effectScope();
    const socket = scope.run(() => useWebSocket(session))!;
    socket.connect();
    expect(socket.state.value).toBe("connecting");
    expect(await socket.send("early")).toBe(false);
    emit({ kind: "open", status: 101, headers: [["x-a", "1"]] });
    expect(socket.state.value).toBe("open");
    expect(session.socket?.headers).toEqual([{ key: "x-a", value: "1" }]);
    expect(await socket.send("hi")).toBe(true);
    emit({ kind: "message", text: "yo", binary: false, size: 2 });
    socket.disconnect();
    expect(close).toHaveBeenCalled();
    emit({ kind: "close", code: 1000, reason: "bye" });
    expect(socket.state.value).toBe("closed");
    expect(session.socket?.messages.map((m) => [m.direction, m.text])).toEqual([
      ["system", "Connecting to ws://chat.test"],
      ["system", "Connected · 101"],
      ["out", "hi"],
      ["in", "yo"],
      ["system", "Closed · 1000 · bye"],
    ]);
    expect(sent).toEqual(["hi"]);
    socket.clear();
    expect(session.socket?.messages).toEqual([]);
    scope.stop();
  });
});
