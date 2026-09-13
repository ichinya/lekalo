// The existing HTTP surface of the task board.
import { createTask } from "./tasks";

export async function handleCreate(request: Request): Promise<Response> {
  const body = await request.json();
  return Response.json(createTask(body.title));
}
