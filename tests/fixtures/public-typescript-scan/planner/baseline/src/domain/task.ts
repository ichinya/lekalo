export type TaskState = 'open' | 'focused' | 'done';
export interface Task { id: string; workspaceId: string; ownerId: string; title: string; state: TaskState; plannedFor: string | null; position: number; }
export interface NewTask { id: string; workspaceId: string; ownerId: string; title: string; plannedFor?: string | null; position?: number; }
export type TaskPatch = Partial<Pick<Task, 'title' | 'state' | 'plannedFor' | 'position'>>;
