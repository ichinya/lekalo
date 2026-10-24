import {expect,test} from 'vitest';
import {add} from '../src/index.ts';
test('adds values',()=>expect(add(2,3)).toBe(5));
test('host tokens are absent',()=>{expect(process.env.LEKALO_NATIVE_SECRET).toBeUndefined();expect(process.env.NPM_TOKEN).toBeUndefined();});

