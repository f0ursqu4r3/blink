import { parse, stringify } from "lossless-json";

export const JSON_HIGHLIGHT_LIMIT = 64_000;

/** Keep large IDs and decimal values exact when inspecting or formatting JSON. */
export function formatJson(text: string): string {
  return stringify(parse(text), null, 2) ?? text;
}
