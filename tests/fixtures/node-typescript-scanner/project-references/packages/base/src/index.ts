export interface Base {
  id: string;
}
export function makeBase(id: string): Base {
  return { id };
}
