import type {Task,NewTask,TaskPatch} from './task.ts';
import {requireText,validPosition,validPriority} from './validation.ts';
import {DomainError} from './errors.ts';
export function createTask(input: NewTask): Task {
 for(const value of [input.id,input.workspaceId,input.ownerId,input.title])requireText(value);
 const position=input.position??0;validPosition(position);
 if(input.priority!==undefined)validPriority(input.priority);
 return {id:input.id,workspaceId:input.workspaceId,ownerId:input.ownerId,title:input.title,state:'open',plannedFor:input.plannedFor??null,position,priority:input.priority??'normal'};
}
export function updateTask(task: Task,patch: TaskPatch): Task {
 if(patch.title!==undefined)requireText(patch.title);
 if(patch.position!==undefined)validPosition(patch.position);
 if(patch.state!==undefined&&!['open','focused','done'].includes(patch.state))throw new DomainError('INVALID_TASK');
 if(patch.priority!==undefined)validPriority(patch.priority);
 const next={...task,...patch};
 if(patch.priority===undefined)next.priority=task.priority;
 return next;
}
