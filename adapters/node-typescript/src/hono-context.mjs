/**
 * Shared handler-body helpers of the Hono provider (issue #115).
 *
 * Small, closed utilities used by the middleware and HTTP phases:
 * resolving a handler's declared body, identifying the handler's
 * context parameter, and matching receiver expressions against it.
 */

/**
 * The import declaration binding this symbol at its use site, or null:
 * ImportSpecifier → NamedImports/ImportClause → ImportDeclaration.
 */
export function importDeclarationOfSymbol(ts, symbol) {
  for (const declaration of symbol.declarations ?? []) {
    if (declaration.kind !== ts.SyntaxKind.ImportSpecifier) continue;
    let ancestor = declaration.parent;
    for (let depth = 0; depth < 4 && ancestor; depth += 1) {
      if (ancestor.kind === ts.SyntaxKind.ImportDeclaration) {
        if (ancestor.moduleSpecifier?.kind === ts.SyntaxKind.StringLiteral) {
          return ancestor;
        }
        return null;
      }
      ancestor = ancestor.parent;
    }
  }
  return null;
}

/** The declared function body of a handler endpoint, if statically present. */
export function functionBodyOf(ctx, handler) {
  const { ts } = ctx;
  const node = handler.node;
  if (!node) return null;
  if (node.kind === ts.SyntaxKind.ArrowFunction || node.kind === ts.SyntaxKind.FunctionExpression) {
    return node.body ?? null;
  }
  const declaration = handler.symbol?.declarations?.find((candidate) => candidate.body);
  return declaration?.body ?? null;
}

/**
 * The symbol of a handler's first parameter — the Hono context `c` —
 * resolved through the checker for inline and referenced handlers.
 */
export function contextParamSymbolOf(ctx, handler) {
  const { ts, checker } = ctx;
  let declaration = null;
  if (handler.indexed && handler.symbol) {
    declaration = handler.symbol.declarations?.find((candidate) => candidate.body
      && (candidate.kind === ts.SyntaxKind.FunctionDeclaration
        || candidate.kind === ts.SyntaxKind.MethodDeclaration
        || candidate.kind === ts.SyntaxKind.ArrowFunction
        || candidate.kind === ts.SyntaxKind.FunctionExpression)) ?? null;
  }
  if (!declaration && handler.node
    && (handler.node.kind === ts.SyntaxKind.ArrowFunction || handler.node.kind === ts.SyntaxKind.FunctionExpression)) {
    declaration = handler.node;
  }
  const first = declaration?.parameters?.[0]?.name;
  if (!first || first.kind !== ts.SyntaxKind.Identifier) return null;
  return checker.getSymbolAtLocation(first) ?? null;
}

/** Does this receiver expression denote the context parameter? */
export function resolvesToContextSymbol(ctx, node, contextSymbol) {
  if (!node || node.kind !== ctx.ts.SyntaxKind.Identifier) return false;
  const resolved = ctx.checker.getSymbolAtLocation(node);
  return resolved === contextSymbol;
}

/**
 * The import specifier binding this expression at its use site (through
 * alias chains), or null when the identifier is not imported.
 */
export function importSpecifierTextAt(ctx, expression) {
  const { ts, checker } = ctx;
  const target = expression.kind === ts.SyntaxKind.PropertyAccessExpression ? expression.name : expression;
  const useSite = checker.getSymbolAtLocation(target);
  if (!useSite) return null;
  // The use-site binding is authoritative: its import declaration names
  // the specifier the caller actually wrote.
  const useSiteImport = importDeclarationOfSymbol(ts, useSite);
  if (useSiteImport !== null) return useSiteImport.moduleSpecifier.text;
  let symbol = useSite;
  for (let depth = 0; depth < 8; depth += 1) {
    if (symbol.flags & ts.SymbolFlags.Alias) {
      try {
        symbol = checker.getAliasedSymbol(symbol);
      } catch {
        break;
      }
    } else {
      break;
    }
  }
  const importDeclaration = importDeclarationOfSymbol(ts, symbol);
  return importDeclaration === null ? null : importDeclaration.moduleSpecifier.text;
}
