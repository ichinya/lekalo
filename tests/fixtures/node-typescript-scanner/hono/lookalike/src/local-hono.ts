// A local class named Hono: no "hono" import specifier, so it must
// never be recognized as a framework instance.
export class Hono {
  get(_path: string, _handler: unknown): void {}
}
