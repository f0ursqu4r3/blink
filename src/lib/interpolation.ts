/**
 * Token interpolation for Blink request values.
 *
 * Syntax
 *   {{name}}   — resolves from local definitions, falls back to workspace
 *   {{_.name}} — resolves from workspace-global definitions only
 *
 * Token values can reference other tokens. Resolution detects cycles and never
 * includes values in errors.
 */

export type InterpolationContext = {
  /** Local definitions (group or request level). */
  definitions: Record<string, string>;
  /** Workspace-global definitions. */
  workspaceDefinitions?: Record<string, string>;
};

const TOKEN_RE = /\{\{(_\.)?([^{}]+?)\}\}/g;

/**
 * Resolve all `{{name}}` / `{{_.name}}` tokens in `template`.
 *
 * @throws {Error} naming the first unresolvable reference (never reveals values)
 */
export function interpolate(
  template: string,
  ctx: InterpolationContext,
): string {
  const { definitions, workspaceDefinitions = {} } = ctx;
  const resolve = (source: string, stack: string[]): string => {
    TOKEN_RE.lastIndex = 0;
    return source.replace(TOKEN_RE, (_token, globalPrefix, name: string) => {
      const workspaceOnly = globalPrefix === "_.";
      const key = `${workspaceOnly ? "_." : ""}${name}`;
      if (stack.includes(key)) {
        throw new Error(`Circular token reference: "${name}".`);
      }

      const value = workspaceOnly
        ? workspaceDefinitions[name]
        : Object.prototype.hasOwnProperty.call(definitions, name)
          ? definitions[name]
          : workspaceDefinitions[name];
      if (value === undefined) {
        throw new Error(
          `Undefined token reference: "${name}". Define it in your ${workspaceOnly ? "workspace" : "group or workspace"} variables.`,
        );
      }
      return resolve(value, [...stack, key]);
    });
  };

  return resolve(template, []);
}
