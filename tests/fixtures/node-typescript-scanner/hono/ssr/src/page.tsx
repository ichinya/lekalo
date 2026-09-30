import { Hono } from "hono";

function Layout(props: { children?: unknown }) {
  return <html><body>{props.children}</body></html>;
}

export const pages = new Hono();

// SSR: JSX returned through the JSX runtime.
pages.get("/page", (c) => c.html(<Layout><h1>hello</h1></Layout>));

// SSR: renderer-backed render call.
pages.get("/render", (c) => c.render("hello"));
