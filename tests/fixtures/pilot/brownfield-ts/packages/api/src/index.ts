// Brownfield fixture application entry (issue #118): the Hono app
// mounting the submission route. Synthetic; every name is fictional.
import { Hono } from "hono";
import { tasksRouter } from "./routes/tasks.js";

export const app = new Hono();

app.route("/", tasksRouter);
