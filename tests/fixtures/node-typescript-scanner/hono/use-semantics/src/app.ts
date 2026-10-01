import { Hono } from "hono";
import { reader } from "./middleware";
import { readerHandler } from "./handlers";

export const app = new Hono();

// Four distinct use() events bind the SAME handler: the composed chain
// legitimately has four members (four ordinals), but each context site
// inside the handler is ONE fact per route — repeated bindings must not
// re-emit byte-identical context records.
app.use(reader);
app.use(reader);
app.use(reader);
app.use(reader);

app.get("/multi", readerHandler);

// A route registered BEFORE a later use() of the same handler: Hono's
// registration order controls entry — the terminal handler never calls
// next(), so the fifth binding can never execute for /first. It must
// not claim chain membership there (matched-but-unreachable).
app.get("/first", readerHandler);
app.use(reader);

// A route registered AFTER the use: the binding composes fully.
app.get("/after", readerHandler);
