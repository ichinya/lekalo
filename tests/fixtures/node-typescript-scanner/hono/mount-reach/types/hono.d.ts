// Synthetic minimal declarations of the recognized Hono surface for
// issue #115 fixtures. Authored in-repo; no real package, no conformance
// claim, no runtime. The scanner recognizes the class through this
// declaration file plus the reserved "hono" import specifier.
export interface FetchedResponse {
  status: number;
  text(): Promise<string>;
  json(): Promise<unknown>;
}
export interface Context {
  json(data: unknown, status?: number): FetchedResponse;
  text(text: string, status?: number): FetchedResponse;
  html(html: unknown, status?: number): FetchedResponse;
  body(data: unknown, status?: number): FetchedResponse;
  render(...args: unknown[]): unknown;
  status(status: number): void;
  set(key: string, value: unknown): void;
  get(key: string): unknown;
  readonly var: Record<string, unknown>;
}
export type Handler = (c: Context) => unknown;
export type Middleware = (c: Context, next: () => Promise<void>) => unknown;
export declare class Hono {
  get(path: string, ...handlers: unknown[]): this;
  post(path: string, ...handlers: unknown[]): this;
  put(path: string, ...handlers: unknown[]): this;
  patch(path: string, ...handlers: unknown[]): this;
  delete(path: string, ...handlers: unknown[]): this;
  options(path: string, ...handlers: unknown[]): this;
  head(path: string, ...handlers: unknown[]): this;
  all(path: string, ...handlers: unknown[]): this;
  on(method: string | string[], path: string, ...handlers: unknown[]): this;
  use(pathOrHandler?: unknown, ...rest: unknown[]): this;
  route(path: string, app: Hono): this;
  basePath(path: string): Hono;
  onError(handler: unknown): this;
  notFound(handler: unknown): this;
  request(input: string, init?: { method?: string; body?: unknown; headers?: Record<string, string> }): Promise<FetchedResponse>;
}
