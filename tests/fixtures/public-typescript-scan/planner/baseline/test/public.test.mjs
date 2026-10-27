import {test} from 'node:test';import assert from 'node:assert/strict';import {createTask,updateTask,listForDay,MemoryStore,toTaskJson,fromTaskJson} from '../src/index.ts';
const make=(id,extra={})=>createTask({id,workspaceId:'workspace',ownerId:'alice',title:'Task '+id,plannedFor:'2026-01-01',...extra});
test('creation and immutable update',()=>{const a=make('a');const b=updateTask(a,{title:'Changed'});assert.equal(a.title,'Task a');assert.equal(b.title,'Changed');});
test('planning scope',()=>{const tasks=[make('a'),make('b',{ownerId:'bob'}),make('c',{workspaceId:'other'}),make('d',{plannedFor:'2026-02-01'}),updateTask(make('e'),{state:'done'})];assert.deepEqual(listForDay(tasks,'workspace','alice','2026-01-01').map(t=>t.id),['a']);});
test('store copies and isolation',()=>{const s=new MemoryStore();s.save(make('x'));s.save(make('x',{workspaceId:'other'}));const a=s.get('workspace','x');a.title='Detached';assert.equal(s.get('workspace','x').title,'Task x');assert.equal(s.all().length,2);});
test('JSON existing fields',()=>{const a=updateTask(make('a',{position:2}),{state:'focused'});assert.deepEqual(fromTaskJson(toTaskJson(a)),a);});
