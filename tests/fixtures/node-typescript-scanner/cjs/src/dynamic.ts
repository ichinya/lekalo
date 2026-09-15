export const registry: Record<string, number> = {};
export function register(name: string): void {
  registry[name] = name.length;
}
// dynamic export assignment is NOT verified by the checker:
const hidden = { sneaky: true };
export { hidden };
