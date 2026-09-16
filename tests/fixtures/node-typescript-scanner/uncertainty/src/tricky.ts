import { missing } from "does-not-exist";

export const anyValue: any = 1;
export const unknownValue: unknown = anyValue;
export const dynamic: Record<string, unknown> = { ["computed" + "Key"]: 1 };
export async function maybe(): Promise<any> {
  return anyValue;
}

export function callUnknown(): unknown {
  const fn = globalThis as unknown as { ghost?: () => void };
  fn.ghost?.();
  return missing;
}

export function callMissing(): unknown {
  // `missing` resolves to nothing: the call target is unknown.
  return (missing as () => number)();
}

export function deepContainer(): { nested: { value: any } } {
  return { nested: { value: unknownValue } };
}
