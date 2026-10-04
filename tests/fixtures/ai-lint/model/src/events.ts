// Synthetic UTF-8: caf? ??.
import { EventEmitter } from 'node:events';
export class Task { state: string = 'backlog'; }
const bus = new EventEmitter();
bus.on('saved', (task: Task) => { task.state = 'done'; });
export function focus(task: Task, method: string) { bus.emit('saved', task); task[method](); }
export function edit(task: Task) { bus.emit('saved', task); }
export function plain(task: Task) { task.state = 'focused'; }
const text = 'magic-reflection-observer';
const shadow = { emit(value: string) { return value; } }; shadow.emit(text);
