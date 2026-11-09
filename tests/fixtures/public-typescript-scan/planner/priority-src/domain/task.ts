export type TaskState = 'open' | 'focused' | 'done';
export type TaskPriority = 'low' | 'normal' | 'high';
export interface Task { id: string; workspaceId: string; ownerId: string; title: string; state: TaskState; plannedFor: string | null; position: number; priority: TaskPriority; }
export interface NewTask { id: string; workspaceId: string; ownerId: string; title: string; plannedFor?: string | null; position?: number; priority?: TaskPriority; }
export type TaskPatch = Partial<Pick<Task, 'title' | 'state' | 'plannedFor' | 'position' | 'priority'>>;
