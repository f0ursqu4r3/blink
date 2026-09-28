export const MIB = 1024 * 1024;
export const DOWNLOAD_LIMIT = 1024 * MIB;

export type TransportOptions = {
  timeoutSeconds: number;
  connectTimeoutSeconds: number;
  followRedirects: boolean;
  maxRedirects: number;
  inspectionLimitMiB: number;
};

export type TransportField = Exclude<keyof TransportOptions, "followRedirects">;

export const defaultTransportOptions = (): TransportOptions => ({
  timeoutSeconds: 30,
  connectTimeoutSeconds: 10,
  followRedirects: false,
  maxRedirects: 10,
  inspectionLimitMiB: 4,
});

export const transportRanges: Record<
  TransportField,
  readonly [number, number]
> = {
  timeoutSeconds: [1, 600],
  connectTimeoutSeconds: [1, 600],
  maxRedirects: [1, 20],
  inspectionLimitMiB: [1, 16],
};

/** One message per invalid field. An empty object means the options are valid. */
export function transportFieldErrors(options: TransportOptions) {
  const errors: Partial<Record<TransportField, string>> = {};
  for (const field of Object.keys(transportRanges) as TransportField[]) {
    const [min, rangeMax] = transportRanges[field];
    // The connect timeout cannot be longer than the total timeout.
    const max =
      field === "connectTimeoutSeconds" &&
      Number.isInteger(options.timeoutSeconds)
        ? Math.min(rangeMax, options.timeoutSeconds)
        : rangeMax;
    const value = options[field];
    if (!Number.isInteger(value) || value < min || value > max)
      errors[field] = `Enter a whole number from ${min} to ${max}.`;
  }
  return errors;
}
