// Synthetic API source (public-fixture).
import { planTask } from "@fixture/planner";
export function handle(task: { id: string; hours: number }): string {
  return planTask(task);
}
