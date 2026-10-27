import type {Task} from '../domain/task.ts';
import {listForDay} from '../domain/planning.ts';
import {toTaskJson} from './task-json.ts';
export function plannerView(tasks: readonly Task[],workspaceId:string,ownerId:string,day:string){return listForDay(tasks,workspaceId,ownerId,day).map(toTaskJson);}
