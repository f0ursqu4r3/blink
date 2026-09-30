import { computed, onScopeDispose, toValue, type MaybeRefOrGetter } from "vue";
import type { ResolvedRequestContext } from "@/lib/authorization";
import type { RequestSession } from "@/lib/session";
import type { TransportOptions } from "@/lib/transport-options";
import { nativeTransport } from "@/lib/transport";
import {
  buildWebSocketRequest,
  openSocket,
  SOCKET_MESSAGE_LIMIT,
  type SocketEvent,
  type SocketHandle,
  type SocketMessage,
} from "@/lib/websocket";

/** Connect, send, and log messages for a WebSocket request. */
export function useWebSocket(
  session: RequestSession,
  contextSource?: MaybeRefOrGetter<ResolvedRequestContext | undefined>,
  optionsSource?: MaybeRefOrGetter<TransportOptions | undefined>,
) {
  let handle: SocketHandle | undefined;
  let sequence = 0;
  const prepared = computed(() => {
    try {
      return {
        request: buildWebSocketRequest(session.draft, toValue(contextSource)),
        error: "",
      };
    } catch (cause) {
      return {
        request: null,
        error: cause instanceof Error ? cause.message : String(cause),
      };
    }
  });
  const state = computed(() => session.socket?.state ?? "closed");
  const active = computed(
    () => state.value === "open" || state.value === "connecting",
  );

  function log(
    direction: SocketMessage["direction"],
    text: string,
    extra: Partial<SocketMessage> = {},
  ) {
    const socket = session.socket;
    if (!socket) return;
    socket.messages.push({
      id: ++sequence,
      direction,
      text,
      at: Date.now(),
      size: text.length,
      ...extra,
    });
    if (socket.messages.length > SOCKET_MESSAGE_LIMIT)
      socket.messages.splice(0, socket.messages.length - SOCKET_MESSAGE_LIMIT);
  }
  function onEvent(event: SocketEvent) {
    const socket = session.socket;
    if (!socket) return;
    if (event.kind === "open") {
      socket.state = "open";
      socket.headers = event.headers.map(([key, value]) => ({ key, value }));
      log("system", `Connected · ${event.status}`);
    } else if (event.kind === "message")
      log("in", event.text, { binary: event.binary, size: event.size });
    else {
      socket.state = "closed";
      handle = undefined;
      log(
        "system",
        event.kind === "error"
          ? event.message
          : `Closed${event.code === null ? "" : ` · ${event.code}`}${event.reason ? ` · ${event.reason}` : ""}`,
      );
    }
  }
  function connect() {
    const request = prepared.value.request;
    if (!request || active.value) return;
    session.socket = {
      state: "connecting",
      messages: session.socket?.messages ?? [],
      headers: [],
    };
    log("system", `Connecting to ${request.url}`);
    if (!nativeTransport && request.headers.length)
      log(
        "system",
        "The browser preview cannot send headers or auth. Use the desktop app.",
      );
    const options = toValue(optionsSource);
    handle = openSocket(
      request.url,
      request.headers,
      options?.connectTimeoutSeconds ?? 10,
      onEvent,
    );
  }
  function disconnect() {
    if (!session.socket || !handle) return;
    session.socket.state = "closing";
    handle.close();
  }
  async function send(text: string) {
    if (state.value !== "open" || !handle || !text) return false;
    try {
      await handle.send(text);
      log("out", text);
      return true;
    } catch (cause) {
      log("system", cause instanceof Error ? cause.message : String(cause));
      return false;
    }
  }
  function clear() {
    if (session.socket) session.socket.messages = [];
  }
  onScopeDispose(() => handle?.close());

  return { prepared, state, active, connect, disconnect, send, clear };
}
