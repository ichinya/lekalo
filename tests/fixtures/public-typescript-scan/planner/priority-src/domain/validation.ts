import {DomainError} from './errors.ts';
import type {TaskPriority} from './task.ts';
export function requireText(value: unknown): asserts value is string { if(typeof value!=='string'||value.trim().length===0) throw new DomainError('INVALID_TASK'); }
export function validPosition(value: number): void { if(!Number.isInteger(value)||value<0)throw new DomainError('INVALID_TASK'); }
export function validPriority(value: unknown): asserts value is TaskPriority { if(value!=='low'&&value!=='normal'&&value!=='high')throw new DomainError('INVALID_PRIORITY'); }
