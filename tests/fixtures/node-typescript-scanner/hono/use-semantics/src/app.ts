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
