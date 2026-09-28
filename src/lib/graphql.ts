/** Format a GraphQL document. Prettier loads on first use to keep startup lean. */
export async function formatGraphql(text: string): Promise<string> {
  const [{ format }, graphql] = await Promise.all([
    import("prettier/standalone"),
    import("prettier/plugins/graphql"),
  ]);
  const formatted = await format(text, {
    parser: "graphql",
    plugins: [graphql],
  });
  return formatted.replace(/\n$/, "");
}
