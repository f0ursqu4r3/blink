/** One server-sent event. */
export type SseEvent = {
  /** "message" when the stream sets no event name. */
  event: string;
  data: string;
  id?: string;
  /** Milliseconds from the request start, for live events. */
  at?: number;
};

/**
 * An incremental parser for the text/event-stream format (WHATWG HTML,
 * "Server-sent events"). Push text in any pieces; it calls `emit` for each
 * complete event.
 */
export function createSseParser(emit: (event: SseEvent) => void) {
  let buffer = "";
  let data: string[] = [];
  let event = "";
  let id: string | undefined;
  let lastId: string | undefined;
  function line(text: string) {
    if (text === "") {
      if (data.length) {
        emit({
          event: event || "message",
          data: data.join("\n"),
          ...((id ?? lastId) ? { id: id ?? lastId } : {}),
        });
        lastId = id ?? lastId;
      }
      data = [];
      event = "";
      id = undefined;
      return;
    }
    if (text.startsWith(":")) return;
    const colon = text.indexOf(":");
    const field = colon < 0 ? text : text.slice(0, colon);
    let value = colon < 0 ? "" : text.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    if (field === "data") data.push(value);
    else if (field === "event") event = value;
    else if (field === "id" && !value.includes("\0")) id = value;
  }
  return {
    push(text: string) {
      buffer += text;
      // A trailing "\r" may be the first half of "\r\n".
      const parts = buffer.split(/\r\n|\r(?!$)|\n/);
      buffer = parts.pop() ?? "";
      parts.forEach(line);
    },
    /** Dispatch an event left open when the stream ends. */
    end() {
      if (buffer) line(buffer.replace(/\r$/, ""));
      buffer = "";
      line("");
    },
  };
}

/** Parse a whole event-stream body. */
export function parseSse(text: string) {
  const events: SseEvent[] = [];
  const parser = createSseParser((event) => events.push(event));
  parser.push(text);
  parser.end();
  return events;
}

export const isEventStream = (contentType: string | undefined) =>
  Boolean(contentType?.trim().toLowerCase().startsWith("text/event-stream"));
