import {DomainError} from './errors.ts';
export function requireText(value: unknown): asserts value is string { if(typeof value!=='string'||value.trim().length===0) throw new DomainError('INVALID_TASK'); }
export function validPosition(value: number): void { if(!Number.isInteger(value)||value<0)throw new DomainError('INVALID_TASK'); }
