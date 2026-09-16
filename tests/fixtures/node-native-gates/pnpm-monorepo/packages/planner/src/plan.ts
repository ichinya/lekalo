// Synthetic planner source (public-fixture). Never executed as code by
// the tooling; only digested.
export interface PlanTask { id: string; hours: number }
export function planTask(task: PlanTask): string {
  return `plan:${task.id}:${task.hours}`;
}
