import type {Task} from './task.ts';
export function listForDay(tasks: readonly Task[],workspaceId: string,ownerId: string,day: string): Task[] {
 return tasks.filter(t=>t.workspaceId===workspaceId&&t.ownerId===ownerId&&t.plannedFor===day&&t.state!=='done').sort((a,b)=>a.position-b.position||(a.id<b.id?-1:a.id>b.id?1:0)).map(t=>({...t}));
}
