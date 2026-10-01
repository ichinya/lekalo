// Synthetic minimal declaration of the recognized Hono surface for the
// issue #118 brownfield fixture. Authored in-repo; no real package, no
// conformance claim, no runtime. The scanner recognizes the class
// through this declaration file plus the reserved "hono" import
// specifier.
export interface FetchedResponse {
  status: number;
  text(): Promise<string>;
  json(): Promise<unknown>;
}
export interface Context {
  req: { valid(): unknown; json(): unknown };
  json(data: unknown, status?: number): FetchedResponse;
  text(text: string, status?: number): FetchedResponse;
  status(status: number): void;
}
export type Handler = (c: Context) => unknown;
export type Middleware = (c: Context, next: () => Promise<void>) => unknown;
export declare class Hono {
  get(path: string, ...handlers: unknown[]): this;
  post(path: string, ...handlers: unknown[]): this;
  put(path: string, ...handlers: unknown[]): this;
  delete(path: string, ...handlers: unknown[]): this;
  patch(path: string, ...handlers: unknown[]): this;
  all(path: string, ...handlers: unknown[]): this;
  use(pathOrHandler?: unknown, ...rest: unknown[]): this;
  route(path: string, app: Hono): this;
  basePath(path: string): Hono;
  onError(handler: unknown): this;
  notFound(handler: unknown): this;
}
