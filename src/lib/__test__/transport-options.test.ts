import { describe, expect, it } from "vitest";
import {
  defaultTransportOptions,
  transportFieldErrors,
} from "../transport-options";

describe("transport options", () => {
  it("accepts the defaults", () => {
    expect(defaultTransportOptions()).toEqual({
      timeoutSeconds: 30,
      connectTimeoutSeconds: 10,
      followRedirects: false,
      maxRedirects: 10,
      inspectionLimitMiB: 4,
    });
    expect(transportFieldErrors(defaultTransportOptions())).toEqual({});
  });
  it("rejects values out of range, decimals and NaN", () => {
    const errors = transportFieldErrors({
      timeoutSeconds: 601,
      connectTimeoutSeconds: 1.5,
      followRedirects: true,
      maxRedirects: 0,
      inspectionLimitMiB: Number.NaN,
    });
    expect(errors).toEqual({
      timeoutSeconds: "Enter a whole number from 1 to 600.",
      connectTimeoutSeconds: "Enter a whole number from 1 to 600.",
      maxRedirects: "Enter a whole number from 1 to 20.",
      inspectionLimitMiB: "Enter a whole number from 1 to 16.",
    });
  });
  it("limits the connect timeout to the total timeout", () => {
    expect(
      transportFieldErrors({
        ...defaultTransportOptions(),
        timeoutSeconds: 5,
        connectTimeoutSeconds: 6,
      }),
    ).toEqual({ connectTimeoutSeconds: "Enter a whole number from 1 to 5." });
  });
});
