// Minimal framework shim for the screen fixture's typecheck only:
// the typecheck target is the GENERATED CLIENT usage inside the SFC
// script, never the Vue runtime itself (the fixture pins no runtime).
// The compileScript output's component plumbing (`defineComponent`,
// `__props`, `__expose`) is declared loosely here so the strict
// program can check the client calls that matter.
declare module "vue" {
  export interface Ref<T> {
    value: T;
  }
  export function ref<T>(value: T): Ref<T>;
  export function computed<T>(getter: () => T): Ref<T>;
  export function defineComponent(options: unknown): unknown;
  export function defineProps<Props>(): Props;
}
