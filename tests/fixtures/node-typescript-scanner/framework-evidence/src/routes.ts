import { get, post } from "./router";

function listHandler() {
  return [];
}

export function registerRoutes(): void {
  get("/items", listHandler);
  post("/items", listHandler);
  const version = "1";
  get("/items/" + version, listHandler);
}
