import type {Task} from '../domain/task.ts';
import {createTask,updateTask} from '../domain/task-service.ts';
import {DomainError} from '../domain/errors.ts';
export function toTaskJson(task: Task): Record<string,unknown> { return {id:task.id,workspaceId:task.workspaceId,ownerId:task.ownerId,title:task.title,state:task.state,plannedFor:task.plannedFor,position:task.position}; }
export function fromTaskJson(value: unknown): Task {
 if(!value||typeof value!=='object'||Array.isArray(value))throw new DomainError('INVALID_TASK');
 const v=value as Record<string,unknown>;
 const task=createTask({id:v.id as string,workspaceId:v.workspaceId as string,ownerId:v.ownerId as string,title:v.title as string,plannedFor:v.plannedFor as string|null,position:v.position as number});
 return updateTask(task,{state:(v.state??'open') as Task['state']});
}
