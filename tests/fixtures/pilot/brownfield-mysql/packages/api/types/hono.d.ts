// Authored synthetic scan aid. Runtime tests exclude this file and load real Hono.
export interface Context<E = unknown> { req: {json(): Promise<unknown>}; env: {repository: {create(row: {id:string;title:string;priority:number}): Promise<unknown>}}; json(data:unknown,status?:number): unknown; }
export declare class Hono<E = unknown> { post(path:string,...handlers:unknown[]): this; request(path:string,options?:unknown,env?:unknown): Promise<unknown>; }
