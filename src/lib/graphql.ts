import { locationFromLineColumn, type TextLocation } from "./text-location";

/** Format a GraphQL document. Prettier loads on first use to keep startup lean. */
export async function formatGraphql(text: string): Promise<string> {
  const [{ format }, graphql] = await Promise.all([
    import("prettier/standalone"),
    import("prettier/plugins/graphql"),
  ]);
  const formatted = await format(text, {
    parser: "graphql",
    plugins: [graphql],
    printWidth: 60,
  });
  return formatted.replace(/\n$/, "");
}

/** Prettier's GraphQL errors carry `loc.start` and a "Syntax Error: … (l:c)" message. */
export function graphqlErrorLocation(
  text: string,
  error: unknown,
): TextLocation | null {
  if (!(error instanceof Error)) return null;
  const start = (
    error as { loc?: { start?: { line?: unknown; column?: unknown } } }
  ).loc?.start;
  if (typeof start?.line !== "number" || typeof start.column !== "number")
    return null;
  const reason = error.message
    .split("\n")[0]
    .replace(/^Syntax Error:\s*/, "")
    .replace(/\s*\(\d+:\d+\)\s*$/, "");
  return locationFromLineColumn(text, start.line, start.column, reason);
}
