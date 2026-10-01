// Synthetic minimal declaration of the recognized vitest surface for
// the issue #118 brownfield fixture. Authored in-repo; no real package,
// no conformance claim, no runtime.
export interface TestFunction {
  (name: string, fn: () => Promise<void> | void): void;
}
export declare const describe: TestFunction;
export declare const it: TestFunction;
export declare function expect(actual: unknown): {
  toBe(expected: unknown): void;
  toEqual(expected: unknown): void;
};
