// Brownfield fixture data barrel (issue #118): the storage surface the
// API consumes. Synthetic; every name is fictional.
export { tasks, type NewTaskRow, type TaskRow } from "./schema.js";
export { db, findTaskById, insertTask } from "./repository.js";
