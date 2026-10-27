import type {Task,NewTask,TaskPatch} from './task.ts';
import {requireText,validPosition} from './validation.ts';
import {DomainError} from './errors.ts';
export function createTask(input: NewTask): Task {
 for(const value of [input.id,input.workspaceId,input.ownerId,input.title])requireText(value);
 const position=input.position??0;validPosition(position);
 return {id:input.id,workspaceId:input.workspaceId,ownerId:input.ownerId,title:input.title,state:'open',plannedFor:input.plannedFor??null,position};
}
export function updateTask(task: Task,patch: TaskPatch): Task {
 if(patch.title!==undefined)requireText(patch.title);
 if(patch.position!==undefined)validPosition(patch.position);
 if(patch.state!==undefined&&!['open','focused','done'].includes(patch.state))throw new DomainError('INVALID_TASK');
 return {...task,...patch};
}
