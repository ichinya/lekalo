export class DomainError extends Error { readonly code: string; constructor(code: string) { super(code); this.code=code; this.name='DomainError'; } }
