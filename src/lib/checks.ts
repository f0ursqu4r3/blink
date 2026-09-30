import type { ApiResponse } from "./request";
import { runJq } from "./jq";

/** What a check reads from the response. `json` runs a jq expression. */
export const checkSources = [
  { id: "status", label: "Status" },
  { id: "time", label: "Time (ms)" },
  { id: "size", label: "Size (bytes)" },
  { id: "header", label: "Header" },
  { id: "json", label: "JSON (jq)" },
  { id: "body", label: "Body text" },
] as const;
export type CheckSource = (typeof checkSources)[number]["id"];

export const checkOperators = [
  { id: "equals", label: "equals" },
  { id: "notEquals", label: "does not equal" },
  { id: "lt", label: "<" },
  { id: "lte", label: "≤" },
  { id: "gt", label: ">" },
  { id: "gte", label: "≥" },
  { id: "contains", label: "contains" },
  { id: "notContains", label: "does not contain" },
  { id: "matches", label: "matches regex" },
  { id: "exists", label: "exists" },
  { id: "notExists", label: "does not exist" },
] as const;
export type CheckOperator = (typeof checkOperators)[number]["id"];
/** Operators that take no expected value. */
export const unaryOperators: CheckOperator[] = ["exists", "notExists"];
/** Sources that take a path: a header name or a jq expression. */
export const pathSources: CheckSource[] = ["header", "json"];

export type Assertion = {
  id: number;
  enabled: boolean;
  source: CheckSource;
  /** Header name or jq expression. */
  path: string;
  operator: CheckOperator;
  expected: string;
};
export type AssertionResult = {
  id: number;
  pass: boolean;
  /** The value read, or an error. */
  actual: string;
  description: string;
};
/** Saves a response value as a workspace token after each successful send. */
export type Capture = {
  id: number;
  enabled: boolean;
  /** Token name, used as {{name}}. */
  name: string;
  source: Exclude<CheckSource, "time" | "size">;
  path: string;
};

export const isCheckSource = (value: unknown): value is CheckSource =>
  checkSources.some((source) => source.id === value);
export const isCheckOperator = (value: unknown): value is CheckOperator =>
  checkOperators.some((operator) => operator.id === value);
export const CAPTURE_NAME_RE = /^[A-Za-z][\w.-]{0,63}$/;

let nextCheckId = 0;
export function reserveCheckId(id: number) {
  nextCheckId = Math.max(nextCheckId, id);
}
export const createAssertion = (
  source: CheckSource = "status",
  operator: CheckOperator = "equals",
  expected = "200",
  path = "",
): Assertion => ({
  id: ++nextCheckId,
  enabled: true,
  source,
  path,
  operator,
  expected,
});
export const createCapture = (
  name = "",
  source: Capture["source"] = "json",
  path = "",
): Capture => ({ id: ++nextCheckId, enabled: true, name, source, path });

/** The value a source reads, as text; undefined when it is absent. */
export async function readSource(
  source: CheckSource,
  path: string,
  response: ApiResponse,
): Promise<string | undefined> {
  if (source === "status") return String(response.status);
  if (source === "time") return String(response.durationMs);
  if (source === "size") return String(response.sizeBytes);
  if (source === "header") {
    const name = path.trim().toLowerCase();
    return response.headers.find((header) => header.key.toLowerCase() === name)
      ?.value;
  }
  if (response.truncated || response.binary)
    throw new Error("The body is truncated or binary.");
  if (source === "body") return response.body;
  // -r prints strings without quotes; -c prints other values on one line.
  const output = await runJq(response.body, path.trim() || ".", ["-c", "-r"]);
  return output === "null" || output === "" ? undefined : output;
}

const numeric = (value: string) =>
  value.trim() !== "" && Number.isFinite(Number(value));
/** Parse JSON text so `{"a": 1}` equals `{"a":1}`; plain text stays text. */
function canonical(value: string) {
  try {
    return JSON.stringify(JSON.parse(value));
  } catch {
    return value;
  }
}

export function compare(
  actual: string | undefined,
  operator: CheckOperator,
  expected: string,
): boolean {
  if (operator === "exists") return actual !== undefined;
  if (operator === "notExists") return actual === undefined;
  if (actual === undefined)
    return operator === "notEquals" || operator === "notContains";
  if (["lt", "lte", "gt", "gte"].includes(operator)) {
    if (!numeric(actual) || !numeric(expected)) return false;
    const a = Number(actual);
    const b = Number(expected);
    return operator === "lt"
      ? a < b
      : operator === "lte"
        ? a <= b
        : operator === "gt"
          ? a > b
          : a >= b;
  }
  if (operator === "equals" || operator === "notEquals") {
    const equal =
      actual === expected ||
      (numeric(actual) &&
        numeric(expected) &&
        Number(actual) === Number(expected)) ||
      canonical(actual) === canonical(expected);
    return operator === "equals" ? equal : !equal;
  }
  if (operator === "contains") return actual.includes(expected);
  if (operator === "notContains") return !actual.includes(expected);
  return new RegExp(expected).test(actual);
}

export function describeAssertion(assertion: Assertion) {
  const source =
    checkSources.find((item) => item.id === assertion.source)?.label ?? "";
  const operator =
    checkOperators.find((item) => item.id === assertion.operator)?.label ?? "";
  const path = pathSources.includes(assertion.source)
    ? ` ${assertion.path.trim() || (assertion.source === "json" ? "." : "")}`
    : "";
  const expected = unaryOperators.includes(assertion.operator)
    ? ""
    : ` ${assertion.expected}`;
  return `${source}${path} ${operator}${expected}`;
}

const MAX_SHOWN = 200;
const shorten = (value: string) =>
  value.length > MAX_SHOWN ? `${value.slice(0, MAX_SHOWN)}…` : value;

export async function runAssertions(
  assertions: Assertion[] | undefined,
  response: ApiResponse,
): Promise<AssertionResult[]> {
  const results: AssertionResult[] = [];
  for (const assertion of assertions ?? []) {
    if (!assertion.enabled) continue;
    const description = describeAssertion(assertion);
    try {
      const actual = await readSource(
        assertion.source,
        assertion.path,
        response,
      );
      results.push({
        id: assertion.id,
        description,
        pass: compare(actual, assertion.operator, assertion.expected),
        actual: actual === undefined ? "(absent)" : shorten(actual),
      });
    } catch (error) {
      results.push({
        id: assertion.id,
        description,
        pass: false,
        actual: error instanceof Error ? error.message : String(error),
      });
    }
  }
  return results;
}

/** Token values from the enabled captures that found a value. */
export async function runCaptures(
  captures: Capture[] | undefined,
  response: ApiResponse,
) {
  const values: Record<string, string> = {};
  const errors: string[] = [];
  for (const capture of captures ?? []) {
    if (!capture.enabled || !CAPTURE_NAME_RE.test(capture.name)) continue;
    try {
      const value = await readSource(capture.source, capture.path, response);
      if (value === undefined) errors.push(`${capture.name}: no value`);
      else values[capture.name] = value;
    } catch (error) {
      errors.push(
        `${capture.name}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
  return { values, errors };
}
