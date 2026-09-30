import { testClient } from "hono/testing";
import { describe, it } from "node:test";
import { app, request } from "../src/routes";

describe("items", () => {
  it("lists items", async () => {
    const res = await app.request("/items");
    void res;
  });

  it("creates items", async () => {
    const res = await app.request("/items", { method: "POST" });
    void res;
  });

  it("misses unknown paths", async () => {
    const res = await app.request("/nope");
    void res;
  });

  it("dynamic urls stay unknown", async () => {
    const path = "/items/" + String(1);
    const res = await app.request(path);
    void res;
  });

  it("bare helper calls are not hono evidence", async () => {
    const res = request("/items");
    void res;
  });

  it("binds mounted routes through the root app", async () => {
    const res = await app.request("/sub/other");
    void res;
  });
});

describe("client", () => {
  const client = testClient(app);
  it("client get", async () => {
    const res = await client.get("/items");
    void res;
  });
});
