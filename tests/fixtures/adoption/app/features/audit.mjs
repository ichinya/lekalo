// Synthetic new contracted feature of the issue #136 hybrid fixture.
export function auditEvent(task) {
  return { id: task.id, state: task.state, kind: "task.audited" };
}
