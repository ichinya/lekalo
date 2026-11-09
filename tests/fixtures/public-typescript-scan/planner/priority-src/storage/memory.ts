import type {Task} from '../domain/task.ts';
export class MemoryStore {
 private rows=new Map<string,Task>();
 private key(t: Pick<Task,'workspaceId'|'id'>): string { return JSON.stringify([t.workspaceId,t.id]); }
 save(task: Task): void { this.rows.set(this.key(task),{...task}); }
 get(workspaceId: string,id: string): Task|undefined { const t=this.rows.get(this.key({workspaceId,id}));return t?{...t}:undefined; }
 all(): Task[] { return [...this.rows.values()].map(t=>({...t})); }
}
