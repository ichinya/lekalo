// Synthetic stub: the recognized hono/testing surface.
import type { Hono } from "hono";
export interface TestClient {
  request(path: string, init?: { method?: string }): Promise<unknown>;
  get(path: string): Promise<unknown>;
  post(path: string): Promise<unknown>;
  put(path: string): Promise<unknown>;
  patch(path: string): Promise<unknown>;
  delete(path: string): Promise<unknown>;
}
export declare function testClient(app: Hono): TestClient;
