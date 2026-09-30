// Synthetic stub: the recognized zod surface used by fixtures.
export interface ZodSchema {
  readonly _def: unknown;
}
export declare const z: {
  object(shape: Record<string, unknown>): ZodSchema;
  string(): unknown;
  number(): unknown;
  literal(value: unknown): unknown;
};
