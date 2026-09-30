// Declared service layer: the handler→service join target.
export interface User {
  id: number;
  name: string;
}

export function queryUsers(): User[] {
  return [{ id: 1, name: "ada" }];
}

export function findUser(id: number): User | null {
  return { id, name: "ada" };
}

export function saveUser(user: User): User {
  return user;
}
