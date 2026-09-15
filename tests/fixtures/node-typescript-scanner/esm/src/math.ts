/** Adds two numbers. */
export function add(a: number, b: number): number {
  return a + b;
}

export function parse(x: string): number;
export function parse(x: number): number;
export function parse(x: string | number): number {
  return Number(x);
}

export interface Shape {
  kind: string;
  area(): number;
}

export type Alias = Shape | null;
export enum Color { Red, Green }
export const answer = 42;
export default add;
