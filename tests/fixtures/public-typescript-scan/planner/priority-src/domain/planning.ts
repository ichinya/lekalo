import type {Task,TaskPriority} from './task.ts';
const rank=(priority: TaskPriority|undefined): number=>priority==='high'?0:priority==='low'?2:1;
export function listForDay(tasks: readonly Task[],workspaceId: string,ownerId: string,day: string): Task[] {
 return tasks.filter(t=>t.workspaceId===workspaceId&&t.ownerId===ownerId&&t.plannedFor===day&&t.state!=='done').sort((a,b)=>rank(a.priority)-rank(b.priority)||a.position-b.position||(a.id<b.id?-1:a.id>b.id?1:0)).map(t=>({...t}));
}
