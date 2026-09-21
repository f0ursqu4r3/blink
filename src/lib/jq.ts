export async function runJq(input: string, query: string) {
  const { raw } = await import("jq-wasm/inline");
  const result = await raw(input, query);
  if (result.exitCode !== 0)
    throw new Error(result.stderr.trim() || "jq did not return a result.");
  return result.stdout.trimEnd();
}
