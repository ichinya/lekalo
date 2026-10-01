// An unrelated import that happens to share the conventional `next`
// name: pass-through detection must resolve symbols, not names.
export async function next(): Promise<void> {}
