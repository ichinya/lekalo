// Synthetic stub: the describe/it/test surface used by fixtures.
export function describe(name: string, fn: () => void): void;
export function it(name: string, fn: () => Promise<void> | void): void;
export function test(name: string, fn: () => Promise<void> | void): void;
