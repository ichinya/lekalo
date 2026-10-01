// Synthetic stub: the recognized @hono/zod-openapi surface.
import type { Context, Hono } from "hono";
export interface RouteConfig {
  method: string;
  path: string;
  operationId?: string;
  request?: unknown;
  responses?: unknown;
}
export declare function createRoute(config: RouteConfig): RouteConfig;
export declare class OpenAPIHono extends Hono {
  openapi(config: RouteConfig, handler: (c: Context) => unknown): this;
}
