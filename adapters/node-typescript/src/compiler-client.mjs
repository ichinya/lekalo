// Generated: TypeScript 7.0.2 API/AST, Microsoft Corporation, Apache-2.0.
// See THIRD_PARTY_NOTICES.md. Transport adaptation revision 1; private IPC frame bound 67108864.
var __defProp = Object.defineProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// node_modules/typescript/dist/ast/index.js
var ast_exports = {};
__export(ast_exports, {
  CharacterCodes: () => CharacterCodes,
  CommentDirectiveType: () => CommentDirectiveType,
  InternalSymbolName: () => InternalSymbolName,
  LanguageVariant: () => LanguageVariant,
  ModifierFlags: () => ModifierFlags,
  NodeFlags: () => NodeFlags,
  RegularExpressionFlags: () => RegularExpressionFlags,
  ScriptKind: () => ScriptKind,
  ScriptTarget: () => ScriptTarget,
  SyntaxKind: () => SyntaxKind,
  TokenFlags: () => TokenFlags,
  cast: () => cast,
  characterCodeToRegularExpressionFlag: () => characterCodeToRegularExpressionFlag,
  cloneSourceFileData: () => cloneSourceFileData,
  computeLineStarts: () => computeLineStarts,
  couldStartTrivia: () => couldStartTrivia,
  createScanner: () => createScanner,
  escapeLeadingUnderscores: () => escapeLeadingUnderscores,
  findNextToken: () => findNextToken,
  findPrecedingToken: () => findPrecedingToken,
  forEachChildOfJSDocParameterTag: () => forEachChildOfJSDocParameterOrPropertyTag,
  forEachChildOfJSDocPropertyTag: () => forEachChildOfJSDocParameterOrPropertyTag,
  forEachLeadingCommentRange: () => forEachLeadingCommentRange,
  forEachTrailingCommentRange: () => forEachTrailingCommentRange,
  formatSyntaxKind: () => formatSyntaxKind,
  getAllJSDocTags: () => getAllJSDocTags,
  getAllJSDocTagsOfKind: () => getAllJSDocTagsOfKind,
  getJSDocTags: () => getJSDocTags,
  getLeadingCommentRanges: () => getLeadingCommentRanges,
  getShebang: () => getShebang,
  getSynthesizedDeepClone: () => getSynthesizedDeepClone,
  getSynthesizedDeepClones: () => getSynthesizedDeepClones,
  getTextOfJSDocComment: () => getTextOfJSDocComment,
  getTokenAtPosition: () => getTokenAtPosition,
  getTokenPosOfNode: () => getTokenPosOfNode,
  getTouchingPropertyName: () => getTouchingPropertyName,
  getTouchingToken: () => getTouchingToken,
  getTrailingCommentRanges: () => getTrailingCommentRanges,
  isAbstractKeyword: () => isAbstractKeyword,
  isAccessExpression: () => isAccessExpression,
  isAccessorDeclaration: () => isAccessorDeclaration,
  isAccessorKeyword: () => isAccessorKeyword,
  isAdditiveOperator: () => isAdditiveOperator,
  isAdditiveOperatorOrHigher: () => isAdditiveOperatorOrHigher,
  isAnyImportSyntax: () => isAnyImportSyntax,
  isArrayBindingElement: () => isArrayBindingElement,
  isArrayBindingPattern: () => isArrayBindingPattern,
  isArrayDestructuringAssignment: () => isArrayDestructuringAssignment,
  isArrayLiteralExpression: () => isArrayLiteralExpression,
  isArrayTypeNode: () => isArrayTypeNode,
  isArrowFunction: () => isArrowFunction,
  isAsExpression: () => isAsExpression,
  isAssertKeyword: () => isAssertKeyword,
  isAssertionExpression: () => isAssertionExpression,
  isAssertsKeyword: () => isAssertsKeyword,
  isAssignmentOperator: () => isAssignmentOperator,
  isAssignmentOperatorOrHigher: () => isAssignmentOperatorOrHigher,
  isAssignmentOperatorToken: () => isAssignmentOperatorToken,
  isAsteriskToken: () => isAsteriskToken,
  isAsyncKeyword: () => isAsyncKeyword,
  isAwaitExpression: () => isAwaitExpression,
  isAwaitKeyword: () => isAwaitKeyword,
  isBigIntLiteral: () => isBigIntLiteral,
  isBinaryExpression: () => isBinaryExpression,
  isBinaryOperator: () => isBinaryOperator,
  isBinaryOperatorToken: () => isBinaryOperatorToken,
  isBindingElement: () => isBindingElement,
  isBindingName: () => isBindingName,
  isBindingPattern: () => isBindingPattern,
  isBitwiseOperator: () => isBitwiseOperator,
  isBitwiseOperatorOrHigher: () => isBitwiseOperatorOrHigher,
  isBlock: () => isBlock,
  isBlockOrExpression: () => isBlockOrExpression,
  isBooleanLiteral: () => isBooleanLiteral,
  isBreakOrContinueStatement: () => isBreakOrContinueStatement,
  isBreakStatement: () => isBreakStatement,
  isCallExpression: () => isCallExpression,
  isCallLikeExpression: () => isCallLikeExpression,
  isCallOrNewExpression: () => isCallOrNewExpression,
  isCallSignatureDeclaration: () => isCallSignatureDeclaration,
  isCaseBlock: () => isCaseBlock,
  isCaseClause: () => isCaseClause,
  isCaseKeyword: () => isCaseKeyword,
  isCatchClause: () => isCatchClause,
  isClassDeclaration: () => isClassDeclaration,
  isClassExpression: () => isClassExpression,
  isClassLikeDeclaration: () => isClassLikeDeclaration,
  isClassStaticBlockDeclaration: () => isClassStaticBlockDeclaration,
  isColonToken: () => isColonToken,
  isCompoundAssignmentOperator: () => isCompoundAssignmentOperator,
  isComputedPropertyName: () => isComputedPropertyName,
  isConciseBody: () => isConciseBody,
  isConditionalExpression: () => isConditionalExpression,
  isConditionalTypeNode: () => isConditionalTypeNode,
  isConstKeyword: () => isConstKeyword,
  isConstructSignatureDeclaration: () => isConstructSignatureDeclaration,
  isConstructorDeclaration: () => isConstructorDeclaration,
  isConstructorTypeNode: () => isConstructorTypeNode,
  isContinueStatement: () => isContinueStatement,
  isDebuggerStatement: () => isDebuggerStatement,
  isDeclarationName: () => isDeclarationName,
  isDeclareKeyword: () => isDeclareKeyword,
  isDecorator: () => isDecorator,
  isDefaultClause: () => isDefaultClause,
  isDefaultKeyword: () => isDefaultKeyword,
  isDeleteExpression: () => isDeleteExpression,
  isDestructuringAssignment: () => isDestructuringAssignment,
  isDoStatement: () => isDoStatement,
  isDotDotDotToken: () => isDotDotDotToken,
  isDotToken: () => isDotToken,
  isElementAccessExpression: () => isElementAccessExpression,
  isEmptyStatement: () => isEmptyStatement,
  isEndOfFile: () => isEndOfFile,
  isEntityName: () => isEntityName,
  isEnumDeclaration: () => isEnumDeclaration,
  isEnumMember: () => isEnumMember,
  isEqualityOperator: () => isEqualityOperator,
  isEqualityOperatorOrHigher: () => isEqualityOperatorOrHigher,
  isEqualsGreaterThanToken: () => isEqualsGreaterThanToken,
  isEqualsToken: () => isEqualsToken,
  isExclamationToken: () => isExclamationToken,
  isExponentiationOperator: () => isExponentiationOperator,
  isExportAssignment: () => isExportAssignment,
  isExportDeclaration: () => isExportDeclaration,
  isExportKeyword: () => isExportKeyword,
  isExportSpecifier: () => isExportSpecifier,
  isExpression: () => isExpression,
  isExpressionStatement: () => isExpressionStatement,
  isExpressionWithTypeArguments: () => isExpressionWithTypeArguments,
  isExternalModuleReference: () => isExternalModuleReference,
  isFalseLiteral: () => isFalseLiteral,
  isForInStatement: () => isForInStatement,
  isForInitializer: () => isForInitializer,
  isForOfStatement: () => isForOfStatement,
  isForStatement: () => isForStatement,
  isFunctionBody: () => isFunctionBody,
  isFunctionDeclaration: () => isFunctionDeclaration,
  isFunctionExpression: () => isFunctionExpression,
  isFunctionLikeDeclaration: () => isFunctionLikeDeclaration,
  isFunctionTypeNode: () => isFunctionTypeNode,
  isGetAccessorDeclaration: () => isGetAccessorDeclaration,
  isHeritageClause: () => isHeritageClause,
  isIdentifier: () => isIdentifier,
  isIdentifierOrJSDocNamespaceDeclaration: () => isIdentifierOrJSDocNamespaceDeclaration,
  isIdentifierOrThisTypeNode: () => isIdentifierOrThisTypeNode,
  isIdentifierPart: () => isIdentifierPart,
  isIdentifierStart: () => isIdentifierStart,
  isIdentifierText: () => isIdentifierText,
  isIfStatement: () => isIfStatement,
  isImportAttribute: () => isImportAttribute,
  isImportAttributeName: () => isImportAttributeName,
  isImportAttributes: () => isImportAttributes,
  isImportClause: () => isImportClause,
  isImportClauseOrBindingPattern: () => isImportClauseOrBindingPattern,
  isImportDeclaration: () => isImportDeclaration,
  isImportEqualsDeclaration: () => isImportEqualsDeclaration,
  isImportExpression: () => isImportExpression,
  isImportPhaseModifierKind: () => isImportPhaseModifierKind,
  isImportSpecifier: () => isImportSpecifier,
  isImportTypeNode: () => isImportTypeNode,
  isInKeyword: () => isInKeyword,
  isIndexSignatureDeclaration: () => isIndexSignatureDeclaration,
  isIndexedAccessTypeNode: () => isIndexedAccessTypeNode,
  isInferTypeNode: () => isInferTypeNode,
  isInterfaceDeclaration: () => isInterfaceDeclaration,
  isIntersectionTypeNode: () => isIntersectionTypeNode,
  isJSDoc: () => isJSDoc,
  isJSDocAllType: () => isJSDocAllType,
  isJSDocAugmentsTag: () => isJSDocAugmentsTag,
  isJSDocCallbackTag: () => isJSDocCallbackTag,
  isJSDocComment: () => isJSDocComment,
  isJSDocDeprecatedTag: () => isJSDocDeprecatedTag,
  isJSDocFullName: () => isJSDocFullName,
  isJSDocImplementsTag: () => isJSDocImplementsTag,
  isJSDocImportTag: () => isJSDocImportTag,
  isJSDocLink: () => isJSDocLink,
  isJSDocLinkCode: () => isJSDocLinkCode,
  isJSDocLinkPlain: () => isJSDocLinkPlain,
  isJSDocNameReference: () => isJSDocNameReference,
  isJSDocNodeKind: () => isJSDocNodeKind,
  isJSDocNonNullableType: () => isJSDocNonNullableType,
  isJSDocNullableType: () => isJSDocNullableType,
  isJSDocOptionalType: () => isJSDocOptionalType,
  isJSDocOverloadTag: () => isJSDocOverloadTag,
  isJSDocOverrideTag: () => isJSDocOverrideTag,
  isJSDocParameterTag: () => isJSDocParameterTag,
  isJSDocPrivateTag: () => isJSDocPrivateTag,
  isJSDocPropertyTag: () => isJSDocPropertyTag,
  isJSDocProtectedTag: () => isJSDocProtectedTag,
  isJSDocPublicTag: () => isJSDocPublicTag,
  isJSDocReadonlyTag: () => isJSDocReadonlyTag,
  isJSDocReturnTag: () => isJSDocReturnTag,
  isJSDocSatisfiesTag: () => isJSDocSatisfiesTag,
  isJSDocSeeTag: () => isJSDocSeeTag,
  isJSDocSignature: () => isJSDocSignature,
  isJSDocTemplateTag: () => isJSDocTemplateTag,
  isJSDocText: () => isJSDocText,
  isJSDocThisTag: () => isJSDocThisTag,
  isJSDocThrowsTag: () => isJSDocThrowsTag,
  isJSDocTypeExpression: () => isJSDocTypeExpression,
  isJSDocTypeExpressionOrJSDocTypeLiteral: () => isJSDocTypeExpressionOrJSDocTypeLiteral,
  isJSDocTypeLiteral: () => isJSDocTypeLiteral,
  isJSDocTypeTag: () => isJSDocTypeTag,
  isJSDocTypedefTag: () => isJSDocTypedefTag,
  isJSDocUnknownTag: () => isJSDocUnknownTag,
  isJSDocVariadicType: () => isJSDocVariadicType,
  isJsxAttribute: () => isJsxAttribute,
  isJsxAttributeLike: () => isJsxAttributeLike,
  isJsxAttributeName: () => isJsxAttributeName,
  isJsxAttributeValue: () => isJsxAttributeValue,
  isJsxAttributes: () => isJsxAttributes,
  isJsxChild: () => isJsxChild,
  isJsxClosingElement: () => isJsxClosingElement,
  isJsxClosingFragment: () => isJsxClosingFragment,
  isJsxElement: () => isJsxElement,
  isJsxExpression: () => isJsxExpression,
  isJsxFragment: () => isJsxFragment,
  isJsxNamespacedName: () => isJsxNamespacedName,
  isJsxOpeningElement: () => isJsxOpeningElement,
  isJsxOpeningFragment: () => isJsxOpeningFragment,
  isJsxOpeningLikeElement: () => isJsxOpeningLikeElement,
  isJsxSelfClosingElement: () => isJsxSelfClosingElement,
  isJsxSpreadAttribute: () => isJsxSpreadAttribute,
  isJsxTagNameExpression: () => isJsxTagNameExpression,
  isJsxText: () => isJsxText,
  isJsxTokenKind: () => isJsxTokenKind,
  isKeywordExpression: () => isKeywordExpression,
  isKeywordExpressionKind: () => isKeywordExpressionKind,
  isKeywordKind: () => isKeywordKind,
  isKeywordTypeKind: () => isKeywordTypeKind,
  isKeywordTypeNode: () => isKeywordTypeNode,
  isLabeledStatement: () => isLabeledStatement,
  isLeftHandSideExpression: () => isLeftHandSideExpression,
  isLineBreak: () => isLineBreak,
  isLiteralExpression: () => isLiteralExpression,
  isLiteralKind: () => isLiteralKind,
  isLiteralLikeNode: () => isLiteralLikeNode,
  isLiteralToken: () => isLiteralToken,
  isLiteralTypeLiteral: () => isLiteralTypeLiteral,
  isLiteralTypeNode: () => isLiteralTypeNode,
  isLogicalOperator: () => isLogicalOperator,
  isLogicalOperatorOrHigher: () => isLogicalOperatorOrHigher,
  isLogicalOrCoalescingAssignmentOperator: () => isLogicalOrCoalescingAssignmentOperator,
  isMappedTypeNode: () => isMappedTypeNode,
  isMemberName: () => isMemberName,
  isMetaProperty: () => isMetaProperty,
  isMethodDeclaration: () => isMethodDeclaration,
  isMethodSignatureDeclaration: () => isMethodSignatureDeclaration,
  isMinusToken: () => isMinusToken,
  isMissingDeclaration: () => isMissingDeclaration,
  isModifier: () => isModifier,
  isModifierKind: () => isModifierKind,
  isModifierLike: () => isModifierLike,
  isModuleBlock: () => isModuleBlock,
  isModuleBody: () => isModuleBody,
  isModuleDeclaration: () => isModuleDeclaration,
  isModuleExportName: () => isModuleExportName,
  isModuleName: () => isModuleName,
  isModuleReference: () => isModuleReference,
  isMultiplicativeOperator: () => isMultiplicativeOperator,
  isMultiplicativeOperatorOrHigher: () => isMultiplicativeOperatorOrHigher,
  isNamedExportBindings: () => isNamedExportBindings,
  isNamedExports: () => isNamedExports,
  isNamedImportBindings: () => isNamedImportBindings,
  isNamedImports: () => isNamedImports,
  isNamedImportsOrExports: () => isNamedImportsOrExports,
  isNamedTupleMember: () => isNamedTupleMember,
  isNamespaceExport: () => isNamespaceExport,
  isNamespaceExportDeclaration: () => isNamespaceExportDeclaration,
  isNamespaceImport: () => isNamespaceImport,
  isNewExpression: () => isNewExpression,
  isNoSubstitutionTemplateLiteral: () => isNoSubstitutionTemplateLiteral,
  isNonNullExpression: () => isNonNullExpression,
  isNotEmittedStatement: () => isNotEmittedStatement,
  isNotEmittedTypeElement: () => isNotEmittedTypeElement,
  isNullLiteral: () => isNullLiteral,
  isNumericLiteral: () => isNumericLiteral,
  isNumericOrStringLikeLiteral: () => isNumericOrStringLikeLiteral,
  isObjectBindingPattern: () => isObjectBindingPattern,
  isObjectDestructuringAssignment: () => isObjectDestructuringAssignment,
  isObjectLiteralElementLike: () => isObjectLiteralElementLike,
  isObjectLiteralExpression: () => isObjectLiteralExpression,
  isObjectLiteralLikeNode: () => isObjectLiteralLikeNode,
  isObjectTypeDeclaration: () => isObjectTypeDeclaration,
  isOmittedExpression: () => isOmittedExpression,
  isOptionalTypeNode: () => isOptionalTypeNode,
  isOutKeyword: () => isOutKeyword,
  isOuterExpression: () => isOuterExpression,
  isOverrideKeyword: () => isOverrideKeyword,
  isParameterDeclaration: () => isParameterDeclaration,
  isParenthesizedExpression: () => isParenthesizedExpression,
  isParenthesizedTypeNode: () => isParenthesizedTypeNode,
  isPartiallyEmittedExpression: () => isPartiallyEmittedExpression,
  isPlusToken: () => isPlusToken,
  isPostfixUnaryExpression: () => isPostfixUnaryExpression,
  isPostfixUnaryOperator: () => isPostfixUnaryOperator,
  isPrefixUnaryExpression: () => isPrefixUnaryExpression,
  isPrefixUnaryOperator: () => isPrefixUnaryOperator,
  isPrivateIdentifier: () => isPrivateIdentifier,
  isPrivateKeyword: () => isPrivateKeyword,
  isPropertyAccessExpression: () => isPropertyAccessExpression,
  isPropertyAssignment: () => isPropertyAssignment,
  isPropertyDeclaration: () => isPropertyDeclaration,
  isPropertyName: () => isPropertyName,
  isPropertyNameLiteral: () => isPropertyNameLiteral,
  isPropertySignatureDeclaration: () => isPropertySignatureDeclaration,
  isProtectedKeyword: () => isProtectedKeyword,
  isPseudoLiteralKind: () => isPseudoLiteralKind,
  isPseudoLiteralToken: () => isPseudoLiteralToken,
  isPublicKeyword: () => isPublicKeyword,
  isPunctuationKind: () => isPunctuationKind,
  isQualifiedName: () => isQualifiedName,
  isQuestionDotToken: () => isQuestionDotToken,
  isQuestionOrExclamationToken: () => isQuestionOrExclamationToken,
  isQuestionOrPlusOrMinusToken: () => isQuestionOrPlusOrMinusToken,
  isQuestionToken: () => isQuestionToken,
  isReadonlyKeyword: () => isReadonlyKeyword,
  isReadonlyKeywordOrPlusOrMinusToken: () => isReadonlyKeywordOrPlusOrMinusToken,
  isRegularExpressionLiteral: () => isRegularExpressionLiteral,
  isRelationalOperator: () => isRelationalOperator,
  isRelationalOperatorOrHigher: () => isRelationalOperatorOrHigher,
  isRestTypeNode: () => isRestTypeNode,
  isReturnStatement: () => isReturnStatement,
  isSatisfiesExpression: () => isSatisfiesExpression,
  isSemicolonClassElement: () => isSemicolonClassElement,
  isSetAccessorDeclaration: () => isSetAccessorDeclaration,
  isShiftOperator: () => isShiftOperator,
  isShiftOperatorOrHigher: () => isShiftOperatorOrHigher,
  isShorthandPropertyAssignment: () => isShorthandPropertyAssignment,
  isSignatureDeclaration: () => isSignatureDeclaration,
  isSourceFile: () => isSourceFile,
  isSpreadAssignment: () => isSpreadAssignment,
  isSpreadElement: () => isSpreadElement,
  isStatement: () => isStatement,
  isStaticKeyword: () => isStaticKeyword,
  isStringLiteral: () => isStringLiteral,
  isStringLiteralLikeNode: () => isStringLiteralLikeNode,
  isSuperExpression: () => isSuperExpression,
  isSwitchStatement: () => isSwitchStatement,
  isSyntaxList: () => isSyntaxList,
  isSyntheticExpression: () => isSyntheticExpression,
  isSyntheticReferenceExpression: () => isSyntheticReferenceExpression,
  isTaggedTemplateExpression: () => isTaggedTemplateExpression,
  isTemplateExpression: () => isTemplateExpression,
  isTemplateHead: () => isTemplateHead,
  isTemplateLiteral: () => isTemplateLiteral,
  isTemplateLiteralLikeNode: () => isTemplateLiteralLikeNode,
  isTemplateLiteralToken: () => isTemplateLiteralToken,
  isTemplateLiteralTypeNode: () => isTemplateLiteralTypeNode,
  isTemplateLiteralTypeSpan: () => isTemplateLiteralTypeSpan,
  isTemplateMiddle: () => isTemplateMiddle,
  isTemplateMiddleOrTail: () => isTemplateMiddleOrTail,
  isTemplateMiddleOrTemplateTail: () => isTemplateMiddleOrTemplateTail,
  isTemplateSpan: () => isTemplateSpan,
  isTemplateTail: () => isTemplateTail,
  isThisExpression: () => isThisExpression,
  isThisTypeNode: () => isThisTypeNode,
  isThrowStatement: () => isThrowStatement,
  isToken: () => isToken,
  isTokenKind: () => isTokenKind,
  isTriviaKind: () => isTriviaKind,
  isTrueLiteral: () => isTrueLiteral,
  isTryStatement: () => isTryStatement,
  isTupleTypeNode: () => isTupleTypeNode,
  isTypeAliasDeclaration: () => isTypeAliasDeclaration,
  isTypeAssertion: () => isTypeAssertion,
  isTypeLiteralNode: () => isTypeLiteralNode,
  isTypeNode: () => isTypeNode,
  isTypeOfExpression: () => isTypeOfExpression,
  isTypeOperatorNode: () => isTypeOperatorNode,
  isTypeParameterDeclaration: () => isTypeParameterDeclaration,
  isTypePredicateNode: () => isTypePredicateNode,
  isTypePredicateParameterName: () => isTypePredicateParameterName,
  isTypeQueryNode: () => isTypeQueryNode,
  isTypeReferenceNode: () => isTypeReferenceNode,
  isUnaryExpression: () => isUnaryExpression,
  isUnicodeIdentifierStart: () => isUnicodeIdentifierStart,
  isUnionOrIntersectionTypeNode: () => isUnionOrIntersectionTypeNode,
  isUnionTypeNode: () => isUnionTypeNode,
  isVariableDeclaration: () => isVariableDeclaration,
  isVariableDeclarationList: () => isVariableDeclarationList,
  isVariableOrParameterDeclaration: () => isVariableOrParameterDeclaration,
  isVariableOrPropertyDeclaration: () => isVariableOrPropertyDeclaration,
  isVariableStatement: () => isVariableStatement,
  isVoidExpression: () => isVoidExpression,
  isWhileStatement: () => isWhileStatement,
  isWhiteSpaceLike: () => isWhiteSpaceLike,
  isWhiteSpaceSingleLine: () => isWhiteSpaceSingleLine,
  isWithStatement: () => isWithStatement,
  isYieldExpression: () => isYieldExpression,
  reduceEachLeadingCommentRange: () => reduceEachLeadingCommentRange,
  reduceEachTrailingCommentRange: () => reduceEachTrailingCommentRange,
  skipOuterExpressions: () => skipOuterExpressions,
  skipPartiallyEmittedExpressions: () => skipPartiallyEmittedExpressions,
  skipTrivia: () => skipTrivia,
  stringToToken: () => stringToToken,
  textToKeywordObj: () => textToKeywordObj,
  tokenIsIdentifierOrKeyword: () => tokenIsIdentifierOrKeyword,
  tokenIsIdentifierOrKeywordOrGreaterThan: () => tokenIsIdentifierOrKeywordOrGreaterThan,
  tokenToString: () => tokenToString,
  tryCast: () => tryCast,
  unescapeLeadingUnderscores: () => unescapeLeadingUnderscores,
  utf16EncodeAsString: () => utf16EncodeAsString,
  visitEachChild: () => visitEachChild,
  visitEachChildOfJSDocParameterTag: () => visitEachChildOfJSDocParameterOrPropertyTag,
  visitEachChildOfJSDocPropertyTag: () => visitEachChildOfJSDocParameterOrPropertyTag,
  visitNode: () => visitNode,
  visitNodes: () => visitNodes,
  visitNodesArray: () => visitNodesArray
});

// node_modules/typescript/dist/enums/characterCodes.js
var CharacterCodes;
(function(CharacterCodes2) {
  CharacterCodes2[CharacterCodes2["EOF"] = -1] = "EOF";
  CharacterCodes2[CharacterCodes2["nullCharacter"] = 0] = "nullCharacter";
  CharacterCodes2[CharacterCodes2["maxAsciiCharacter"] = 127] = "maxAsciiCharacter";
  CharacterCodes2[CharacterCodes2["lineFeed"] = 10] = "lineFeed";
  CharacterCodes2[CharacterCodes2["carriageReturn"] = 13] = "carriageReturn";
  CharacterCodes2[CharacterCodes2["lineSeparator"] = 8232] = "lineSeparator";
  CharacterCodes2[CharacterCodes2["paragraphSeparator"] = 8233] = "paragraphSeparator";
  CharacterCodes2[CharacterCodes2["nextLine"] = 133] = "nextLine";
  CharacterCodes2[CharacterCodes2["space"] = 32] = "space";
  CharacterCodes2[CharacterCodes2["nonBreakingSpace"] = 160] = "nonBreakingSpace";
  CharacterCodes2[CharacterCodes2["enQuad"] = 8192] = "enQuad";
  CharacterCodes2[CharacterCodes2["emQuad"] = 8193] = "emQuad";
  CharacterCodes2[CharacterCodes2["enSpace"] = 8194] = "enSpace";
  CharacterCodes2[CharacterCodes2["emSpace"] = 8195] = "emSpace";
  CharacterCodes2[CharacterCodes2["threePerEmSpace"] = 8196] = "threePerEmSpace";
  CharacterCodes2[CharacterCodes2["fourPerEmSpace"] = 8197] = "fourPerEmSpace";
  CharacterCodes2[CharacterCodes2["sixPerEmSpace"] = 8198] = "sixPerEmSpace";
  CharacterCodes2[CharacterCodes2["figureSpace"] = 8199] = "figureSpace";
  CharacterCodes2[CharacterCodes2["punctuationSpace"] = 8200] = "punctuationSpace";
  CharacterCodes2[CharacterCodes2["thinSpace"] = 8201] = "thinSpace";
  CharacterCodes2[CharacterCodes2["hairSpace"] = 8202] = "hairSpace";
  CharacterCodes2[CharacterCodes2["zeroWidthSpace"] = 8203] = "zeroWidthSpace";
  CharacterCodes2[CharacterCodes2["narrowNoBreakSpace"] = 8239] = "narrowNoBreakSpace";
  CharacterCodes2[CharacterCodes2["ideographicSpace"] = 12288] = "ideographicSpace";
  CharacterCodes2[CharacterCodes2["mathematicalSpace"] = 8287] = "mathematicalSpace";
  CharacterCodes2[CharacterCodes2["ogham"] = 5765] = "ogham";
  CharacterCodes2[CharacterCodes2["replacementCharacter"] = 65533] = "replacementCharacter";
  CharacterCodes2[CharacterCodes2["_"] = 95] = "_";
  CharacterCodes2[CharacterCodes2["$"] = 36] = "$";
  CharacterCodes2[CharacterCodes2["_0"] = 48] = "_0";
  CharacterCodes2[CharacterCodes2["_1"] = 49] = "_1";
  CharacterCodes2[CharacterCodes2["_2"] = 50] = "_2";
  CharacterCodes2[CharacterCodes2["_3"] = 51] = "_3";
  CharacterCodes2[CharacterCodes2["_4"] = 52] = "_4";
  CharacterCodes2[CharacterCodes2["_5"] = 53] = "_5";
  CharacterCodes2[CharacterCodes2["_6"] = 54] = "_6";
  CharacterCodes2[CharacterCodes2["_7"] = 55] = "_7";
  CharacterCodes2[CharacterCodes2["_8"] = 56] = "_8";
  CharacterCodes2[CharacterCodes2["_9"] = 57] = "_9";
  CharacterCodes2[CharacterCodes2["a"] = 97] = "a";
  CharacterCodes2[CharacterCodes2["b"] = 98] = "b";
  CharacterCodes2[CharacterCodes2["c"] = 99] = "c";
  CharacterCodes2[CharacterCodes2["d"] = 100] = "d";
  CharacterCodes2[CharacterCodes2["e"] = 101] = "e";
  CharacterCodes2[CharacterCodes2["f"] = 102] = "f";
  CharacterCodes2[CharacterCodes2["g"] = 103] = "g";
  CharacterCodes2[CharacterCodes2["h"] = 104] = "h";
  CharacterCodes2[CharacterCodes2["i"] = 105] = "i";
  CharacterCodes2[CharacterCodes2["j"] = 106] = "j";
  CharacterCodes2[CharacterCodes2["k"] = 107] = "k";
  CharacterCodes2[CharacterCodes2["l"] = 108] = "l";
  CharacterCodes2[CharacterCodes2["m"] = 109] = "m";
  CharacterCodes2[CharacterCodes2["n"] = 110] = "n";
  CharacterCodes2[CharacterCodes2["o"] = 111] = "o";
  CharacterCodes2[CharacterCodes2["p"] = 112] = "p";
  CharacterCodes2[CharacterCodes2["q"] = 113] = "q";
  CharacterCodes2[CharacterCodes2["r"] = 114] = "r";
  CharacterCodes2[CharacterCodes2["s"] = 115] = "s";
  CharacterCodes2[CharacterCodes2["t"] = 116] = "t";
  CharacterCodes2[CharacterCodes2["u"] = 117] = "u";
  CharacterCodes2[CharacterCodes2["v"] = 118] = "v";
  CharacterCodes2[CharacterCodes2["w"] = 119] = "w";
  CharacterCodes2[CharacterCodes2["x"] = 120] = "x";
  CharacterCodes2[CharacterCodes2["y"] = 121] = "y";
  CharacterCodes2[CharacterCodes2["z"] = 122] = "z";
  CharacterCodes2[CharacterCodes2["A"] = 65] = "A";
  CharacterCodes2[CharacterCodes2["B"] = 66] = "B";
  CharacterCodes2[CharacterCodes2["C"] = 67] = "C";
  CharacterCodes2[CharacterCodes2["D"] = 68] = "D";
  CharacterCodes2[CharacterCodes2["E"] = 69] = "E";
  CharacterCodes2[CharacterCodes2["F"] = 70] = "F";
  CharacterCodes2[CharacterCodes2["G"] = 71] = "G";
  CharacterCodes2[CharacterCodes2["H"] = 72] = "H";
  CharacterCodes2[CharacterCodes2["I"] = 73] = "I";
  CharacterCodes2[CharacterCodes2["J"] = 74] = "J";
  CharacterCodes2[CharacterCodes2["K"] = 75] = "K";
  CharacterCodes2[CharacterCodes2["L"] = 76] = "L";
  CharacterCodes2[CharacterCodes2["M"] = 77] = "M";
  CharacterCodes2[CharacterCodes2["N"] = 78] = "N";
  CharacterCodes2[CharacterCodes2["O"] = 79] = "O";
  CharacterCodes2[CharacterCodes2["P"] = 80] = "P";
  CharacterCodes2[CharacterCodes2["Q"] = 81] = "Q";
  CharacterCodes2[CharacterCodes2["R"] = 82] = "R";
  CharacterCodes2[CharacterCodes2["S"] = 83] = "S";
  CharacterCodes2[CharacterCodes2["T"] = 84] = "T";
  CharacterCodes2[CharacterCodes2["U"] = 85] = "U";
  CharacterCodes2[CharacterCodes2["V"] = 86] = "V";
  CharacterCodes2[CharacterCodes2["W"] = 87] = "W";
  CharacterCodes2[CharacterCodes2["X"] = 88] = "X";
  CharacterCodes2[CharacterCodes2["Y"] = 89] = "Y";
  CharacterCodes2[CharacterCodes2["Z"] = 90] = "Z";
  CharacterCodes2[CharacterCodes2["ampersand"] = 38] = "ampersand";
  CharacterCodes2[CharacterCodes2["asterisk"] = 42] = "asterisk";
  CharacterCodes2[CharacterCodes2["at"] = 64] = "at";
  CharacterCodes2[CharacterCodes2["backslash"] = 92] = "backslash";
  CharacterCodes2[CharacterCodes2["backtick"] = 96] = "backtick";
  CharacterCodes2[CharacterCodes2["bar"] = 124] = "bar";
  CharacterCodes2[CharacterCodes2["caret"] = 94] = "caret";
  CharacterCodes2[CharacterCodes2["closeBrace"] = 125] = "closeBrace";
  CharacterCodes2[CharacterCodes2["closeBracket"] = 93] = "closeBracket";
  CharacterCodes2[CharacterCodes2["closeParen"] = 41] = "closeParen";
  CharacterCodes2[CharacterCodes2["colon"] = 58] = "colon";
  CharacterCodes2[CharacterCodes2["comma"] = 44] = "comma";
  CharacterCodes2[CharacterCodes2["dot"] = 46] = "dot";
  CharacterCodes2[CharacterCodes2["doubleQuote"] = 34] = "doubleQuote";
  CharacterCodes2[CharacterCodes2["equals"] = 61] = "equals";
  CharacterCodes2[CharacterCodes2["exclamation"] = 33] = "exclamation";
  CharacterCodes2[CharacterCodes2["greaterThan"] = 62] = "greaterThan";
  CharacterCodes2[CharacterCodes2["hash"] = 35] = "hash";
  CharacterCodes2[CharacterCodes2["lessThan"] = 60] = "lessThan";
  CharacterCodes2[CharacterCodes2["minus"] = 45] = "minus";
  CharacterCodes2[CharacterCodes2["openBrace"] = 123] = "openBrace";
  CharacterCodes2[CharacterCodes2["openBracket"] = 91] = "openBracket";
  CharacterCodes2[CharacterCodes2["openParen"] = 40] = "openParen";
  CharacterCodes2[CharacterCodes2["percent"] = 37] = "percent";
  CharacterCodes2[CharacterCodes2["plus"] = 43] = "plus";
  CharacterCodes2[CharacterCodes2["question"] = 63] = "question";
  CharacterCodes2[CharacterCodes2["semicolon"] = 59] = "semicolon";
  CharacterCodes2[CharacterCodes2["singleQuote"] = 39] = "singleQuote";
  CharacterCodes2[CharacterCodes2["slash"] = 47] = "slash";
  CharacterCodes2[CharacterCodes2["tilde"] = 126] = "tilde";
  CharacterCodes2[CharacterCodes2["backspace"] = 8] = "backspace";
  CharacterCodes2[CharacterCodes2["formFeed"] = 12] = "formFeed";
  CharacterCodes2[CharacterCodes2["byteOrderMark"] = 65279] = "byteOrderMark";
  CharacterCodes2[CharacterCodes2["tab"] = 9] = "tab";
  CharacterCodes2[CharacterCodes2["verticalTab"] = 11] = "verticalTab";
})(CharacterCodes || (CharacterCodes = {}));

// node_modules/typescript/dist/enums/commentDirectiveType.js
var CommentDirectiveType;
(function(CommentDirectiveType2) {
  CommentDirectiveType2[CommentDirectiveType2["ExpectError"] = 0] = "ExpectError";
  CommentDirectiveType2[CommentDirectiveType2["Ignore"] = 1] = "Ignore";
})(CommentDirectiveType || (CommentDirectiveType = {}));

// node_modules/typescript/dist/enums/internalSymbolName.js
var InternalSymbolName;
(function(InternalSymbolName2) {
  InternalSymbolName2["Call"] = "__call";
  InternalSymbolName2["Constructor"] = "__constructor";
  InternalSymbolName2["New"] = "__new";
  InternalSymbolName2["Index"] = "__index";
  InternalSymbolName2["ExportStar"] = "__export";
  InternalSymbolName2["Global"] = "__global";
  InternalSymbolName2["Missing"] = "__missing";
  InternalSymbolName2["Type"] = "__type";
  InternalSymbolName2["Object"] = "__object";
  InternalSymbolName2["JSXAttributes"] = "__jsxAttributes";
  InternalSymbolName2["Class"] = "__class";
  InternalSymbolName2["Function"] = "__function";
  InternalSymbolName2["Computed"] = "__computed";
  InternalSymbolName2["AssignmentDeclaration"] = "__assignment";
  InternalSymbolName2["InstantiationExpression"] = "__instantiationExpression";
  InternalSymbolName2["ImportAttributes"] = "__importAttributes";
  InternalSymbolName2["ExportEquals"] = "export=";
  InternalSymbolName2["Default"] = "default";
  InternalSymbolName2["This"] = "this";
  InternalSymbolName2["ModuleExports"] = "module.exports";
})(InternalSymbolName || (InternalSymbolName = {}));

// node_modules/typescript/dist/enums/languageVariant.js
var LanguageVariant;
(function(LanguageVariant2) {
  LanguageVariant2[LanguageVariant2["Standard"] = 0] = "Standard";
  LanguageVariant2[LanguageVariant2["JSX"] = 1] = "JSX";
})(LanguageVariant || (LanguageVariant = {}));

// node_modules/typescript/dist/enums/modifierFlags.js
var ModifierFlags;
(function(ModifierFlags2) {
  ModifierFlags2[ModifierFlags2["None"] = 0] = "None";
  ModifierFlags2[ModifierFlags2["Public"] = 1] = "Public";
  ModifierFlags2[ModifierFlags2["Private"] = 2] = "Private";
  ModifierFlags2[ModifierFlags2["Protected"] = 4] = "Protected";
  ModifierFlags2[ModifierFlags2["Readonly"] = 8] = "Readonly";
  ModifierFlags2[ModifierFlags2["Override"] = 16] = "Override";
  ModifierFlags2[ModifierFlags2["Export"] = 32] = "Export";
  ModifierFlags2[ModifierFlags2["Abstract"] = 64] = "Abstract";
  ModifierFlags2[ModifierFlags2["Ambient"] = 128] = "Ambient";
  ModifierFlags2[ModifierFlags2["Static"] = 256] = "Static";
  ModifierFlags2[ModifierFlags2["Accessor"] = 512] = "Accessor";
  ModifierFlags2[ModifierFlags2["Async"] = 1024] = "Async";
  ModifierFlags2[ModifierFlags2["Default"] = 2048] = "Default";
  ModifierFlags2[ModifierFlags2["Const"] = 4096] = "Const";
  ModifierFlags2[ModifierFlags2["In"] = 8192] = "In";
  ModifierFlags2[ModifierFlags2["Out"] = 16384] = "Out";
  ModifierFlags2[ModifierFlags2["Decorator"] = 32768] = "Decorator";
  ModifierFlags2[ModifierFlags2["Deprecated"] = 65536] = "Deprecated";
  ModifierFlags2[ModifierFlags2["JSDocPublic"] = 8388608] = "JSDocPublic";
  ModifierFlags2[ModifierFlags2["JSDocPrivate"] = 16777216] = "JSDocPrivate";
  ModifierFlags2[ModifierFlags2["JSDocProtected"] = 33554432] = "JSDocProtected";
  ModifierFlags2[ModifierFlags2["JSDocReadonly"] = 67108864] = "JSDocReadonly";
  ModifierFlags2[ModifierFlags2["JSDocOverride"] = 134217728] = "JSDocOverride";
  ModifierFlags2[ModifierFlags2["HasComputedJSDocModifiers"] = 268435456] = "HasComputedJSDocModifiers";
  ModifierFlags2[ModifierFlags2["HasComputedFlags"] = 536870912] = "HasComputedFlags";
  ModifierFlags2[ModifierFlags2["SyntacticOrJSDocModifiers"] = 31] = "SyntacticOrJSDocModifiers";
  ModifierFlags2[ModifierFlags2["SyntacticOnlyModifiers"] = 65504] = "SyntacticOnlyModifiers";
  ModifierFlags2[ModifierFlags2["SyntacticModifiers"] = 65535] = "SyntacticModifiers";
  ModifierFlags2[ModifierFlags2["JSDocCacheOnlyModifiers"] = 260046848] = "JSDocCacheOnlyModifiers";
  ModifierFlags2[ModifierFlags2["JSDocOnlyModifiers"] = 65536] = "JSDocOnlyModifiers";
  ModifierFlags2[ModifierFlags2["NonCacheOnlyModifiers"] = 131071] = "NonCacheOnlyModifiers";
  ModifierFlags2[ModifierFlags2["AccessibilityModifier"] = 7] = "AccessibilityModifier";
  ModifierFlags2[ModifierFlags2["ParameterPropertyModifier"] = 31] = "ParameterPropertyModifier";
  ModifierFlags2[ModifierFlags2["NonPublicAccessibilityModifier"] = 6] = "NonPublicAccessibilityModifier";
  ModifierFlags2[ModifierFlags2["TypeScriptModifier"] = 28895] = "TypeScriptModifier";
  ModifierFlags2[ModifierFlags2["ExportDefault"] = 2080] = "ExportDefault";
  ModifierFlags2[ModifierFlags2["All"] = 131071] = "All";
  ModifierFlags2[ModifierFlags2["Modifier"] = 98303] = "Modifier";
  ModifierFlags2[ModifierFlags2["JavaScript"] = 3872] = "JavaScript";
})(ModifierFlags || (ModifierFlags = {}));

// node_modules/typescript/dist/enums/nodeFlags.js
var NodeFlags;
(function(NodeFlags2) {
  NodeFlags2[NodeFlags2["None"] = 0] = "None";
  NodeFlags2[NodeFlags2["Let"] = 1] = "Let";
  NodeFlags2[NodeFlags2["Const"] = 2] = "Const";
  NodeFlags2[NodeFlags2["Using"] = 4] = "Using";
  NodeFlags2[NodeFlags2["Reparsed"] = 8] = "Reparsed";
  NodeFlags2[NodeFlags2["Synthesized"] = 16] = "Synthesized";
  NodeFlags2[NodeFlags2["OptionalChain"] = 32] = "OptionalChain";
  NodeFlags2[NodeFlags2["ExportContext"] = 64] = "ExportContext";
  NodeFlags2[NodeFlags2["ContainsThis"] = 128] = "ContainsThis";
  NodeFlags2[NodeFlags2["HasImplicitReturn"] = 256] = "HasImplicitReturn";
  NodeFlags2[NodeFlags2["HasExplicitReturn"] = 512] = "HasExplicitReturn";
  NodeFlags2[NodeFlags2["DisallowInContext"] = 1024] = "DisallowInContext";
  NodeFlags2[NodeFlags2["YieldContext"] = 2048] = "YieldContext";
  NodeFlags2[NodeFlags2["DecoratorContext"] = 4096] = "DecoratorContext";
  NodeFlags2[NodeFlags2["AwaitContext"] = 8192] = "AwaitContext";
  NodeFlags2[NodeFlags2["DisallowConditionalTypesContext"] = 16384] = "DisallowConditionalTypesContext";
  NodeFlags2[NodeFlags2["ThisNodeHasError"] = 32768] = "ThisNodeHasError";
  NodeFlags2[NodeFlags2["JavaScriptFile"] = 65536] = "JavaScriptFile";
  NodeFlags2[NodeFlags2["ThisNodeOrAnySubNodesHasError"] = 131072] = "ThisNodeOrAnySubNodesHasError";
  NodeFlags2[NodeFlags2["HasAsyncFunctions"] = 262144] = "HasAsyncFunctions";
  NodeFlags2[NodeFlags2["PossiblyContainsDynamicImport"] = 524288] = "PossiblyContainsDynamicImport";
  NodeFlags2[NodeFlags2["PossiblyContainsImportMeta"] = 1048576] = "PossiblyContainsImportMeta";
  NodeFlags2[NodeFlags2["HasJSDoc"] = 2097152] = "HasJSDoc";
  NodeFlags2[NodeFlags2["JSDoc"] = 4194304] = "JSDoc";
  NodeFlags2[NodeFlags2["Ambient"] = 8388608] = "Ambient";
  NodeFlags2[NodeFlags2["InWithStatement"] = 16777216] = "InWithStatement";
  NodeFlags2[NodeFlags2["JsonFile"] = 33554432] = "JsonFile";
  NodeFlags2[NodeFlags2["PossiblyContainsDeprecatedTag"] = 67108864] = "PossiblyContainsDeprecatedTag";
  NodeFlags2[NodeFlags2["Unreachable"] = 134217728] = "Unreachable";
  NodeFlags2[NodeFlags2["ReparserTransformedLiteral"] = 268435456] = "ReparserTransformedLiteral";
  NodeFlags2[NodeFlags2["BlockScoped"] = 7] = "BlockScoped";
  NodeFlags2[NodeFlags2["Constant"] = 6] = "Constant";
  NodeFlags2[NodeFlags2["AwaitUsing"] = 6] = "AwaitUsing";
  NodeFlags2[NodeFlags2["ReachabilityCheckFlags"] = 768] = "ReachabilityCheckFlags";
  NodeFlags2[NodeFlags2["ReachabilityAndEmitFlags"] = 262912] = "ReachabilityAndEmitFlags";
  NodeFlags2[NodeFlags2["ContextFlags"] = 25263104] = "ContextFlags";
  NodeFlags2[NodeFlags2["TypeExcludesFlags"] = 10240] = "TypeExcludesFlags";
  NodeFlags2[NodeFlags2["PermanentlySetIncrementalFlags"] = 1572864] = "PermanentlySetIncrementalFlags";
  NodeFlags2[NodeFlags2["IdentifierHasExtendedUnicodeEscape"] = 128] = "IdentifierHasExtendedUnicodeEscape";
  NodeFlags2[NodeFlags2["IdentifierIsInJSDocNamespace"] = 262144] = "IdentifierIsInJSDocNamespace";
  NodeFlags2[NodeFlags2["NestedNamespace"] = 32] = "NestedNamespace";
})(NodeFlags || (NodeFlags = {}));

// node_modules/typescript/dist/enums/regularExpressionFlags.js
var RegularExpressionFlags;
(function(RegularExpressionFlags2) {
  RegularExpressionFlags2[RegularExpressionFlags2["None"] = 0] = "None";
  RegularExpressionFlags2[RegularExpressionFlags2["HasIndices"] = 1] = "HasIndices";
  RegularExpressionFlags2[RegularExpressionFlags2["Global"] = 2] = "Global";
  RegularExpressionFlags2[RegularExpressionFlags2["IgnoreCase"] = 4] = "IgnoreCase";
  RegularExpressionFlags2[RegularExpressionFlags2["Multiline"] = 8] = "Multiline";
  RegularExpressionFlags2[RegularExpressionFlags2["DotAll"] = 16] = "DotAll";
  RegularExpressionFlags2[RegularExpressionFlags2["Unicode"] = 32] = "Unicode";
  RegularExpressionFlags2[RegularExpressionFlags2["UnicodeSets"] = 64] = "UnicodeSets";
  RegularExpressionFlags2[RegularExpressionFlags2["Sticky"] = 128] = "Sticky";
  RegularExpressionFlags2[RegularExpressionFlags2["AnyUnicodeMode"] = 96] = "AnyUnicodeMode";
})(RegularExpressionFlags || (RegularExpressionFlags = {}));

// node_modules/typescript/dist/enums/scriptKind.js
var ScriptKind;
(function(ScriptKind2) {
  ScriptKind2[ScriptKind2["Unknown"] = 0] = "Unknown";
  ScriptKind2[ScriptKind2["JS"] = 1] = "JS";
  ScriptKind2[ScriptKind2["JSX"] = 2] = "JSX";
  ScriptKind2[ScriptKind2["TS"] = 3] = "TS";
  ScriptKind2[ScriptKind2["TSX"] = 4] = "TSX";
  ScriptKind2[ScriptKind2["External"] = 5] = "External";
  ScriptKind2[ScriptKind2["JSON"] = 6] = "JSON";
  ScriptKind2[ScriptKind2["Deferred"] = 7] = "Deferred";
})(ScriptKind || (ScriptKind = {}));

// node_modules/typescript/dist/enums/scriptTarget.js
var ScriptTarget;
(function(ScriptTarget2) {
  ScriptTarget2[ScriptTarget2["ES2015"] = 2] = "ES2015";
  ScriptTarget2[ScriptTarget2["ES2016"] = 3] = "ES2016";
  ScriptTarget2[ScriptTarget2["ES2017"] = 4] = "ES2017";
  ScriptTarget2[ScriptTarget2["ES2018"] = 5] = "ES2018";
  ScriptTarget2[ScriptTarget2["ES2019"] = 6] = "ES2019";
  ScriptTarget2[ScriptTarget2["ES2020"] = 7] = "ES2020";
  ScriptTarget2[ScriptTarget2["ES2021"] = 8] = "ES2021";
  ScriptTarget2[ScriptTarget2["ES2022"] = 9] = "ES2022";
  ScriptTarget2[ScriptTarget2["ES2023"] = 10] = "ES2023";
  ScriptTarget2[ScriptTarget2["ES2024"] = 11] = "ES2024";
  ScriptTarget2[ScriptTarget2["ES2025"] = 12] = "ES2025";
  ScriptTarget2[ScriptTarget2["ESNext"] = 99] = "ESNext";
  ScriptTarget2[ScriptTarget2["JSON"] = 100] = "JSON";
  ScriptTarget2[ScriptTarget2["Latest"] = 99] = "Latest";
})(ScriptTarget || (ScriptTarget = {}));

// node_modules/typescript/dist/enums/syntaxKind.js
var SyntaxKind;
(function(SyntaxKind2) {
  SyntaxKind2[SyntaxKind2["Unknown"] = 0] = "Unknown";
  SyntaxKind2[SyntaxKind2["EndOfFile"] = 1] = "EndOfFile";
  SyntaxKind2[SyntaxKind2["SingleLineCommentTrivia"] = 2] = "SingleLineCommentTrivia";
  SyntaxKind2[SyntaxKind2["MultiLineCommentTrivia"] = 3] = "MultiLineCommentTrivia";
  SyntaxKind2[SyntaxKind2["NewLineTrivia"] = 4] = "NewLineTrivia";
  SyntaxKind2[SyntaxKind2["WhitespaceTrivia"] = 5] = "WhitespaceTrivia";
  SyntaxKind2[SyntaxKind2["ConflictMarkerTrivia"] = 6] = "ConflictMarkerTrivia";
  SyntaxKind2[SyntaxKind2["NonTextFileMarkerTrivia"] = 7] = "NonTextFileMarkerTrivia";
  SyntaxKind2[SyntaxKind2["NumericLiteral"] = 8] = "NumericLiteral";
  SyntaxKind2[SyntaxKind2["BigIntLiteral"] = 9] = "BigIntLiteral";
  SyntaxKind2[SyntaxKind2["StringLiteral"] = 10] = "StringLiteral";
  SyntaxKind2[SyntaxKind2["JsxText"] = 11] = "JsxText";
  SyntaxKind2[SyntaxKind2["JsxTextAllWhiteSpaces"] = 12] = "JsxTextAllWhiteSpaces";
  SyntaxKind2[SyntaxKind2["RegularExpressionLiteral"] = 13] = "RegularExpressionLiteral";
  SyntaxKind2[SyntaxKind2["NoSubstitutionTemplateLiteral"] = 14] = "NoSubstitutionTemplateLiteral";
  SyntaxKind2[SyntaxKind2["TemplateHead"] = 15] = "TemplateHead";
  SyntaxKind2[SyntaxKind2["TemplateMiddle"] = 16] = "TemplateMiddle";
  SyntaxKind2[SyntaxKind2["TemplateTail"] = 17] = "TemplateTail";
  SyntaxKind2[SyntaxKind2["OpenBraceToken"] = 18] = "OpenBraceToken";
  SyntaxKind2[SyntaxKind2["CloseBraceToken"] = 19] = "CloseBraceToken";
  SyntaxKind2[SyntaxKind2["OpenParenToken"] = 20] = "OpenParenToken";
  SyntaxKind2[SyntaxKind2["CloseParenToken"] = 21] = "CloseParenToken";
  SyntaxKind2[SyntaxKind2["OpenBracketToken"] = 22] = "OpenBracketToken";
  SyntaxKind2[SyntaxKind2["CloseBracketToken"] = 23] = "CloseBracketToken";
  SyntaxKind2[SyntaxKind2["DotToken"] = 24] = "DotToken";
  SyntaxKind2[SyntaxKind2["DotDotDotToken"] = 25] = "DotDotDotToken";
  SyntaxKind2[SyntaxKind2["SemicolonToken"] = 26] = "SemicolonToken";
  SyntaxKind2[SyntaxKind2["CommaToken"] = 27] = "CommaToken";
  SyntaxKind2[SyntaxKind2["QuestionDotToken"] = 28] = "QuestionDotToken";
  SyntaxKind2[SyntaxKind2["LessThanToken"] = 29] = "LessThanToken";
  SyntaxKind2[SyntaxKind2["LessThanSlashToken"] = 30] = "LessThanSlashToken";
  SyntaxKind2[SyntaxKind2["GreaterThanToken"] = 31] = "GreaterThanToken";
  SyntaxKind2[SyntaxKind2["LessThanEqualsToken"] = 32] = "LessThanEqualsToken";
  SyntaxKind2[SyntaxKind2["GreaterThanEqualsToken"] = 33] = "GreaterThanEqualsToken";
  SyntaxKind2[SyntaxKind2["EqualsEqualsToken"] = 34] = "EqualsEqualsToken";
  SyntaxKind2[SyntaxKind2["ExclamationEqualsToken"] = 35] = "ExclamationEqualsToken";
  SyntaxKind2[SyntaxKind2["EqualsEqualsEqualsToken"] = 36] = "EqualsEqualsEqualsToken";
  SyntaxKind2[SyntaxKind2["ExclamationEqualsEqualsToken"] = 37] = "ExclamationEqualsEqualsToken";
  SyntaxKind2[SyntaxKind2["EqualsGreaterThanToken"] = 38] = "EqualsGreaterThanToken";
  SyntaxKind2[SyntaxKind2["PlusToken"] = 39] = "PlusToken";
  SyntaxKind2[SyntaxKind2["MinusToken"] = 40] = "MinusToken";
  SyntaxKind2[SyntaxKind2["AsteriskToken"] = 41] = "AsteriskToken";
  SyntaxKind2[SyntaxKind2["AsteriskAsteriskToken"] = 42] = "AsteriskAsteriskToken";
  SyntaxKind2[SyntaxKind2["SlashToken"] = 43] = "SlashToken";
  SyntaxKind2[SyntaxKind2["PercentToken"] = 44] = "PercentToken";
  SyntaxKind2[SyntaxKind2["PlusPlusToken"] = 45] = "PlusPlusToken";
  SyntaxKind2[SyntaxKind2["MinusMinusToken"] = 46] = "MinusMinusToken";
  SyntaxKind2[SyntaxKind2["LessThanLessThanToken"] = 47] = "LessThanLessThanToken";
  SyntaxKind2[SyntaxKind2["GreaterThanGreaterThanToken"] = 48] = "GreaterThanGreaterThanToken";
  SyntaxKind2[SyntaxKind2["GreaterThanGreaterThanGreaterThanToken"] = 49] = "GreaterThanGreaterThanGreaterThanToken";
  SyntaxKind2[SyntaxKind2["AmpersandToken"] = 50] = "AmpersandToken";
  SyntaxKind2[SyntaxKind2["BarToken"] = 51] = "BarToken";
  SyntaxKind2[SyntaxKind2["CaretToken"] = 52] = "CaretToken";
  SyntaxKind2[SyntaxKind2["ExclamationToken"] = 53] = "ExclamationToken";
  SyntaxKind2[SyntaxKind2["TildeToken"] = 54] = "TildeToken";
  SyntaxKind2[SyntaxKind2["AmpersandAmpersandToken"] = 55] = "AmpersandAmpersandToken";
  SyntaxKind2[SyntaxKind2["BarBarToken"] = 56] = "BarBarToken";
  SyntaxKind2[SyntaxKind2["QuestionToken"] = 57] = "QuestionToken";
  SyntaxKind2[SyntaxKind2["ColonToken"] = 58] = "ColonToken";
  SyntaxKind2[SyntaxKind2["AtToken"] = 59] = "AtToken";
  SyntaxKind2[SyntaxKind2["QuestionQuestionToken"] = 60] = "QuestionQuestionToken";
  SyntaxKind2[SyntaxKind2["BacktickToken"] = 61] = "BacktickToken";
  SyntaxKind2[SyntaxKind2["HashToken"] = 62] = "HashToken";
  SyntaxKind2[SyntaxKind2["EqualsToken"] = 63] = "EqualsToken";
  SyntaxKind2[SyntaxKind2["PlusEqualsToken"] = 64] = "PlusEqualsToken";
  SyntaxKind2[SyntaxKind2["MinusEqualsToken"] = 65] = "MinusEqualsToken";
  SyntaxKind2[SyntaxKind2["AsteriskEqualsToken"] = 66] = "AsteriskEqualsToken";
  SyntaxKind2[SyntaxKind2["AsteriskAsteriskEqualsToken"] = 67] = "AsteriskAsteriskEqualsToken";
  SyntaxKind2[SyntaxKind2["SlashEqualsToken"] = 68] = "SlashEqualsToken";
  SyntaxKind2[SyntaxKind2["PercentEqualsToken"] = 69] = "PercentEqualsToken";
  SyntaxKind2[SyntaxKind2["LessThanLessThanEqualsToken"] = 70] = "LessThanLessThanEqualsToken";
  SyntaxKind2[SyntaxKind2["GreaterThanGreaterThanEqualsToken"] = 71] = "GreaterThanGreaterThanEqualsToken";
  SyntaxKind2[SyntaxKind2["GreaterThanGreaterThanGreaterThanEqualsToken"] = 72] = "GreaterThanGreaterThanGreaterThanEqualsToken";
  SyntaxKind2[SyntaxKind2["AmpersandEqualsToken"] = 73] = "AmpersandEqualsToken";
  SyntaxKind2[SyntaxKind2["BarEqualsToken"] = 74] = "BarEqualsToken";
  SyntaxKind2[SyntaxKind2["BarBarEqualsToken"] = 75] = "BarBarEqualsToken";
  SyntaxKind2[SyntaxKind2["AmpersandAmpersandEqualsToken"] = 76] = "AmpersandAmpersandEqualsToken";
  SyntaxKind2[SyntaxKind2["QuestionQuestionEqualsToken"] = 77] = "QuestionQuestionEqualsToken";
  SyntaxKind2[SyntaxKind2["CaretEqualsToken"] = 78] = "CaretEqualsToken";
  SyntaxKind2[SyntaxKind2["Identifier"] = 79] = "Identifier";
  SyntaxKind2[SyntaxKind2["PrivateIdentifier"] = 80] = "PrivateIdentifier";
  SyntaxKind2[SyntaxKind2["JSDocCommentTextToken"] = 81] = "JSDocCommentTextToken";
  SyntaxKind2[SyntaxKind2["BreakKeyword"] = 82] = "BreakKeyword";
  SyntaxKind2[SyntaxKind2["CaseKeyword"] = 83] = "CaseKeyword";
  SyntaxKind2[SyntaxKind2["CatchKeyword"] = 84] = "CatchKeyword";
  SyntaxKind2[SyntaxKind2["ClassKeyword"] = 85] = "ClassKeyword";
  SyntaxKind2[SyntaxKind2["ConstKeyword"] = 86] = "ConstKeyword";
  SyntaxKind2[SyntaxKind2["ContinueKeyword"] = 87] = "ContinueKeyword";
  SyntaxKind2[SyntaxKind2["DebuggerKeyword"] = 88] = "DebuggerKeyword";
  SyntaxKind2[SyntaxKind2["DefaultKeyword"] = 89] = "DefaultKeyword";
  SyntaxKind2[SyntaxKind2["DeleteKeyword"] = 90] = "DeleteKeyword";
  SyntaxKind2[SyntaxKind2["DoKeyword"] = 91] = "DoKeyword";
  SyntaxKind2[SyntaxKind2["ElseKeyword"] = 92] = "ElseKeyword";
  SyntaxKind2[SyntaxKind2["EnumKeyword"] = 93] = "EnumKeyword";
  SyntaxKind2[SyntaxKind2["ExportKeyword"] = 94] = "ExportKeyword";
  SyntaxKind2[SyntaxKind2["ExtendsKeyword"] = 95] = "ExtendsKeyword";
  SyntaxKind2[SyntaxKind2["FalseKeyword"] = 96] = "FalseKeyword";
  SyntaxKind2[SyntaxKind2["FinallyKeyword"] = 97] = "FinallyKeyword";
  SyntaxKind2[SyntaxKind2["ForKeyword"] = 98] = "ForKeyword";
  SyntaxKind2[SyntaxKind2["FunctionKeyword"] = 99] = "FunctionKeyword";
  SyntaxKind2[SyntaxKind2["IfKeyword"] = 100] = "IfKeyword";
  SyntaxKind2[SyntaxKind2["ImportKeyword"] = 101] = "ImportKeyword";
  SyntaxKind2[SyntaxKind2["InKeyword"] = 102] = "InKeyword";
  SyntaxKind2[SyntaxKind2["InstanceOfKeyword"] = 103] = "InstanceOfKeyword";
  SyntaxKind2[SyntaxKind2["NewKeyword"] = 104] = "NewKeyword";
  SyntaxKind2[SyntaxKind2["NullKeyword"] = 105] = "NullKeyword";
  SyntaxKind2[SyntaxKind2["ReturnKeyword"] = 106] = "ReturnKeyword";
  SyntaxKind2[SyntaxKind2["SuperKeyword"] = 107] = "SuperKeyword";
  SyntaxKind2[SyntaxKind2["SwitchKeyword"] = 108] = "SwitchKeyword";
  SyntaxKind2[SyntaxKind2["ThisKeyword"] = 109] = "ThisKeyword";
  SyntaxKind2[SyntaxKind2["ThrowKeyword"] = 110] = "ThrowKeyword";
  SyntaxKind2[SyntaxKind2["TrueKeyword"] = 111] = "TrueKeyword";
  SyntaxKind2[SyntaxKind2["TryKeyword"] = 112] = "TryKeyword";
  SyntaxKind2[SyntaxKind2["TypeOfKeyword"] = 113] = "TypeOfKeyword";
  SyntaxKind2[SyntaxKind2["VarKeyword"] = 114] = "VarKeyword";
  SyntaxKind2[SyntaxKind2["VoidKeyword"] = 115] = "VoidKeyword";
  SyntaxKind2[SyntaxKind2["WhileKeyword"] = 116] = "WhileKeyword";
  SyntaxKind2[SyntaxKind2["WithKeyword"] = 117] = "WithKeyword";
  SyntaxKind2[SyntaxKind2["ImplementsKeyword"] = 118] = "ImplementsKeyword";
  SyntaxKind2[SyntaxKind2["InterfaceKeyword"] = 119] = "InterfaceKeyword";
  SyntaxKind2[SyntaxKind2["LetKeyword"] = 120] = "LetKeyword";
  SyntaxKind2[SyntaxKind2["PackageKeyword"] = 121] = "PackageKeyword";
  SyntaxKind2[SyntaxKind2["PrivateKeyword"] = 122] = "PrivateKeyword";
  SyntaxKind2[SyntaxKind2["ProtectedKeyword"] = 123] = "ProtectedKeyword";
  SyntaxKind2[SyntaxKind2["PublicKeyword"] = 124] = "PublicKeyword";
  SyntaxKind2[SyntaxKind2["StaticKeyword"] = 125] = "StaticKeyword";
  SyntaxKind2[SyntaxKind2["YieldKeyword"] = 126] = "YieldKeyword";
  SyntaxKind2[SyntaxKind2["AbstractKeyword"] = 127] = "AbstractKeyword";
  SyntaxKind2[SyntaxKind2["AccessorKeyword"] = 128] = "AccessorKeyword";
  SyntaxKind2[SyntaxKind2["AsKeyword"] = 129] = "AsKeyword";
  SyntaxKind2[SyntaxKind2["AssertsKeyword"] = 130] = "AssertsKeyword";
  SyntaxKind2[SyntaxKind2["AssertKeyword"] = 131] = "AssertKeyword";
  SyntaxKind2[SyntaxKind2["AnyKeyword"] = 132] = "AnyKeyword";
  SyntaxKind2[SyntaxKind2["AsyncKeyword"] = 133] = "AsyncKeyword";
  SyntaxKind2[SyntaxKind2["AwaitKeyword"] = 134] = "AwaitKeyword";
  SyntaxKind2[SyntaxKind2["BooleanKeyword"] = 135] = "BooleanKeyword";
  SyntaxKind2[SyntaxKind2["ConstructorKeyword"] = 136] = "ConstructorKeyword";
  SyntaxKind2[SyntaxKind2["DeclareKeyword"] = 137] = "DeclareKeyword";
  SyntaxKind2[SyntaxKind2["GetKeyword"] = 138] = "GetKeyword";
  SyntaxKind2[SyntaxKind2["ImmediateKeyword"] = 139] = "ImmediateKeyword";
  SyntaxKind2[SyntaxKind2["InferKeyword"] = 140] = "InferKeyword";
  SyntaxKind2[SyntaxKind2["IntrinsicKeyword"] = 141] = "IntrinsicKeyword";
  SyntaxKind2[SyntaxKind2["IsKeyword"] = 142] = "IsKeyword";
  SyntaxKind2[SyntaxKind2["KeyOfKeyword"] = 143] = "KeyOfKeyword";
  SyntaxKind2[SyntaxKind2["ModuleKeyword"] = 144] = "ModuleKeyword";
  SyntaxKind2[SyntaxKind2["NamespaceKeyword"] = 145] = "NamespaceKeyword";
  SyntaxKind2[SyntaxKind2["NeverKeyword"] = 146] = "NeverKeyword";
  SyntaxKind2[SyntaxKind2["OutKeyword"] = 147] = "OutKeyword";
  SyntaxKind2[SyntaxKind2["ReadonlyKeyword"] = 148] = "ReadonlyKeyword";
  SyntaxKind2[SyntaxKind2["RequireKeyword"] = 149] = "RequireKeyword";
  SyntaxKind2[SyntaxKind2["NumberKeyword"] = 150] = "NumberKeyword";
  SyntaxKind2[SyntaxKind2["ObjectKeyword"] = 151] = "ObjectKeyword";
  SyntaxKind2[SyntaxKind2["SatisfiesKeyword"] = 152] = "SatisfiesKeyword";
  SyntaxKind2[SyntaxKind2["SetKeyword"] = 153] = "SetKeyword";
  SyntaxKind2[SyntaxKind2["StringKeyword"] = 154] = "StringKeyword";
  SyntaxKind2[SyntaxKind2["SymbolKeyword"] = 155] = "SymbolKeyword";
  SyntaxKind2[SyntaxKind2["TypeKeyword"] = 156] = "TypeKeyword";
  SyntaxKind2[SyntaxKind2["UndefinedKeyword"] = 157] = "UndefinedKeyword";
  SyntaxKind2[SyntaxKind2["UniqueKeyword"] = 158] = "UniqueKeyword";
  SyntaxKind2[SyntaxKind2["UnknownKeyword"] = 159] = "UnknownKeyword";
  SyntaxKind2[SyntaxKind2["UsingKeyword"] = 160] = "UsingKeyword";
  SyntaxKind2[SyntaxKind2["FromKeyword"] = 161] = "FromKeyword";
  SyntaxKind2[SyntaxKind2["GlobalKeyword"] = 162] = "GlobalKeyword";
  SyntaxKind2[SyntaxKind2["BigIntKeyword"] = 163] = "BigIntKeyword";
  SyntaxKind2[SyntaxKind2["OverrideKeyword"] = 164] = "OverrideKeyword";
  SyntaxKind2[SyntaxKind2["OfKeyword"] = 165] = "OfKeyword";
  SyntaxKind2[SyntaxKind2["DeferKeyword"] = 166] = "DeferKeyword";
  SyntaxKind2[SyntaxKind2["QualifiedName"] = 167] = "QualifiedName";
  SyntaxKind2[SyntaxKind2["ComputedPropertyName"] = 168] = "ComputedPropertyName";
  SyntaxKind2[SyntaxKind2["TypeParameter"] = 169] = "TypeParameter";
  SyntaxKind2[SyntaxKind2["Parameter"] = 170] = "Parameter";
  SyntaxKind2[SyntaxKind2["Decorator"] = 171] = "Decorator";
  SyntaxKind2[SyntaxKind2["PropertySignature"] = 172] = "PropertySignature";
  SyntaxKind2[SyntaxKind2["PropertyDeclaration"] = 173] = "PropertyDeclaration";
  SyntaxKind2[SyntaxKind2["MethodSignature"] = 174] = "MethodSignature";
  SyntaxKind2[SyntaxKind2["MethodDeclaration"] = 175] = "MethodDeclaration";
  SyntaxKind2[SyntaxKind2["ClassStaticBlockDeclaration"] = 176] = "ClassStaticBlockDeclaration";
  SyntaxKind2[SyntaxKind2["Constructor"] = 177] = "Constructor";
  SyntaxKind2[SyntaxKind2["GetAccessor"] = 178] = "GetAccessor";
  SyntaxKind2[SyntaxKind2["SetAccessor"] = 179] = "SetAccessor";
  SyntaxKind2[SyntaxKind2["CallSignature"] = 180] = "CallSignature";
  SyntaxKind2[SyntaxKind2["ConstructSignature"] = 181] = "ConstructSignature";
  SyntaxKind2[SyntaxKind2["IndexSignature"] = 182] = "IndexSignature";
  SyntaxKind2[SyntaxKind2["TypePredicate"] = 183] = "TypePredicate";
  SyntaxKind2[SyntaxKind2["TypeReference"] = 184] = "TypeReference";
  SyntaxKind2[SyntaxKind2["FunctionType"] = 185] = "FunctionType";
  SyntaxKind2[SyntaxKind2["ConstructorType"] = 186] = "ConstructorType";
  SyntaxKind2[SyntaxKind2["TypeQuery"] = 187] = "TypeQuery";
  SyntaxKind2[SyntaxKind2["TypeLiteral"] = 188] = "TypeLiteral";
  SyntaxKind2[SyntaxKind2["ArrayType"] = 189] = "ArrayType";
  SyntaxKind2[SyntaxKind2["TupleType"] = 190] = "TupleType";
  SyntaxKind2[SyntaxKind2["OptionalType"] = 191] = "OptionalType";
  SyntaxKind2[SyntaxKind2["RestType"] = 192] = "RestType";
  SyntaxKind2[SyntaxKind2["UnionType"] = 193] = "UnionType";
  SyntaxKind2[SyntaxKind2["IntersectionType"] = 194] = "IntersectionType";
  SyntaxKind2[SyntaxKind2["ConditionalType"] = 195] = "ConditionalType";
  SyntaxKind2[SyntaxKind2["InferType"] = 196] = "InferType";
  SyntaxKind2[SyntaxKind2["ParenthesizedType"] = 197] = "ParenthesizedType";
  SyntaxKind2[SyntaxKind2["ThisType"] = 198] = "ThisType";
  SyntaxKind2[SyntaxKind2["TypeOperator"] = 199] = "TypeOperator";
  SyntaxKind2[SyntaxKind2["IndexedAccessType"] = 200] = "IndexedAccessType";
  SyntaxKind2[SyntaxKind2["MappedType"] = 201] = "MappedType";
  SyntaxKind2[SyntaxKind2["LiteralType"] = 202] = "LiteralType";
  SyntaxKind2[SyntaxKind2["NamedTupleMember"] = 203] = "NamedTupleMember";
  SyntaxKind2[SyntaxKind2["TemplateLiteralType"] = 204] = "TemplateLiteralType";
  SyntaxKind2[SyntaxKind2["TemplateLiteralTypeSpan"] = 205] = "TemplateLiteralTypeSpan";
  SyntaxKind2[SyntaxKind2["ImportType"] = 206] = "ImportType";
  SyntaxKind2[SyntaxKind2["ObjectBindingPattern"] = 207] = "ObjectBindingPattern";
  SyntaxKind2[SyntaxKind2["ArrayBindingPattern"] = 208] = "ArrayBindingPattern";
  SyntaxKind2[SyntaxKind2["BindingElement"] = 209] = "BindingElement";
  SyntaxKind2[SyntaxKind2["ArrayLiteralExpression"] = 210] = "ArrayLiteralExpression";
  SyntaxKind2[SyntaxKind2["ObjectLiteralExpression"] = 211] = "ObjectLiteralExpression";
  SyntaxKind2[SyntaxKind2["PropertyAccessExpression"] = 212] = "PropertyAccessExpression";
  SyntaxKind2[SyntaxKind2["ElementAccessExpression"] = 213] = "ElementAccessExpression";
  SyntaxKind2[SyntaxKind2["CallExpression"] = 214] = "CallExpression";
  SyntaxKind2[SyntaxKind2["NewExpression"] = 215] = "NewExpression";
  SyntaxKind2[SyntaxKind2["TaggedTemplateExpression"] = 216] = "TaggedTemplateExpression";
  SyntaxKind2[SyntaxKind2["TypeAssertionExpression"] = 217] = "TypeAssertionExpression";
  SyntaxKind2[SyntaxKind2["ParenthesizedExpression"] = 218] = "ParenthesizedExpression";
  SyntaxKind2[SyntaxKind2["FunctionExpression"] = 219] = "FunctionExpression";
  SyntaxKind2[SyntaxKind2["ArrowFunction"] = 220] = "ArrowFunction";
  SyntaxKind2[SyntaxKind2["DeleteExpression"] = 221] = "DeleteExpression";
  SyntaxKind2[SyntaxKind2["TypeOfExpression"] = 222] = "TypeOfExpression";
  SyntaxKind2[SyntaxKind2["VoidExpression"] = 223] = "VoidExpression";
  SyntaxKind2[SyntaxKind2["AwaitExpression"] = 224] = "AwaitExpression";
  SyntaxKind2[SyntaxKind2["PrefixUnaryExpression"] = 225] = "PrefixUnaryExpression";
  SyntaxKind2[SyntaxKind2["PostfixUnaryExpression"] = 226] = "PostfixUnaryExpression";
  SyntaxKind2[SyntaxKind2["BinaryExpression"] = 227] = "BinaryExpression";
  SyntaxKind2[SyntaxKind2["ConditionalExpression"] = 228] = "ConditionalExpression";
  SyntaxKind2[SyntaxKind2["TemplateExpression"] = 229] = "TemplateExpression";
  SyntaxKind2[SyntaxKind2["YieldExpression"] = 230] = "YieldExpression";
  SyntaxKind2[SyntaxKind2["SpreadElement"] = 231] = "SpreadElement";
  SyntaxKind2[SyntaxKind2["ClassExpression"] = 232] = "ClassExpression";
  SyntaxKind2[SyntaxKind2["OmittedExpression"] = 233] = "OmittedExpression";
  SyntaxKind2[SyntaxKind2["ExpressionWithTypeArguments"] = 234] = "ExpressionWithTypeArguments";
  SyntaxKind2[SyntaxKind2["AsExpression"] = 235] = "AsExpression";
  SyntaxKind2[SyntaxKind2["NonNullExpression"] = 236] = "NonNullExpression";
  SyntaxKind2[SyntaxKind2["MetaProperty"] = 237] = "MetaProperty";
  SyntaxKind2[SyntaxKind2["SyntheticExpression"] = 238] = "SyntheticExpression";
  SyntaxKind2[SyntaxKind2["SatisfiesExpression"] = 239] = "SatisfiesExpression";
  SyntaxKind2[SyntaxKind2["TemplateSpan"] = 240] = "TemplateSpan";
  SyntaxKind2[SyntaxKind2["SemicolonClassElement"] = 241] = "SemicolonClassElement";
  SyntaxKind2[SyntaxKind2["Block"] = 242] = "Block";
  SyntaxKind2[SyntaxKind2["EmptyStatement"] = 243] = "EmptyStatement";
  SyntaxKind2[SyntaxKind2["VariableStatement"] = 244] = "VariableStatement";
  SyntaxKind2[SyntaxKind2["ExpressionStatement"] = 245] = "ExpressionStatement";
  SyntaxKind2[SyntaxKind2["IfStatement"] = 246] = "IfStatement";
  SyntaxKind2[SyntaxKind2["DoStatement"] = 247] = "DoStatement";
  SyntaxKind2[SyntaxKind2["WhileStatement"] = 248] = "WhileStatement";
  SyntaxKind2[SyntaxKind2["ForStatement"] = 249] = "ForStatement";
  SyntaxKind2[SyntaxKind2["ForInStatement"] = 250] = "ForInStatement";
  SyntaxKind2[SyntaxKind2["ForOfStatement"] = 251] = "ForOfStatement";
  SyntaxKind2[SyntaxKind2["ContinueStatement"] = 252] = "ContinueStatement";
  SyntaxKind2[SyntaxKind2["BreakStatement"] = 253] = "BreakStatement";
  SyntaxKind2[SyntaxKind2["ReturnStatement"] = 254] = "ReturnStatement";
  SyntaxKind2[SyntaxKind2["WithStatement"] = 255] = "WithStatement";
  SyntaxKind2[SyntaxKind2["SwitchStatement"] = 256] = "SwitchStatement";
  SyntaxKind2[SyntaxKind2["LabeledStatement"] = 257] = "LabeledStatement";
  SyntaxKind2[SyntaxKind2["ThrowStatement"] = 258] = "ThrowStatement";
  SyntaxKind2[SyntaxKind2["TryStatement"] = 259] = "TryStatement";
  SyntaxKind2[SyntaxKind2["DebuggerStatement"] = 260] = "DebuggerStatement";
  SyntaxKind2[SyntaxKind2["VariableDeclaration"] = 261] = "VariableDeclaration";
  SyntaxKind2[SyntaxKind2["VariableDeclarationList"] = 262] = "VariableDeclarationList";
  SyntaxKind2[SyntaxKind2["FunctionDeclaration"] = 263] = "FunctionDeclaration";
  SyntaxKind2[SyntaxKind2["ClassDeclaration"] = 264] = "ClassDeclaration";
  SyntaxKind2[SyntaxKind2["InterfaceDeclaration"] = 265] = "InterfaceDeclaration";
  SyntaxKind2[SyntaxKind2["TypeAliasDeclaration"] = 266] = "TypeAliasDeclaration";
  SyntaxKind2[SyntaxKind2["EnumDeclaration"] = 267] = "EnumDeclaration";
  SyntaxKind2[SyntaxKind2["ModuleDeclaration"] = 268] = "ModuleDeclaration";
  SyntaxKind2[SyntaxKind2["ModuleBlock"] = 269] = "ModuleBlock";
  SyntaxKind2[SyntaxKind2["CaseBlock"] = 270] = "CaseBlock";
  SyntaxKind2[SyntaxKind2["NamespaceExportDeclaration"] = 271] = "NamespaceExportDeclaration";
  SyntaxKind2[SyntaxKind2["ImportEqualsDeclaration"] = 272] = "ImportEqualsDeclaration";
  SyntaxKind2[SyntaxKind2["ImportDeclaration"] = 273] = "ImportDeclaration";
  SyntaxKind2[SyntaxKind2["ImportClause"] = 274] = "ImportClause";
  SyntaxKind2[SyntaxKind2["NamespaceImport"] = 275] = "NamespaceImport";
  SyntaxKind2[SyntaxKind2["NamedImports"] = 276] = "NamedImports";
  SyntaxKind2[SyntaxKind2["ImportSpecifier"] = 277] = "ImportSpecifier";
  SyntaxKind2[SyntaxKind2["ExportAssignment"] = 278] = "ExportAssignment";
  SyntaxKind2[SyntaxKind2["ExportDeclaration"] = 279] = "ExportDeclaration";
  SyntaxKind2[SyntaxKind2["NamedExports"] = 280] = "NamedExports";
  SyntaxKind2[SyntaxKind2["NamespaceExport"] = 281] = "NamespaceExport";
  SyntaxKind2[SyntaxKind2["ExportSpecifier"] = 282] = "ExportSpecifier";
  SyntaxKind2[SyntaxKind2["MissingDeclaration"] = 283] = "MissingDeclaration";
  SyntaxKind2[SyntaxKind2["ExternalModuleReference"] = 284] = "ExternalModuleReference";
  SyntaxKind2[SyntaxKind2["JsxElement"] = 285] = "JsxElement";
  SyntaxKind2[SyntaxKind2["JsxSelfClosingElement"] = 286] = "JsxSelfClosingElement";
  SyntaxKind2[SyntaxKind2["JsxOpeningElement"] = 287] = "JsxOpeningElement";
  SyntaxKind2[SyntaxKind2["JsxClosingElement"] = 288] = "JsxClosingElement";
  SyntaxKind2[SyntaxKind2["JsxFragment"] = 289] = "JsxFragment";
  SyntaxKind2[SyntaxKind2["JsxOpeningFragment"] = 290] = "JsxOpeningFragment";
  SyntaxKind2[SyntaxKind2["JsxClosingFragment"] = 291] = "JsxClosingFragment";
  SyntaxKind2[SyntaxKind2["JsxAttribute"] = 292] = "JsxAttribute";
  SyntaxKind2[SyntaxKind2["JsxAttributes"] = 293] = "JsxAttributes";
  SyntaxKind2[SyntaxKind2["JsxSpreadAttribute"] = 294] = "JsxSpreadAttribute";
  SyntaxKind2[SyntaxKind2["JsxExpression"] = 295] = "JsxExpression";
  SyntaxKind2[SyntaxKind2["JsxNamespacedName"] = 296] = "JsxNamespacedName";
  SyntaxKind2[SyntaxKind2["CaseClause"] = 297] = "CaseClause";
  SyntaxKind2[SyntaxKind2["DefaultClause"] = 298] = "DefaultClause";
  SyntaxKind2[SyntaxKind2["HeritageClause"] = 299] = "HeritageClause";
  SyntaxKind2[SyntaxKind2["CatchClause"] = 300] = "CatchClause";
  SyntaxKind2[SyntaxKind2["ImportAttributes"] = 301] = "ImportAttributes";
  SyntaxKind2[SyntaxKind2["ImportAttribute"] = 302] = "ImportAttribute";
  SyntaxKind2[SyntaxKind2["PropertyAssignment"] = 303] = "PropertyAssignment";
  SyntaxKind2[SyntaxKind2["ShorthandPropertyAssignment"] = 304] = "ShorthandPropertyAssignment";
  SyntaxKind2[SyntaxKind2["SpreadAssignment"] = 305] = "SpreadAssignment";
  SyntaxKind2[SyntaxKind2["EnumMember"] = 306] = "EnumMember";
  SyntaxKind2[SyntaxKind2["SourceFile"] = 307] = "SourceFile";
  SyntaxKind2[SyntaxKind2["JSDocTypeExpression"] = 308] = "JSDocTypeExpression";
  SyntaxKind2[SyntaxKind2["JSDocNameReference"] = 309] = "JSDocNameReference";
  SyntaxKind2[SyntaxKind2["JSDocAllType"] = 310] = "JSDocAllType";
  SyntaxKind2[SyntaxKind2["JSDocNullableType"] = 311] = "JSDocNullableType";
  SyntaxKind2[SyntaxKind2["JSDocNonNullableType"] = 312] = "JSDocNonNullableType";
  SyntaxKind2[SyntaxKind2["JSDocOptionalType"] = 313] = "JSDocOptionalType";
  SyntaxKind2[SyntaxKind2["JSDocVariadicType"] = 314] = "JSDocVariadicType";
  SyntaxKind2[SyntaxKind2["JSDoc"] = 315] = "JSDoc";
  SyntaxKind2[SyntaxKind2["JSDocText"] = 316] = "JSDocText";
  SyntaxKind2[SyntaxKind2["JSDocTypeLiteral"] = 317] = "JSDocTypeLiteral";
  SyntaxKind2[SyntaxKind2["JSDocSignature"] = 318] = "JSDocSignature";
  SyntaxKind2[SyntaxKind2["JSDocLink"] = 319] = "JSDocLink";
  SyntaxKind2[SyntaxKind2["JSDocLinkCode"] = 320] = "JSDocLinkCode";
  SyntaxKind2[SyntaxKind2["JSDocLinkPlain"] = 321] = "JSDocLinkPlain";
  SyntaxKind2[SyntaxKind2["JSDocUnknownTag"] = 322] = "JSDocUnknownTag";
  SyntaxKind2[SyntaxKind2["JSDocAugmentsTag"] = 323] = "JSDocAugmentsTag";
  SyntaxKind2[SyntaxKind2["JSDocImplementsTag"] = 324] = "JSDocImplementsTag";
  SyntaxKind2[SyntaxKind2["JSDocDeprecatedTag"] = 325] = "JSDocDeprecatedTag";
  SyntaxKind2[SyntaxKind2["JSDocPublicTag"] = 326] = "JSDocPublicTag";
  SyntaxKind2[SyntaxKind2["JSDocPrivateTag"] = 327] = "JSDocPrivateTag";
  SyntaxKind2[SyntaxKind2["JSDocProtectedTag"] = 328] = "JSDocProtectedTag";
  SyntaxKind2[SyntaxKind2["JSDocReadonlyTag"] = 329] = "JSDocReadonlyTag";
  SyntaxKind2[SyntaxKind2["JSDocOverrideTag"] = 330] = "JSDocOverrideTag";
  SyntaxKind2[SyntaxKind2["JSDocCallbackTag"] = 331] = "JSDocCallbackTag";
  SyntaxKind2[SyntaxKind2["JSDocOverloadTag"] = 332] = "JSDocOverloadTag";
  SyntaxKind2[SyntaxKind2["JSDocParameterTag"] = 333] = "JSDocParameterTag";
  SyntaxKind2[SyntaxKind2["JSDocReturnTag"] = 334] = "JSDocReturnTag";
  SyntaxKind2[SyntaxKind2["JSDocThisTag"] = 335] = "JSDocThisTag";
  SyntaxKind2[SyntaxKind2["JSDocTypeTag"] = 336] = "JSDocTypeTag";
  SyntaxKind2[SyntaxKind2["JSDocTemplateTag"] = 337] = "JSDocTemplateTag";
  SyntaxKind2[SyntaxKind2["JSDocTypedefTag"] = 338] = "JSDocTypedefTag";
  SyntaxKind2[SyntaxKind2["JSDocSeeTag"] = 339] = "JSDocSeeTag";
  SyntaxKind2[SyntaxKind2["JSDocPropertyTag"] = 340] = "JSDocPropertyTag";
  SyntaxKind2[SyntaxKind2["JSDocThrowsTag"] = 341] = "JSDocThrowsTag";
  SyntaxKind2[SyntaxKind2["JSDocSatisfiesTag"] = 342] = "JSDocSatisfiesTag";
  SyntaxKind2[SyntaxKind2["JSDocImportTag"] = 343] = "JSDocImportTag";
  SyntaxKind2[SyntaxKind2["SyntaxList"] = 344] = "SyntaxList";
  SyntaxKind2[SyntaxKind2["JSTypeAliasDeclaration"] = 345] = "JSTypeAliasDeclaration";
  SyntaxKind2[SyntaxKind2["JSImportDeclaration"] = 346] = "JSImportDeclaration";
  SyntaxKind2[SyntaxKind2["NotEmittedStatement"] = 347] = "NotEmittedStatement";
  SyntaxKind2[SyntaxKind2["PartiallyEmittedExpression"] = 348] = "PartiallyEmittedExpression";
  SyntaxKind2[SyntaxKind2["SyntheticReferenceExpression"] = 349] = "SyntheticReferenceExpression";
  SyntaxKind2[SyntaxKind2["NotEmittedTypeElement"] = 350] = "NotEmittedTypeElement";
  SyntaxKind2[SyntaxKind2["Count"] = 351] = "Count";
  SyntaxKind2[SyntaxKind2["FirstAssignment"] = 63] = "FirstAssignment";
  SyntaxKind2[SyntaxKind2["LastAssignment"] = 78] = "LastAssignment";
  SyntaxKind2[SyntaxKind2["FirstCompoundAssignment"] = 64] = "FirstCompoundAssignment";
  SyntaxKind2[SyntaxKind2["LastCompoundAssignment"] = 78] = "LastCompoundAssignment";
  SyntaxKind2[SyntaxKind2["FirstReservedWord"] = 82] = "FirstReservedWord";
  SyntaxKind2[SyntaxKind2["LastReservedWord"] = 117] = "LastReservedWord";
  SyntaxKind2[SyntaxKind2["FirstKeyword"] = 82] = "FirstKeyword";
  SyntaxKind2[SyntaxKind2["LastKeyword"] = 166] = "LastKeyword";
  SyntaxKind2[SyntaxKind2["FirstFutureReservedWord"] = 118] = "FirstFutureReservedWord";
  SyntaxKind2[SyntaxKind2["LastFutureReservedWord"] = 126] = "LastFutureReservedWord";
  SyntaxKind2[SyntaxKind2["FirstTypeNode"] = 183] = "FirstTypeNode";
  SyntaxKind2[SyntaxKind2["LastTypeNode"] = 206] = "LastTypeNode";
  SyntaxKind2[SyntaxKind2["FirstPunctuation"] = 18] = "FirstPunctuation";
  SyntaxKind2[SyntaxKind2["LastPunctuation"] = 78] = "LastPunctuation";
  SyntaxKind2[SyntaxKind2["FirstToken"] = 0] = "FirstToken";
  SyntaxKind2[SyntaxKind2["LastToken"] = 166] = "LastToken";
  SyntaxKind2[SyntaxKind2["FirstLiteralToken"] = 8] = "FirstLiteralToken";
  SyntaxKind2[SyntaxKind2["LastLiteralToken"] = 14] = "LastLiteralToken";
  SyntaxKind2[SyntaxKind2["FirstTemplateToken"] = 14] = "FirstTemplateToken";
  SyntaxKind2[SyntaxKind2["LastTemplateToken"] = 17] = "LastTemplateToken";
  SyntaxKind2[SyntaxKind2["FirstBinaryOperator"] = 29] = "FirstBinaryOperator";
  SyntaxKind2[SyntaxKind2["LastBinaryOperator"] = 78] = "LastBinaryOperator";
  SyntaxKind2[SyntaxKind2["FirstStatement"] = 244] = "FirstStatement";
  SyntaxKind2[SyntaxKind2["LastStatement"] = 260] = "LastStatement";
  SyntaxKind2[SyntaxKind2["FirstNode"] = 167] = "FirstNode";
  SyntaxKind2[SyntaxKind2["FirstJSDocNode"] = 308] = "FirstJSDocNode";
  SyntaxKind2[SyntaxKind2["LastJSDocNode"] = 343] = "LastJSDocNode";
  SyntaxKind2[SyntaxKind2["FirstJSDocTagNode"] = 322] = "FirstJSDocTagNode";
  SyntaxKind2[SyntaxKind2["LastJSDocTagNode"] = 343] = "LastJSDocTagNode";
  SyntaxKind2[SyntaxKind2["FirstContextualKeyword"] = 127] = "FirstContextualKeyword";
  SyntaxKind2[SyntaxKind2["LastContextualKeyword"] = 166] = "LastContextualKeyword";
  SyntaxKind2[SyntaxKind2["LastUnaryOperator"] = 54] = "LastUnaryOperator";
  SyntaxKind2[SyntaxKind2["FirstTriviaToken"] = 2] = "FirstTriviaToken";
  SyntaxKind2[SyntaxKind2["LastTriviaToken"] = 6] = "LastTriviaToken";
})(SyntaxKind || (SyntaxKind = {}));

// node_modules/typescript/dist/enums/tokenFlags.js
var TokenFlags;
(function(TokenFlags2) {
  TokenFlags2[TokenFlags2["None"] = 0] = "None";
  TokenFlags2[TokenFlags2["PrecedingLineBreak"] = 1] = "PrecedingLineBreak";
  TokenFlags2[TokenFlags2["PrecedingJSDocComment"] = 2] = "PrecedingJSDocComment";
  TokenFlags2[TokenFlags2["Unterminated"] = 4] = "Unterminated";
  TokenFlags2[TokenFlags2["ExtendedUnicodeEscape"] = 8] = "ExtendedUnicodeEscape";
  TokenFlags2[TokenFlags2["Scientific"] = 16] = "Scientific";
  TokenFlags2[TokenFlags2["Octal"] = 32] = "Octal";
  TokenFlags2[TokenFlags2["HexSpecifier"] = 64] = "HexSpecifier";
  TokenFlags2[TokenFlags2["BinarySpecifier"] = 128] = "BinarySpecifier";
  TokenFlags2[TokenFlags2["OctalSpecifier"] = 256] = "OctalSpecifier";
  TokenFlags2[TokenFlags2["ContainsSeparator"] = 512] = "ContainsSeparator";
  TokenFlags2[TokenFlags2["UnicodeEscape"] = 1024] = "UnicodeEscape";
  TokenFlags2[TokenFlags2["ContainsInvalidEscape"] = 2048] = "ContainsInvalidEscape";
  TokenFlags2[TokenFlags2["HexEscape"] = 4096] = "HexEscape";
  TokenFlags2[TokenFlags2["ContainsLeadingZero"] = 8192] = "ContainsLeadingZero";
  TokenFlags2[TokenFlags2["ContainsInvalidSeparator"] = 16384] = "ContainsInvalidSeparator";
  TokenFlags2[TokenFlags2["PrecedingJSDocLeadingAsterisks"] = 32768] = "PrecedingJSDocLeadingAsterisks";
  TokenFlags2[TokenFlags2["SingleQuote"] = 65536] = "SingleQuote";
  TokenFlags2[TokenFlags2["PrecedingJSDocWithDeprecated"] = 131072] = "PrecedingJSDocWithDeprecated";
  TokenFlags2[TokenFlags2["PrecedingJSDocWithSeeOrLink"] = 262144] = "PrecedingJSDocWithSeeOrLink";
  TokenFlags2[TokenFlags2["BinaryOrOctalSpecifier"] = 384] = "BinaryOrOctalSpecifier";
  TokenFlags2[TokenFlags2["WithSpecifier"] = 448] = "WithSpecifier";
  TokenFlags2[TokenFlags2["StringLiteralFlags"] = 72716] = "StringLiteralFlags";
  TokenFlags2[TokenFlags2["NumericLiteralFlags"] = 25584] = "NumericLiteralFlags";
  TokenFlags2[TokenFlags2["TemplateLiteralLikeFlags"] = 7180] = "TemplateLiteralLikeFlags";
  TokenFlags2[TokenFlags2["RegularExpressionLiteralFlags"] = 4] = "RegularExpressionLiteralFlags";
  TokenFlags2[TokenFlags2["IsInvalid"] = 26656] = "IsInvalid";
})(TokenFlags || (TokenFlags = {}));

// node_modules/typescript/dist/ast/utils.js
var syntaxKindNames;
function getSyntaxKindNames() {
  if (!syntaxKindNames) {
    syntaxKindNames = /* @__PURE__ */ new Map();
    for (const name of Object.keys(SyntaxKind)) {
      const val = SyntaxKind[name];
      if (typeof val === "number" && !syntaxKindNames.has(val)) {
        syntaxKindNames.set(val, name);
      }
    }
    syntaxKindNames.set(SyntaxKind.EndOfFile, "EndOfFileToken");
  }
  return syntaxKindNames;
}
function formatSyntaxKind(kind) {
  return getSyntaxKindNames().get(kind) ?? `Unknown(${kind})`;
}
function unescapeLeadingUnderscores(identifier) {
  const id = identifier;
  return id.length >= 3 && id.charCodeAt(0) === CharacterCodes._ && id.charCodeAt(1) === CharacterCodes._ && id.charCodeAt(2) === CharacterCodes._ ? id.slice(1) : id;
}
function escapeLeadingUnderscores(identifier) {
  return identifier.length >= 2 && identifier.charCodeAt(0) === CharacterCodes._ && identifier.charCodeAt(1) === CharacterCodes._ ? "_" + identifier : identifier;
}
function tryCast(value, test) {
  return value !== void 0 && test(value) ? value : void 0;
}
function cast(value, test) {
  if (value !== void 0 && test(value))
    return value;
  throw new Error(`Invalid cast. The supplied value ${value} did not pass the test '${test.name}'.`);
}
function cloneSourceFileData(sourceFile) {
  return {
    statements: sourceFile.statements,
    endOfFileToken: sourceFile.endOfFileToken,
    text: sourceFile.text,
    fileName: sourceFile.fileName,
    path: sourceFile.path,
    languageVariant: sourceFile.languageVariant,
    scriptKind: sourceFile.scriptKind,
    isDeclarationFile: sourceFile.isDeclarationFile,
    referencedFiles: sourceFile.referencedFiles,
    typeReferenceDirectives: sourceFile.typeReferenceDirectives,
    libReferenceDirectives: sourceFile.libReferenceDirectives,
    imports: sourceFile.imports,
    moduleAugmentations: sourceFile.moduleAugmentations,
    ambientModuleNames: sourceFile.ambientModuleNames,
    externalModuleIndicator: sourceFile.externalModuleIndicator,
    tokenCache: void 0
  };
}

// node_modules/typescript/dist/enums/outerExpressionKinds.js
var OuterExpressionKinds;
(function(OuterExpressionKinds2) {
  OuterExpressionKinds2[OuterExpressionKinds2["Parentheses"] = 1] = "Parentheses";
  OuterExpressionKinds2[OuterExpressionKinds2["TypeAssertions"] = 2] = "TypeAssertions";
  OuterExpressionKinds2[OuterExpressionKinds2["NonNullAssertions"] = 4] = "NonNullAssertions";
  OuterExpressionKinds2[OuterExpressionKinds2["PartiallyEmittedExpressions"] = 8] = "PartiallyEmittedExpressions";
  OuterExpressionKinds2[OuterExpressionKinds2["ExpressionsWithTypeArguments"] = 16] = "ExpressionsWithTypeArguments";
  OuterExpressionKinds2[OuterExpressionKinds2["Satisfies"] = 32] = "Satisfies";
  OuterExpressionKinds2[OuterExpressionKinds2["ExcludeJSDocTypeAssertion"] = 64] = "ExcludeJSDocTypeAssertion";
  OuterExpressionKinds2[OuterExpressionKinds2["Assignments"] = 128] = "Assignments";
  OuterExpressionKinds2[OuterExpressionKinds2["Comma"] = 256] = "Comma";
  OuterExpressionKinds2[OuterExpressionKinds2["Assertions"] = 38] = "Assertions";
  OuterExpressionKinds2[OuterExpressionKinds2["All"] = 63] = "All";
  OuterExpressionKinds2[OuterExpressionKinds2["AllExceptAssertionsOrExpressionsWithTypeArguments"] = 9] = "AllExceptAssertionsOrExpressionsWithTypeArguments";
  OuterExpressionKinds2[OuterExpressionKinds2["ExpressionTypePassthrough"] = 385] = "ExpressionTypePassthrough";
})(OuterExpressionKinds || (OuterExpressionKinds = {}));

// node_modules/typescript/dist/ast/is.generated.js
function isToken(node) {
  return isTokenKind(node.kind);
}
function isIdentifier(node) {
  return node.kind === SyntaxKind.Identifier;
}
function isPrivateIdentifier(node) {
  return node.kind === SyntaxKind.PrivateIdentifier;
}
function isQualifiedName(node) {
  return node.kind === SyntaxKind.QualifiedName;
}
function isComputedPropertyName(node) {
  return node.kind === SyntaxKind.ComputedPropertyName;
}
function isDecorator(node) {
  return node.kind === SyntaxKind.Decorator;
}
function isEmptyStatement(node) {
  return node.kind === SyntaxKind.EmptyStatement;
}
function isIfStatement(node) {
  return node.kind === SyntaxKind.IfStatement;
}
function isDoStatement(node) {
  return node.kind === SyntaxKind.DoStatement;
}
function isWhileStatement(node) {
  return node.kind === SyntaxKind.WhileStatement;
}
function isForStatement(node) {
  return node.kind === SyntaxKind.ForStatement;
}
function isBreakStatement(node) {
  return node.kind === SyntaxKind.BreakStatement;
}
function isContinueStatement(node) {
  return node.kind === SyntaxKind.ContinueStatement;
}
function isReturnStatement(node) {
  return node.kind === SyntaxKind.ReturnStatement;
}
function isWithStatement(node) {
  return node.kind === SyntaxKind.WithStatement;
}
function isSwitchStatement(node) {
  return node.kind === SyntaxKind.SwitchStatement;
}
function isCaseBlock(node) {
  return node.kind === SyntaxKind.CaseBlock;
}
function isThrowStatement(node) {
  return node.kind === SyntaxKind.ThrowStatement;
}
function isTryStatement(node) {
  return node.kind === SyntaxKind.TryStatement;
}
function isCatchClause(node) {
  return node.kind === SyntaxKind.CatchClause;
}
function isDebuggerStatement(node) {
  return node.kind === SyntaxKind.DebuggerStatement;
}
function isLabeledStatement(node) {
  return node.kind === SyntaxKind.LabeledStatement;
}
function isExpressionStatement(node) {
  return node.kind === SyntaxKind.ExpressionStatement;
}
function isBlock(node) {
  return node.kind === SyntaxKind.Block;
}
function isVariableStatement(node) {
  return node.kind === SyntaxKind.VariableStatement;
}
function isVariableDeclaration(node) {
  return node.kind === SyntaxKind.VariableDeclaration;
}
function isVariableDeclarationList(node) {
  return node.kind === SyntaxKind.VariableDeclarationList;
}
function isParameterDeclaration(node) {
  return node.kind === SyntaxKind.Parameter;
}
function isBindingElement(node) {
  return node.kind === SyntaxKind.BindingElement;
}
function isMissingDeclaration(node) {
  return node.kind === SyntaxKind.MissingDeclaration;
}
function isFunctionDeclaration(node) {
  return node.kind === SyntaxKind.FunctionDeclaration;
}
function isClassDeclaration(node) {
  return node.kind === SyntaxKind.ClassDeclaration;
}
function isClassExpression(node) {
  return node.kind === SyntaxKind.ClassExpression;
}
function isHeritageClause(node) {
  return node.kind === SyntaxKind.HeritageClause;
}
function isInterfaceDeclaration(node) {
  return node.kind === SyntaxKind.InterfaceDeclaration;
}
function isTypeAliasDeclaration(node) {
  switch (node.kind) {
    case SyntaxKind.TypeAliasDeclaration:
    case SyntaxKind.JSTypeAliasDeclaration:
      return true;
    default:
      return false;
  }
}
function isEnumMember(node) {
  return node.kind === SyntaxKind.EnumMember;
}
function isEnumDeclaration(node) {
  return node.kind === SyntaxKind.EnumDeclaration;
}
function isModuleBlock(node) {
  return node.kind === SyntaxKind.ModuleBlock;
}
function isNotEmittedStatement(node) {
  return node.kind === SyntaxKind.NotEmittedStatement;
}
function isNotEmittedTypeElement(node) {
  return node.kind === SyntaxKind.NotEmittedTypeElement;
}
function isImportDeclaration(node) {
  switch (node.kind) {
    case SyntaxKind.ImportDeclaration:
    case SyntaxKind.JSImportDeclaration:
      return true;
    default:
      return false;
  }
}
function isExternalModuleReference(node) {
  return node.kind === SyntaxKind.ExternalModuleReference;
}
function isNamespaceImport(node) {
  return node.kind === SyntaxKind.NamespaceImport;
}
function isNamedImports(node) {
  return node.kind === SyntaxKind.NamedImports;
}
function isExportAssignment(node) {
  return node.kind === SyntaxKind.ExportAssignment;
}
function isNamespaceExportDeclaration(node) {
  return node.kind === SyntaxKind.NamespaceExportDeclaration;
}
function isNamespaceExport(node) {
  return node.kind === SyntaxKind.NamespaceExport;
}
function isNamedExports(node) {
  return node.kind === SyntaxKind.NamedExports;
}
function isExportSpecifier(node) {
  return node.kind === SyntaxKind.ExportSpecifier;
}
function isCallSignatureDeclaration(node) {
  return node.kind === SyntaxKind.CallSignature;
}
function isConstructSignatureDeclaration(node) {
  return node.kind === SyntaxKind.ConstructSignature;
}
function isConstructorDeclaration(node) {
  return node.kind === SyntaxKind.Constructor;
}
function isGetAccessorDeclaration(node) {
  return node.kind === SyntaxKind.GetAccessor;
}
function isSetAccessorDeclaration(node) {
  return node.kind === SyntaxKind.SetAccessor;
}
function isIndexSignatureDeclaration(node) {
  return node.kind === SyntaxKind.IndexSignature;
}
function isMethodSignatureDeclaration(node) {
  return node.kind === SyntaxKind.MethodSignature;
}
function isMethodDeclaration(node) {
  return node.kind === SyntaxKind.MethodDeclaration;
}
function isPropertySignatureDeclaration(node) {
  return node.kind === SyntaxKind.PropertySignature;
}
function isPropertyDeclaration(node) {
  return node.kind === SyntaxKind.PropertyDeclaration;
}
function isSemicolonClassElement(node) {
  return node.kind === SyntaxKind.SemicolonClassElement;
}
function isClassStaticBlockDeclaration(node) {
  return node.kind === SyntaxKind.ClassStaticBlockDeclaration;
}
function isOmittedExpression(node) {
  return node.kind === SyntaxKind.OmittedExpression;
}
function isKeywordExpression(node) {
  return isKeywordExpressionKind(node.kind);
}
function isStringLiteral(node) {
  return node.kind === SyntaxKind.StringLiteral;
}
function isNumericLiteral(node) {
  return node.kind === SyntaxKind.NumericLiteral;
}
function isBigIntLiteral(node) {
  return node.kind === SyntaxKind.BigIntLiteral;
}
function isRegularExpressionLiteral(node) {
  return node.kind === SyntaxKind.RegularExpressionLiteral;
}
function isNoSubstitutionTemplateLiteral(node) {
  return node.kind === SyntaxKind.NoSubstitutionTemplateLiteral;
}
function isBinaryExpression(node) {
  return node.kind === SyntaxKind.BinaryExpression;
}
function isPrefixUnaryExpression(node) {
  return node.kind === SyntaxKind.PrefixUnaryExpression;
}
function isPostfixUnaryExpression(node) {
  return node.kind === SyntaxKind.PostfixUnaryExpression;
}
function isYieldExpression(node) {
  return node.kind === SyntaxKind.YieldExpression;
}
function isArrowFunction(node) {
  return node.kind === SyntaxKind.ArrowFunction;
}
function isFunctionExpression(node) {
  return node.kind === SyntaxKind.FunctionExpression;
}
function isAsExpression(node) {
  return node.kind === SyntaxKind.AsExpression;
}
function isSatisfiesExpression(node) {
  return node.kind === SyntaxKind.SatisfiesExpression;
}
function isConditionalExpression(node) {
  return node.kind === SyntaxKind.ConditionalExpression;
}
function isPropertyAccessExpression(node) {
  return node.kind === SyntaxKind.PropertyAccessExpression;
}
function isElementAccessExpression(node) {
  return node.kind === SyntaxKind.ElementAccessExpression;
}
function isCallExpression(node) {
  return node.kind === SyntaxKind.CallExpression;
}
function isNewExpression(node) {
  return node.kind === SyntaxKind.NewExpression;
}
function isMetaProperty(node) {
  return node.kind === SyntaxKind.MetaProperty;
}
function isNonNullExpression(node) {
  return node.kind === SyntaxKind.NonNullExpression;
}
function isSpreadElement(node) {
  return node.kind === SyntaxKind.SpreadElement;
}
function isTemplateExpression(node) {
  return node.kind === SyntaxKind.TemplateExpression;
}
function isTemplateSpan(node) {
  return node.kind === SyntaxKind.TemplateSpan;
}
function isTaggedTemplateExpression(node) {
  return node.kind === SyntaxKind.TaggedTemplateExpression;
}
function isParenthesizedExpression(node) {
  return node.kind === SyntaxKind.ParenthesizedExpression;
}
function isArrayLiteralExpression(node) {
  return node.kind === SyntaxKind.ArrayLiteralExpression;
}
function isObjectLiteralExpression(node) {
  return node.kind === SyntaxKind.ObjectLiteralExpression;
}
function isSpreadAssignment(node) {
  return node.kind === SyntaxKind.SpreadAssignment;
}
function isPropertyAssignment(node) {
  return node.kind === SyntaxKind.PropertyAssignment;
}
function isShorthandPropertyAssignment(node) {
  return node.kind === SyntaxKind.ShorthandPropertyAssignment;
}
function isDeleteExpression(node) {
  return node.kind === SyntaxKind.DeleteExpression;
}
function isTypeOfExpression(node) {
  return node.kind === SyntaxKind.TypeOfExpression;
}
function isVoidExpression(node) {
  return node.kind === SyntaxKind.VoidExpression;
}
function isAwaitExpression(node) {
  return node.kind === SyntaxKind.AwaitExpression;
}
function isTypeAssertion(node) {
  return node.kind === SyntaxKind.TypeAssertionExpression;
}
function isKeywordTypeNode(node) {
  return isKeywordTypeKind(node.kind);
}
function isUnionTypeNode(node) {
  return node.kind === SyntaxKind.UnionType;
}
function isIntersectionTypeNode(node) {
  return node.kind === SyntaxKind.IntersectionType;
}
function isConditionalTypeNode(node) {
  return node.kind === SyntaxKind.ConditionalType;
}
function isTypeOperatorNode(node) {
  return node.kind === SyntaxKind.TypeOperator;
}
function isInferTypeNode(node) {
  return node.kind === SyntaxKind.InferType;
}
function isArrayTypeNode(node) {
  return node.kind === SyntaxKind.ArrayType;
}
function isIndexedAccessTypeNode(node) {
  return node.kind === SyntaxKind.IndexedAccessType;
}
function isTypeReferenceNode(node) {
  return node.kind === SyntaxKind.TypeReference;
}
function isExpressionWithTypeArguments(node) {
  return node.kind === SyntaxKind.ExpressionWithTypeArguments;
}
function isLiteralTypeNode(node) {
  return node.kind === SyntaxKind.LiteralType;
}
function isThisTypeNode(node) {
  return node.kind === SyntaxKind.ThisType;
}
function isTypePredicateNode(node) {
  return node.kind === SyntaxKind.TypePredicate;
}
function isImportAttribute(node) {
  return node.kind === SyntaxKind.ImportAttribute;
}
function isImportAttributes(node) {
  return node.kind === SyntaxKind.ImportAttributes;
}
function isTypeQueryNode(node) {
  return node.kind === SyntaxKind.TypeQuery;
}
function isMappedTypeNode(node) {
  return node.kind === SyntaxKind.MappedType;
}
function isTypeLiteralNode(node) {
  return node.kind === SyntaxKind.TypeLiteral;
}
function isTupleTypeNode(node) {
  return node.kind === SyntaxKind.TupleType;
}
function isNamedTupleMember(node) {
  return node.kind === SyntaxKind.NamedTupleMember;
}
function isOptionalTypeNode(node) {
  return node.kind === SyntaxKind.OptionalType;
}
function isRestTypeNode(node) {
  return node.kind === SyntaxKind.RestType;
}
function isParenthesizedTypeNode(node) {
  return node.kind === SyntaxKind.ParenthesizedType;
}
function isFunctionTypeNode(node) {
  return node.kind === SyntaxKind.FunctionType;
}
function isConstructorTypeNode(node) {
  return node.kind === SyntaxKind.ConstructorType;
}
function isTemplateHead(node) {
  return node.kind === SyntaxKind.TemplateHead;
}
function isTemplateMiddle(node) {
  return node.kind === SyntaxKind.TemplateMiddle;
}
function isTemplateTail(node) {
  return node.kind === SyntaxKind.TemplateTail;
}
function isTemplateLiteralTypeNode(node) {
  return node.kind === SyntaxKind.TemplateLiteralType;
}
function isTemplateLiteralTypeSpan(node) {
  return node.kind === SyntaxKind.TemplateLiteralTypeSpan;
}
function isSyntheticExpression(node) {
  return node.kind === SyntaxKind.SyntheticExpression;
}
function isPartiallyEmittedExpression(node) {
  return node.kind === SyntaxKind.PartiallyEmittedExpression;
}
function isJsxElement(node) {
  return node.kind === SyntaxKind.JsxElement;
}
function isJsxAttributes(node) {
  return node.kind === SyntaxKind.JsxAttributes;
}
function isJsxNamespacedName(node) {
  return node.kind === SyntaxKind.JsxNamespacedName;
}
function isJsxOpeningElement(node) {
  return node.kind === SyntaxKind.JsxOpeningElement;
}
function isJsxSelfClosingElement(node) {
  return node.kind === SyntaxKind.JsxSelfClosingElement;
}
function isJsxFragment(node) {
  return node.kind === SyntaxKind.JsxFragment;
}
function isJsxOpeningFragment(node) {
  return node.kind === SyntaxKind.JsxOpeningFragment;
}
function isJsxClosingFragment(node) {
  return node.kind === SyntaxKind.JsxClosingFragment;
}
function isJsxAttribute(node) {
  return node.kind === SyntaxKind.JsxAttribute;
}
function isJsxSpreadAttribute(node) {
  return node.kind === SyntaxKind.JsxSpreadAttribute;
}
function isJsxClosingElement(node) {
  return node.kind === SyntaxKind.JsxClosingElement;
}
function isJsxExpression(node) {
  return node.kind === SyntaxKind.JsxExpression;
}
function isJsxText(node) {
  return node.kind === SyntaxKind.JsxText;
}
function isSyntaxList(node) {
  return node.kind === SyntaxKind.SyntaxList;
}
function isJSDoc(node) {
  return node.kind === SyntaxKind.JSDoc;
}
function isJSDocTypeExpression(node) {
  return node.kind === SyntaxKind.JSDocTypeExpression;
}
function isJSDocNonNullableType(node) {
  return node.kind === SyntaxKind.JSDocNonNullableType;
}
function isJSDocNullableType(node) {
  return node.kind === SyntaxKind.JSDocNullableType;
}
function isJSDocAllType(node) {
  return node.kind === SyntaxKind.JSDocAllType;
}
function isJSDocVariadicType(node) {
  return node.kind === SyntaxKind.JSDocVariadicType;
}
function isJSDocOptionalType(node) {
  return node.kind === SyntaxKind.JSDocOptionalType;
}
function isJSDocTypeTag(node) {
  return node.kind === SyntaxKind.JSDocTypeTag;
}
function isJSDocUnknownTag(node) {
  return node.kind === SyntaxKind.JSDocUnknownTag;
}
function isJSDocTemplateTag(node) {
  return node.kind === SyntaxKind.JSDocTemplateTag;
}
function isJSDocReturnTag(node) {
  return node.kind === SyntaxKind.JSDocReturnTag;
}
function isJSDocPublicTag(node) {
  return node.kind === SyntaxKind.JSDocPublicTag;
}
function isJSDocPrivateTag(node) {
  return node.kind === SyntaxKind.JSDocPrivateTag;
}
function isJSDocProtectedTag(node) {
  return node.kind === SyntaxKind.JSDocProtectedTag;
}
function isJSDocReadonlyTag(node) {
  return node.kind === SyntaxKind.JSDocReadonlyTag;
}
function isJSDocOverrideTag(node) {
  return node.kind === SyntaxKind.JSDocOverrideTag;
}
function isJSDocDeprecatedTag(node) {
  return node.kind === SyntaxKind.JSDocDeprecatedTag;
}
function isJSDocSeeTag(node) {
  return node.kind === SyntaxKind.JSDocSeeTag;
}
function isJSDocImplementsTag(node) {
  return node.kind === SyntaxKind.JSDocImplementsTag;
}
function isJSDocAugmentsTag(node) {
  return node.kind === SyntaxKind.JSDocAugmentsTag;
}
function isJSDocSatisfiesTag(node) {
  return node.kind === SyntaxKind.JSDocSatisfiesTag;
}
function isJSDocThrowsTag(node) {
  return node.kind === SyntaxKind.JSDocThrowsTag;
}
function isJSDocThisTag(node) {
  return node.kind === SyntaxKind.JSDocThisTag;
}
function isJSDocImportTag(node) {
  return node.kind === SyntaxKind.JSDocImportTag;
}
function isJSDocCallbackTag(node) {
  return node.kind === SyntaxKind.JSDocCallbackTag;
}
function isJSDocOverloadTag(node) {
  return node.kind === SyntaxKind.JSDocOverloadTag;
}
function isJSDocTypedefTag(node) {
  return node.kind === SyntaxKind.JSDocTypedefTag;
}
function isJSDocSignature(node) {
  return node.kind === SyntaxKind.JSDocSignature;
}
function isJSDocNameReference(node) {
  return node.kind === SyntaxKind.JSDocNameReference;
}
function isSourceFile(node) {
  return node.kind === SyntaxKind.SourceFile;
}
function isModuleDeclaration(node) {
  return node.kind === SyntaxKind.ModuleDeclaration;
}
function isImportEqualsDeclaration(node) {
  return node.kind === SyntaxKind.ImportEqualsDeclaration;
}
function isExportDeclaration(node) {
  return node.kind === SyntaxKind.ExportDeclaration;
}
function isImportTypeNode(node) {
  return node.kind === SyntaxKind.ImportType;
}
function isImportClause(node) {
  return node.kind === SyntaxKind.ImportClause;
}
function isImportSpecifier(node) {
  return node.kind === SyntaxKind.ImportSpecifier;
}
function isJSDocText(node) {
  return node.kind === SyntaxKind.JSDocText;
}
function isJSDocLink(node) {
  return node.kind === SyntaxKind.JSDocLink;
}
function isJSDocLinkPlain(node) {
  return node.kind === SyntaxKind.JSDocLinkPlain;
}
function isJSDocLinkCode(node) {
  return node.kind === SyntaxKind.JSDocLinkCode;
}
function isTypeParameterDeclaration(node) {
  return node.kind === SyntaxKind.TypeParameter;
}
function isSyntheticReferenceExpression(node) {
  return node.kind === SyntaxKind.SyntheticReferenceExpression;
}
function isJSDocTypeLiteral(node) {
  return node.kind === SyntaxKind.JSDocTypeLiteral;
}
function isForInStatement(node) {
  return node.kind === SyntaxKind.ForInStatement;
}
function isForOfStatement(node) {
  return node.kind === SyntaxKind.ForOfStatement;
}
function isCaseClause(node) {
  return node.kind === SyntaxKind.CaseClause;
}
function isDefaultClause(node) {
  return node.kind === SyntaxKind.DefaultClause;
}
function isObjectBindingPattern(node) {
  return node.kind === SyntaxKind.ObjectBindingPattern;
}
function isArrayBindingPattern(node) {
  return node.kind === SyntaxKind.ArrayBindingPattern;
}
function isJSDocParameterTag(node) {
  return node.kind === SyntaxKind.JSDocParameterTag;
}
function isJSDocPropertyTag(node) {
  return node.kind === SyntaxKind.JSDocPropertyTag;
}
function isAccessExpression(node) {
  return node.kind === SyntaxKind.PropertyAccessExpression || node.kind === SyntaxKind.ElementAccessExpression;
}
function isDeclarationName(node) {
  const kind = node.kind;
  return kind === SyntaxKind.Identifier || kind === SyntaxKind.PrivateIdentifier || kind === SyntaxKind.StringLiteral || kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.BigIntLiteral || kind === SyntaxKind.NoSubstitutionTemplateLiteral || kind === SyntaxKind.ComputedPropertyName || kind === SyntaxKind.ObjectBindingPattern || kind === SyntaxKind.ArrayBindingPattern || kind === SyntaxKind.ElementAccessExpression;
}
function isModuleName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.StringLiteral;
}
function isModuleExportName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.StringLiteral;
}
function isPropertyName(node) {
  const kind = node.kind;
  return kind === SyntaxKind.Identifier || kind === SyntaxKind.StringLiteral || kind === SyntaxKind.NoSubstitutionTemplateLiteral || kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.ComputedPropertyName || kind === SyntaxKind.PrivateIdentifier || kind === SyntaxKind.BigIntLiteral;
}
function isModuleBody(node) {
  return node.kind === SyntaxKind.ModuleBlock || node.kind === SyntaxKind.ModuleDeclaration;
}
function isJSDocFullName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.ModuleDeclaration;
}
function isModuleReference(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.QualifiedName || node.kind === SyntaxKind.ExternalModuleReference;
}
function isNamedImportBindings(node) {
  return node.kind === SyntaxKind.NamespaceImport || node.kind === SyntaxKind.NamedImports;
}
function isNamedExportBindings(node) {
  return node.kind === SyntaxKind.NamespaceExport || node.kind === SyntaxKind.NamedExports;
}
function isMemberName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.PrivateIdentifier;
}
function isEntityName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.QualifiedName;
}
function isBindingName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.ObjectBindingPattern || node.kind === SyntaxKind.ArrayBindingPattern;
}
function isModifierLike(node) {
  const kind = node.kind;
  return kind === SyntaxKind.AbstractKeyword || kind === SyntaxKind.AccessorKeyword || kind === SyntaxKind.AsyncKeyword || kind === SyntaxKind.ConstKeyword || kind === SyntaxKind.DeclareKeyword || kind === SyntaxKind.DefaultKeyword || kind === SyntaxKind.ExportKeyword || kind === SyntaxKind.InKeyword || kind === SyntaxKind.PrivateKeyword || kind === SyntaxKind.ProtectedKeyword || kind === SyntaxKind.PublicKeyword || kind === SyntaxKind.ReadonlyKeyword || kind === SyntaxKind.OutKeyword || kind === SyntaxKind.OverrideKeyword || kind === SyntaxKind.StaticKeyword || kind === SyntaxKind.Decorator;
}
function isJsxChild(node) {
  const kind = node.kind;
  return kind === SyntaxKind.JsxText || kind === SyntaxKind.JsxExpression || kind === SyntaxKind.JsxElement || kind === SyntaxKind.JsxSelfClosingElement || kind === SyntaxKind.JsxFragment;
}
function isJsxAttributeLike(node) {
  return node.kind === SyntaxKind.JsxAttribute || node.kind === SyntaxKind.JsxSpreadAttribute;
}
function isJsxAttributeName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.JsxNamespacedName;
}
function isJsxAttributeValue(node) {
  const kind = node.kind;
  return kind === SyntaxKind.StringLiteral || kind === SyntaxKind.JsxExpression || kind === SyntaxKind.JsxElement || kind === SyntaxKind.JsxSelfClosingElement || kind === SyntaxKind.JsxFragment;
}
function isClassLikeDeclaration(node) {
  return node.kind === SyntaxKind.ClassDeclaration || node.kind === SyntaxKind.ClassExpression;
}
function isAccessorDeclaration(node) {
  return node.kind === SyntaxKind.GetAccessor || node.kind === SyntaxKind.SetAccessor;
}
function isLiteralLikeNode(node) {
  const kind = node.kind;
  return kind === SyntaxKind.StringLiteral || kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.BigIntLiteral || kind === SyntaxKind.RegularExpressionLiteral || kind === SyntaxKind.TemplateHead || kind === SyntaxKind.TemplateMiddle || kind === SyntaxKind.TemplateTail || kind === SyntaxKind.JsxText;
}
function isLiteralExpression(node) {
  const kind = node.kind;
  return kind === SyntaxKind.StringLiteral || kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.BigIntLiteral || kind === SyntaxKind.RegularExpressionLiteral || kind === SyntaxKind.NoSubstitutionTemplateLiteral;
}
function isUnionOrIntersectionTypeNode(node) {
  return node.kind === SyntaxKind.UnionType || node.kind === SyntaxKind.IntersectionType;
}
function isTemplateLiteralLikeNode(node) {
  const kind = node.kind;
  return isPseudoLiteralKind(kind);
}
function isTemplateMiddleOrTail(node) {
  return node.kind === SyntaxKind.TemplateMiddle || node.kind === SyntaxKind.TemplateTail;
}
function isTemplateLiteral(node) {
  return node.kind === SyntaxKind.TemplateExpression || node.kind === SyntaxKind.NoSubstitutionTemplateLiteral;
}
function isTypePredicateParameterName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.ThisType;
}
function isImportAttributeName(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.StringLiteral;
}
function isJSDocComment(node) {
  const kind = node.kind;
  return kind === SyntaxKind.JSDocText || kind === SyntaxKind.JSDocLink || kind === SyntaxKind.JSDocLinkCode || kind === SyntaxKind.JSDocLinkPlain;
}
function isSignatureDeclaration(node) {
  const kind = node.kind;
  return kind === SyntaxKind.CallSignature || kind === SyntaxKind.ConstructSignature || kind === SyntaxKind.MethodSignature || kind === SyntaxKind.IndexSignature || kind === SyntaxKind.FunctionType || kind === SyntaxKind.ConstructorType || kind === SyntaxKind.FunctionDeclaration || kind === SyntaxKind.MethodDeclaration || kind === SyntaxKind.Constructor || kind === SyntaxKind.GetAccessor || kind === SyntaxKind.SetAccessor || kind === SyntaxKind.FunctionExpression || kind === SyntaxKind.ArrowFunction;
}
function isStringLiteralLikeNode(node) {
  return node.kind === SyntaxKind.StringLiteral || node.kind === SyntaxKind.NoSubstitutionTemplateLiteral;
}
function isNumericOrStringLikeLiteral(node) {
  return node.kind === SyntaxKind.StringLiteral || node.kind === SyntaxKind.NoSubstitutionTemplateLiteral || node.kind === SyntaxKind.NumericLiteral;
}
function isObjectLiteralLikeNode(node) {
  return node.kind === SyntaxKind.ObjectLiteralExpression || node.kind === SyntaxKind.ObjectBindingPattern;
}
function isObjectTypeDeclaration(node) {
  const kind = node.kind;
  return kind === SyntaxKind.ClassDeclaration || kind === SyntaxKind.ClassExpression || kind === SyntaxKind.InterfaceDeclaration || kind === SyntaxKind.TypeLiteral;
}
function isJsxOpeningLikeElement(node) {
  return node.kind === SyntaxKind.JsxOpeningElement || node.kind === SyntaxKind.JsxSelfClosingElement;
}
function isNamedImportsOrExports(node) {
  return node.kind === SyntaxKind.NamedImports || node.kind === SyntaxKind.NamedExports;
}
function isBreakOrContinueStatement(node) {
  return node.kind === SyntaxKind.BreakStatement || node.kind === SyntaxKind.ContinueStatement;
}
function isCallLikeExpression(node) {
  const kind = node.kind;
  return kind === SyntaxKind.CallExpression || kind === SyntaxKind.NewExpression || kind === SyntaxKind.TaggedTemplateExpression || kind === SyntaxKind.Decorator || kind === SyntaxKind.JsxOpeningElement || kind === SyntaxKind.JsxSelfClosingElement || kind === SyntaxKind.BinaryExpression;
}
function isFunctionLikeDeclaration(node) {
  const kind = node.kind;
  return kind === SyntaxKind.FunctionDeclaration || kind === SyntaxKind.MethodDeclaration || kind === SyntaxKind.GetAccessor || kind === SyntaxKind.SetAccessor || kind === SyntaxKind.Constructor || kind === SyntaxKind.FunctionExpression || kind === SyntaxKind.ArrowFunction;
}
function isVariableOrParameterDeclaration(node) {
  return node.kind === SyntaxKind.VariableDeclaration || node.kind === SyntaxKind.Parameter;
}
function isVariableOrPropertyDeclaration(node) {
  return node.kind === SyntaxKind.VariableDeclaration || node.kind === SyntaxKind.PropertyDeclaration;
}
function isCallOrNewExpression(node) {
  return node.kind === SyntaxKind.CallExpression || node.kind === SyntaxKind.NewExpression;
}
function isImportClauseOrBindingPattern(node) {
  return node.kind === SyntaxKind.ImportClause || node.kind === SyntaxKind.ObjectBindingPattern || node.kind === SyntaxKind.ArrayBindingPattern;
}
function isAnyImportSyntax(node) {
  return node.kind === SyntaxKind.ImportDeclaration || node.kind === SyntaxKind.JSImportDeclaration || node.kind === SyntaxKind.ImportEqualsDeclaration;
}
function isArrayBindingElement(node) {
  return node.kind === SyntaxKind.BindingElement || node.kind === SyntaxKind.OmittedExpression;
}
function isAssertionExpression(node) {
  return node.kind === SyntaxKind.TypeAssertionExpression || node.kind === SyntaxKind.AsExpression;
}
function isBooleanLiteral(node) {
  return node.kind === SyntaxKind.TrueKeyword || node.kind === SyntaxKind.FalseKeyword;
}
function isDestructuringAssignment(node) {
  return node.kind === SyntaxKind.BinaryExpression;
}
function isLiteralToken(node) {
  const kind = node.kind;
  return kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.BigIntLiteral || kind === SyntaxKind.StringLiteral || kind === SyntaxKind.JsxText || kind === SyntaxKind.RegularExpressionLiteral || kind === SyntaxKind.NoSubstitutionTemplateLiteral;
}
function isModifier(node) {
  const kind = node.kind;
  return isModifierKind(kind);
}
function isObjectLiteralElementLike(node) {
  const kind = node.kind;
  return kind === SyntaxKind.PropertyAssignment || kind === SyntaxKind.ShorthandPropertyAssignment || kind === SyntaxKind.SpreadAssignment || kind === SyntaxKind.MethodDeclaration || kind === SyntaxKind.GetAccessor || kind === SyntaxKind.SetAccessor;
}
function isPropertyNameLiteral(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.StringLiteral || node.kind === SyntaxKind.NumericLiteral;
}
function isPseudoLiteralToken(node) {
  const kind = node.kind;
  return isPseudoLiteralKind(kind);
}
function isTemplateLiteralToken(node) {
  const kind = node.kind;
  return kind === SyntaxKind.NoSubstitutionTemplateLiteral || kind === SyntaxKind.TemplateHead || kind === SyntaxKind.TemplateMiddle || kind === SyntaxKind.TemplateTail;
}
function isArrayDestructuringAssignment(node) {
  return node.kind === SyntaxKind.BinaryExpression;
}
function isObjectDestructuringAssignment(node) {
  return node.kind === SyntaxKind.BinaryExpression;
}
function isFunctionBody(node) {
  return node.kind === SyntaxKind.Block;
}
function isTriviaKind(kind) {
  return kind === SyntaxKind.SingleLineCommentTrivia || kind === SyntaxKind.MultiLineCommentTrivia || kind === SyntaxKind.NewLineTrivia || kind === SyntaxKind.WhitespaceTrivia || kind === SyntaxKind.ConflictMarkerTrivia;
}
function isPseudoLiteralKind(kind) {
  return kind === SyntaxKind.TemplateHead || kind === SyntaxKind.TemplateMiddle || kind === SyntaxKind.TemplateTail;
}
function isModifierKind(kind) {
  return kind === SyntaxKind.AbstractKeyword || kind === SyntaxKind.AccessorKeyword || kind === SyntaxKind.AsyncKeyword || kind === SyntaxKind.ConstKeyword || kind === SyntaxKind.DeclareKeyword || kind === SyntaxKind.DefaultKeyword || kind === SyntaxKind.ExportKeyword || kind === SyntaxKind.InKeyword || kind === SyntaxKind.PrivateKeyword || kind === SyntaxKind.ProtectedKeyword || kind === SyntaxKind.PublicKeyword || kind === SyntaxKind.ReadonlyKeyword || kind === SyntaxKind.OutKeyword || kind === SyntaxKind.OverrideKeyword || kind === SyntaxKind.StaticKeyword;
}
function isKeywordTypeKind(kind) {
  return kind === SyntaxKind.AnyKeyword || kind === SyntaxKind.BigIntKeyword || kind === SyntaxKind.BooleanKeyword || kind === SyntaxKind.IntrinsicKeyword || kind === SyntaxKind.NeverKeyword || kind === SyntaxKind.NumberKeyword || kind === SyntaxKind.ObjectKeyword || kind === SyntaxKind.StringKeyword || kind === SyntaxKind.SymbolKeyword || kind === SyntaxKind.UndefinedKeyword || kind === SyntaxKind.UnknownKeyword || kind === SyntaxKind.VoidKeyword;
}
function isKeywordExpressionKind(kind) {
  return kind === SyntaxKind.NullKeyword || kind === SyntaxKind.TrueKeyword || kind === SyntaxKind.FalseKeyword || kind === SyntaxKind.ThisKeyword || kind === SyntaxKind.SuperKeyword || kind === SyntaxKind.ImportKeyword;
}
function isJsxTokenKind(kind) {
  return kind === SyntaxKind.LessThanSlashToken || kind === SyntaxKind.EndOfFile || kind === SyntaxKind.ConflictMarkerTrivia || kind === SyntaxKind.JsxText || kind === SyntaxKind.JsxTextAllWhiteSpaces || kind === SyntaxKind.OpenBraceToken || kind === SyntaxKind.LessThanToken;
}
function isImportPhaseModifierKind(kind) {
  return kind === SyntaxKind.TypeKeyword || kind === SyntaxKind.DeferKeyword;
}
function isPostfixUnaryOperator(kind) {
  return kind === SyntaxKind.PlusPlusToken || kind === SyntaxKind.MinusMinusToken;
}
function isPrefixUnaryOperator(kind) {
  return kind === SyntaxKind.PlusToken || kind === SyntaxKind.MinusToken || kind === SyntaxKind.TildeToken || kind === SyntaxKind.ExclamationToken || kind === SyntaxKind.PlusPlusToken || kind === SyntaxKind.MinusMinusToken;
}
function isAssignmentOperator(kind) {
  return kind === SyntaxKind.EqualsToken || isCompoundAssignmentOperator(kind);
}
function isBinaryOperator(kind) {
  return isAssignmentOperatorOrHigher(kind) || kind === SyntaxKind.CommaToken;
}
function isExponentiationOperator(kind) {
  return kind === SyntaxKind.AsteriskAsteriskToken;
}
function isMultiplicativeOperator(kind) {
  return kind === SyntaxKind.AsteriskToken || kind === SyntaxKind.SlashToken || kind === SyntaxKind.PercentToken;
}
function isMultiplicativeOperatorOrHigher(kind) {
  return isExponentiationOperator(kind) || isMultiplicativeOperator(kind);
}
function isAdditiveOperator(kind) {
  return kind === SyntaxKind.PlusToken || kind === SyntaxKind.MinusToken;
}
function isAdditiveOperatorOrHigher(kind) {
  return isMultiplicativeOperatorOrHigher(kind) || isAdditiveOperator(kind);
}
function isShiftOperator(kind) {
  return kind === SyntaxKind.LessThanLessThanToken || kind === SyntaxKind.GreaterThanGreaterThanToken || kind === SyntaxKind.GreaterThanGreaterThanGreaterThanToken;
}
function isShiftOperatorOrHigher(kind) {
  return isAdditiveOperatorOrHigher(kind) || isShiftOperator(kind);
}
function isRelationalOperator(kind) {
  return kind === SyntaxKind.LessThanToken || kind === SyntaxKind.LessThanEqualsToken || kind === SyntaxKind.GreaterThanToken || kind === SyntaxKind.GreaterThanEqualsToken || kind === SyntaxKind.InstanceOfKeyword || kind === SyntaxKind.InKeyword;
}
function isRelationalOperatorOrHigher(kind) {
  return isShiftOperatorOrHigher(kind) || isRelationalOperator(kind);
}
function isEqualityOperator(kind) {
  return kind === SyntaxKind.EqualsEqualsToken || kind === SyntaxKind.EqualsEqualsEqualsToken || kind === SyntaxKind.ExclamationEqualsEqualsToken || kind === SyntaxKind.ExclamationEqualsToken;
}
function isEqualityOperatorOrHigher(kind) {
  return isRelationalOperatorOrHigher(kind) || isEqualityOperator(kind);
}
function isBitwiseOperator(kind) {
  return kind === SyntaxKind.AmpersandToken || kind === SyntaxKind.BarToken || kind === SyntaxKind.CaretToken;
}
function isBitwiseOperatorOrHigher(kind) {
  return isEqualityOperatorOrHigher(kind) || isBitwiseOperator(kind);
}
function isLogicalOperator(kind) {
  return kind === SyntaxKind.AmpersandAmpersandToken || kind === SyntaxKind.BarBarToken;
}
function isLogicalOperatorOrHigher(kind) {
  return isBitwiseOperatorOrHigher(kind) || isLogicalOperator(kind);
}
function isCompoundAssignmentOperator(kind) {
  return kind === SyntaxKind.PlusEqualsToken || kind === SyntaxKind.MinusEqualsToken || kind === SyntaxKind.AsteriskAsteriskEqualsToken || kind === SyntaxKind.AsteriskEqualsToken || kind === SyntaxKind.SlashEqualsToken || kind === SyntaxKind.PercentEqualsToken || kind === SyntaxKind.AmpersandEqualsToken || kind === SyntaxKind.BarEqualsToken || kind === SyntaxKind.CaretEqualsToken || kind === SyntaxKind.LessThanLessThanEqualsToken || kind === SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken || kind === SyntaxKind.GreaterThanGreaterThanEqualsToken || kind === SyntaxKind.BarBarEqualsToken || kind === SyntaxKind.AmpersandAmpersandEqualsToken || kind === SyntaxKind.QuestionQuestionEqualsToken;
}
function isAssignmentOperatorOrHigher(kind) {
  return kind === SyntaxKind.QuestionQuestionToken || isLogicalOperatorOrHigher(kind) || isAssignmentOperator(kind);
}
function isLogicalOrCoalescingAssignmentOperator(kind) {
  return kind === SyntaxKind.AmpersandAmpersandEqualsToken || kind === SyntaxKind.BarBarEqualsToken || kind === SyntaxKind.QuestionQuestionEqualsToken;
}
function isLiteralKind(kind) {
  return kind >= SyntaxKind.FirstLiteralToken && kind <= SyntaxKind.LastLiteralToken;
}
function isPunctuationKind(kind) {
  return kind >= SyntaxKind.FirstPunctuation && kind <= SyntaxKind.LastPunctuation;
}
function isKeywordKind(kind) {
  return kind >= SyntaxKind.FirstKeyword && kind <= SyntaxKind.LastKeyword;
}
function isTokenKind(kind) {
  return kind >= SyntaxKind.FirstToken && kind <= SyntaxKind.LastToken;
}
function isJSDocNodeKind(kind) {
  return kind >= SyntaxKind.FirstJSDocNode && kind <= SyntaxKind.LastJSDocNode;
}
function isEndOfFile(node) {
  return node.kind === SyntaxKind.EndOfFile;
}
function isDotToken(node) {
  return node.kind === SyntaxKind.DotToken;
}
function isDotDotDotToken(node) {
  return node.kind === SyntaxKind.DotDotDotToken;
}
function isQuestionToken(node) {
  return node.kind === SyntaxKind.QuestionToken;
}
function isExclamationToken(node) {
  return node.kind === SyntaxKind.ExclamationToken;
}
function isColonToken(node) {
  return node.kind === SyntaxKind.ColonToken;
}
function isEqualsToken(node) {
  return node.kind === SyntaxKind.EqualsToken;
}
function isAsteriskToken(node) {
  return node.kind === SyntaxKind.AsteriskToken;
}
function isEqualsGreaterThanToken(node) {
  return node.kind === SyntaxKind.EqualsGreaterThanToken;
}
function isPlusToken(node) {
  return node.kind === SyntaxKind.PlusToken;
}
function isMinusToken(node) {
  return node.kind === SyntaxKind.MinusToken;
}
function isQuestionDotToken(node) {
  return node.kind === SyntaxKind.QuestionDotToken;
}
function isAssertsKeyword(node) {
  return node.kind === SyntaxKind.AssertsKeyword;
}
function isAssertKeyword(node) {
  return node.kind === SyntaxKind.AssertKeyword;
}
function isAwaitKeyword(node) {
  return node.kind === SyntaxKind.AwaitKeyword;
}
function isCaseKeyword(node) {
  return node.kind === SyntaxKind.CaseKeyword;
}
function isAbstractKeyword(node) {
  return node.kind === SyntaxKind.AbstractKeyword;
}
function isAccessorKeyword(node) {
  return node.kind === SyntaxKind.AccessorKeyword;
}
function isAsyncKeyword(node) {
  return node.kind === SyntaxKind.AsyncKeyword;
}
function isConstKeyword(node) {
  return node.kind === SyntaxKind.ConstKeyword;
}
function isDeclareKeyword(node) {
  return node.kind === SyntaxKind.DeclareKeyword;
}
function isDefaultKeyword(node) {
  return node.kind === SyntaxKind.DefaultKeyword;
}
function isExportKeyword(node) {
  return node.kind === SyntaxKind.ExportKeyword;
}
function isInKeyword(node) {
  return node.kind === SyntaxKind.InKeyword;
}
function isPrivateKeyword(node) {
  return node.kind === SyntaxKind.PrivateKeyword;
}
function isProtectedKeyword(node) {
  return node.kind === SyntaxKind.ProtectedKeyword;
}
function isPublicKeyword(node) {
  return node.kind === SyntaxKind.PublicKeyword;
}
function isReadonlyKeyword(node) {
  return node.kind === SyntaxKind.ReadonlyKeyword;
}
function isOutKeyword(node) {
  return node.kind === SyntaxKind.OutKeyword;
}
function isOverrideKeyword(node) {
  return node.kind === SyntaxKind.OverrideKeyword;
}
function isStaticKeyword(node) {
  return node.kind === SyntaxKind.StaticKeyword;
}
function isBinaryOperatorToken(node) {
  return isBinaryOperator(node.kind);
}
function isAssignmentOperatorToken(node) {
  return isAssignmentOperator(node.kind);
}
function isNullLiteral(node) {
  return node.kind === SyntaxKind.NullKeyword;
}
function isTrueLiteral(node) {
  return node.kind === SyntaxKind.TrueKeyword;
}
function isFalseLiteral(node) {
  return node.kind === SyntaxKind.FalseKeyword;
}
function isThisExpression(node) {
  return node.kind === SyntaxKind.ThisKeyword;
}
function isSuperExpression(node) {
  return node.kind === SyntaxKind.SuperKeyword;
}
function isImportExpression(node) {
  return node.kind === SyntaxKind.ImportKeyword;
}

// node_modules/typescript/dist/ast/is.js
function isTypeNode(node) {
  return isTypeNodeKind(node.kind);
}
function isTypeNodeKind(kind) {
  return kind >= SyntaxKind.FirstTypeNode && kind <= SyntaxKind.LastTypeNode || kind === SyntaxKind.AnyKeyword || kind === SyntaxKind.UnknownKeyword || kind === SyntaxKind.NumberKeyword || kind === SyntaxKind.BigIntKeyword || kind === SyntaxKind.ObjectKeyword || kind === SyntaxKind.BooleanKeyword || kind === SyntaxKind.StringKeyword || kind === SyntaxKind.SymbolKeyword || kind === SyntaxKind.VoidKeyword || kind === SyntaxKind.UndefinedKeyword || kind === SyntaxKind.NeverKeyword || kind === SyntaxKind.IntrinsicKeyword || kind === SyntaxKind.ExpressionWithTypeArguments || kind === SyntaxKind.JSDocAllType || kind === SyntaxKind.JSDocNullableType || kind === SyntaxKind.JSDocNonNullableType || kind === SyntaxKind.JSDocOptionalType || kind === SyntaxKind.JSDocVariadicType || kind === SyntaxKind.JSDocTypeExpression || kind === SyntaxKind.JSDocTypeLiteral || kind === SyntaxKind.JSDocSignature;
}
function isStatement(node) {
  const kind = node.kind;
  return kind === SyntaxKind.VariableStatement || kind === SyntaxKind.EmptyStatement || kind === SyntaxKind.ExpressionStatement || kind === SyntaxKind.IfStatement || kind === SyntaxKind.DoStatement || kind === SyntaxKind.WhileStatement || kind === SyntaxKind.ForStatement || kind === SyntaxKind.ForInStatement || kind === SyntaxKind.ForOfStatement || kind === SyntaxKind.ContinueStatement || kind === SyntaxKind.BreakStatement || kind === SyntaxKind.ReturnStatement || kind === SyntaxKind.WithStatement || kind === SyntaxKind.SwitchStatement || kind === SyntaxKind.LabeledStatement || kind === SyntaxKind.ThrowStatement || kind === SyntaxKind.TryStatement || kind === SyntaxKind.DebuggerStatement || kind === SyntaxKind.InterfaceDeclaration || kind === SyntaxKind.TypeAliasDeclaration || kind === SyntaxKind.EnumDeclaration || kind === SyntaxKind.ModuleDeclaration || kind === SyntaxKind.ImportDeclaration || kind === SyntaxKind.ImportEqualsDeclaration || kind === SyntaxKind.ExportDeclaration || kind === SyntaxKind.ExportAssignment || kind === SyntaxKind.NamespaceExportDeclaration || kind === SyntaxKind.FunctionDeclaration || kind === SyntaxKind.ClassDeclaration || kind === SyntaxKind.MissingDeclaration || kind === SyntaxKind.NotEmittedStatement || kind === SyntaxKind.Block;
}
function isExpression(node) {
  const kind = node.kind;
  return kind === SyntaxKind.ConditionalExpression || kind === SyntaxKind.YieldExpression || kind === SyntaxKind.ArrowFunction || kind === SyntaxKind.BinaryExpression || kind === SyntaxKind.SpreadElement || kind === SyntaxKind.AsExpression || kind === SyntaxKind.OmittedExpression || kind === SyntaxKind.SatisfiesExpression || kind === SyntaxKind.PrefixUnaryExpression || kind === SyntaxKind.PostfixUnaryExpression || kind === SyntaxKind.DeleteExpression || kind === SyntaxKind.TypeOfExpression || kind === SyntaxKind.VoidExpression || kind === SyntaxKind.AwaitExpression || kind === SyntaxKind.TypeAssertionExpression || kind === SyntaxKind.CallExpression || kind === SyntaxKind.NewExpression || kind === SyntaxKind.TaggedTemplateExpression || kind === SyntaxKind.NonNullExpression || kind === SyntaxKind.MetaProperty || kind === SyntaxKind.JsxExpression || kind === SyntaxKind.PropertyAccessExpression || kind === SyntaxKind.ElementAccessExpression || kind === SyntaxKind.FunctionExpression || kind === SyntaxKind.ClassExpression || kind === SyntaxKind.ParenthesizedExpression || kind === SyntaxKind.ArrayLiteralExpression || kind === SyntaxKind.ObjectLiteralExpression || kind === SyntaxKind.TemplateExpression || kind === SyntaxKind.Identifier || kind === SyntaxKind.PrivateIdentifier || kind === SyntaxKind.NumericLiteral || kind === SyntaxKind.BigIntLiteral || kind === SyntaxKind.StringLiteral || kind === SyntaxKind.RegularExpressionLiteral || kind === SyntaxKind.NoSubstitutionTemplateLiteral || kind === SyntaxKind.JsxElement || kind === SyntaxKind.JsxSelfClosingElement || kind === SyntaxKind.JsxFragment || kind === SyntaxKind.NullKeyword || kind === SyntaxKind.TrueKeyword || kind === SyntaxKind.FalseKeyword || kind === SyntaxKind.ThisKeyword || kind === SyntaxKind.SuperKeyword || kind === SyntaxKind.ImportKeyword || kind === SyntaxKind.ExpressionWithTypeArguments;
}
function isBlockOrExpression(node) {
  return node.kind === SyntaxKind.Block || isExpression(node);
}
function isLeftHandSideExpression(node) {
  return isLeftHandSideExpressionKind(skipPartiallyEmittedExpressions(node).kind);
}
function skipPartiallyEmittedExpressions(node) {
  return skipOuterExpressions(node, OuterExpressionKinds.PartiallyEmittedExpressions);
}
function isLeftHandSideExpressionKind(kind) {
  switch (kind) {
    case SyntaxKind.PropertyAccessExpression:
    case SyntaxKind.ElementAccessExpression:
    case SyntaxKind.NewExpression:
    case SyntaxKind.CallExpression:
    case SyntaxKind.JsxElement:
    case SyntaxKind.JsxSelfClosingElement:
    case SyntaxKind.JsxFragment:
    case SyntaxKind.TaggedTemplateExpression:
    case SyntaxKind.ArrayLiteralExpression:
    case SyntaxKind.ParenthesizedExpression:
    case SyntaxKind.ObjectLiteralExpression:
    case SyntaxKind.ClassExpression:
    case SyntaxKind.FunctionExpression:
    case SyntaxKind.Identifier:
    case SyntaxKind.PrivateIdentifier:
    // technically this is only an Expression if it's in a `#field in expr` BinaryExpression
    case SyntaxKind.RegularExpressionLiteral:
    case SyntaxKind.NumericLiteral:
    case SyntaxKind.BigIntLiteral:
    case SyntaxKind.StringLiteral:
    case SyntaxKind.NoSubstitutionTemplateLiteral:
    case SyntaxKind.TemplateExpression:
    case SyntaxKind.FalseKeyword:
    case SyntaxKind.NullKeyword:
    case SyntaxKind.ThisKeyword:
    case SyntaxKind.TrueKeyword:
    case SyntaxKind.SuperKeyword:
    case SyntaxKind.NonNullExpression:
    case SyntaxKind.ExpressionWithTypeArguments:
    case SyntaxKind.MetaProperty:
    case SyntaxKind.ImportKeyword:
    // technically this is only an Expression if it's in a CallExpression
    case SyntaxKind.MissingDeclaration:
      return true;
    default:
      return false;
  }
}
function isUnaryExpression(node) {
  return isUnaryExpressionKind(skipPartiallyEmittedExpressions(node).kind);
}
function isUnaryExpressionKind(kind) {
  switch (kind) {
    case SyntaxKind.PrefixUnaryExpression:
    case SyntaxKind.PostfixUnaryExpression:
    case SyntaxKind.DeleteExpression:
    case SyntaxKind.TypeOfExpression:
    case SyntaxKind.VoidExpression:
    case SyntaxKind.AwaitExpression:
    case SyntaxKind.TypeAssertionExpression:
      return true;
    default:
      return isLeftHandSideExpressionKind(kind);
  }
}
function isOuterExpression(node, kinds = OuterExpressionKinds.All) {
  switch (node.kind) {
    case SyntaxKind.ParenthesizedExpression:
      if (kinds & OuterExpressionKinds.ExcludeJSDocTypeAssertion && isJSDocTypeAssertion(node)) {
        return false;
      }
      return (kinds & OuterExpressionKinds.Parentheses) !== 0;
    case SyntaxKind.TypeAssertionExpression:
    case SyntaxKind.AsExpression:
      return (kinds & OuterExpressionKinds.TypeAssertions) !== 0;
    case SyntaxKind.SatisfiesExpression:
      return (kinds & (OuterExpressionKinds.ExpressionsWithTypeArguments | OuterExpressionKinds.Satisfies)) !== 0;
    case SyntaxKind.ExpressionWithTypeArguments:
      return (kinds & OuterExpressionKinds.ExpressionsWithTypeArguments) !== 0;
    case SyntaxKind.NonNullExpression:
      return (kinds & OuterExpressionKinds.NonNullAssertions) !== 0;
    case SyntaxKind.PartiallyEmittedExpression:
      return (kinds & OuterExpressionKinds.PartiallyEmittedExpressions) !== 0;
  }
  return false;
}
function skipOuterExpressions(node, kinds = OuterExpressionKinds.All) {
  while (isOuterExpression(node, kinds)) {
    node = node.expression;
  }
  return node;
}
function isJSDocTypeAssertion(node) {
  const sourceFile = node.getSourceFile();
  if (sourceFile.scriptKind !== ScriptKind.JS && sourceFile.scriptKind !== ScriptKind.JSX) {
    return false;
  }
  const expression = node.expression;
  if (expression.kind !== SyntaxKind.AsExpression) {
    return false;
  }
  const asExpression = expression;
  return !!asExpression.type && (asExpression.type.flags & NodeFlags.Reparsed) !== 0;
}
function isBindingPattern(node) {
  return node.kind === SyntaxKind.ObjectBindingPattern || node.kind === SyntaxKind.ArrayBindingPattern;
}
function isConciseBody(node) {
  return node.kind === SyntaxKind.Block || isExpression(node);
}
function isForInitializer(node) {
  return node.kind === SyntaxKind.VariableDeclarationList || isExpression(node);
}
function isQuestionOrExclamationToken(node) {
  return node.kind === SyntaxKind.QuestionToken || node.kind === SyntaxKind.ExclamationToken;
}
function isIdentifierOrThisTypeNode(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.ThisType;
}
function isReadonlyKeywordOrPlusOrMinusToken(node) {
  return node.kind === SyntaxKind.ReadonlyKeyword || node.kind === SyntaxKind.PlusToken || node.kind === SyntaxKind.MinusToken;
}
function isQuestionOrPlusOrMinusToken(node) {
  return node.kind === SyntaxKind.QuestionToken || node.kind === SyntaxKind.PlusToken || node.kind === SyntaxKind.MinusToken;
}
function isTemplateMiddleOrTemplateTail(node) {
  return node.kind === SyntaxKind.TemplateMiddle || node.kind === SyntaxKind.TemplateTail;
}
function isLiteralTypeLiteral(node) {
  const kind = node.kind;
  return kind === SyntaxKind.NullKeyword || kind === SyntaxKind.TrueKeyword || kind === SyntaxKind.FalseKeyword || kind === SyntaxKind.PrefixUnaryExpression || isLiteralExpression(node);
}
function isIdentifierOrJSDocNamespaceDeclaration(node) {
  return node.kind === SyntaxKind.Identifier || node.kind === SyntaxKind.ModuleDeclaration;
}
function isJSDocTypeExpressionOrJSDocTypeLiteral(node) {
  return node.kind === SyntaxKind.JSDocTypeExpression || node.kind === SyntaxKind.JSDocTypeLiteral;
}
function isJsxTagNameExpression(node) {
  const kind = node.kind;
  return kind === SyntaxKind.ThisKeyword || kind === SyntaxKind.Identifier || kind === SyntaxKind.PropertyAccessExpression || kind === SyntaxKind.JsxNamespacedName;
}

// node_modules/typescript/dist/ast/visitor.generated.js
function visitNode(node, visitor, test) {
  if (node === void 0)
    return void 0;
  const visited = visitor(node);
  if (visited !== void 0 && test !== void 0 && !test(visited)) {
    throw new Error("Visited node failed test assertion.");
  }
  return visited;
}
function visitNodes(nodes, visitor) {
  if (nodes === void 0)
    return void 0;
  const updated = visitNodesArray(nodes, visitor);
  if (updated === nodes) {
    return nodes;
  }
  return createNodeArray(updated, nodes.pos, nodes.end);
}
function visitNodesArray(nodes, visitor) {
  if (nodes === void 0)
    return void 0;
  let updated;
  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    const visited = visitor(node);
    if (updated) {
      if (visited)
        updated.push(visited);
    } else if (visited !== node) {
      updated = [];
      for (let j = 0; j < i; j++)
        updated.push(nodes[j]);
      if (visited)
        updated.push(visited);
    }
  }
  return updated ?? nodes;
}
function visitEachChild(node, visitor) {
  if (node === void 0)
    return void 0;
  const fn = visitEachChildTable[node.kind];
  return fn ? fn(node, visitor) : node;
}
var visitEachChildTable = {
  [SyntaxKind.QualifiedName]: (node, visitor) => {
    const _left = visitNode(node.left, visitor, isEntityName);
    const _right = visitNode(node.right, visitor, isIdentifier);
    return updateQualifiedName(node, _left, _right);
  },
  [SyntaxKind.ComputedPropertyName]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateComputedPropertyName(node, _expression);
  },
  [SyntaxKind.Decorator]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isLeftHandSideExpression);
    return updateDecorator(node, _expression);
  },
  [SyntaxKind.IfStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _thenStatement = visitNode(node.thenStatement, visitor, isStatement);
    const _elseStatement = visitNode(node.elseStatement, visitor, isStatement);
    return updateIfStatement(node, _expression, _thenStatement, _elseStatement);
  },
  [SyntaxKind.DoStatement]: (node, visitor) => {
    const _statement = visitNode(node.statement, visitor, isStatement);
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateDoStatement(node, _statement, _expression);
  },
  [SyntaxKind.WhileStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateWhileStatement(node, _expression, _statement);
  },
  [SyntaxKind.ForStatement]: (node, visitor) => {
    const _initializer = visitNode(node.initializer, visitor, isForInitializer);
    const _condition = visitNode(node.condition, visitor, isExpression);
    const _incrementor = visitNode(node.incrementor, visitor, isExpression);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateForStatement(node, _initializer, _condition, _incrementor, _statement);
  },
  [SyntaxKind.BreakStatement]: (node, visitor) => {
    const _label = visitNode(node.label, visitor, isIdentifier);
    return updateBreakStatement(node, _label);
  },
  [SyntaxKind.ContinueStatement]: (node, visitor) => {
    const _label = visitNode(node.label, visitor, isIdentifier);
    return updateContinueStatement(node, _label);
  },
  [SyntaxKind.ReturnStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateReturnStatement(node, _expression);
  },
  [SyntaxKind.WithStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateWithStatement(node, _expression, _statement);
  },
  [SyntaxKind.SwitchStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _caseBlock = visitNode(node.caseBlock, visitor, isCaseBlock);
    return updateSwitchStatement(node, _expression, _caseBlock);
  },
  [SyntaxKind.CaseBlock]: (node, visitor) => {
    const _clauses = visitNodes(node.clauses, visitor);
    return updateCaseBlock(node, _clauses);
  },
  [SyntaxKind.ThrowStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateThrowStatement(node, _expression);
  },
  [SyntaxKind.TryStatement]: (node, visitor) => {
    const _tryBlock = visitNode(node.tryBlock, visitor, isBlock);
    const _catchClause = visitNode(node.catchClause, visitor, isCatchClause);
    const _finallyBlock = visitNode(node.finallyBlock, visitor, isBlock);
    return updateTryStatement(node, _tryBlock, _catchClause, _finallyBlock);
  },
  [SyntaxKind.CatchClause]: (node, visitor) => {
    const _variableDeclaration = visitNode(node.variableDeclaration, visitor, isVariableDeclaration);
    const _block = visitNode(node.block, visitor, isBlock);
    return updateCatchClause(node, _variableDeclaration, _block);
  },
  [SyntaxKind.LabeledStatement]: (node, visitor) => {
    const _label = visitNode(node.label, visitor, isIdentifier);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateLabeledStatement(node, _label, _statement);
  },
  [SyntaxKind.ExpressionStatement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateExpressionStatement(node, _expression);
  },
  [SyntaxKind.Block]: (node, visitor) => {
    const _statements = visitNodes(node.statements, visitor);
    return updateBlock(node, _statements);
  },
  [SyntaxKind.VariableStatement]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _declarationList = visitNode(node.declarationList, visitor, isVariableDeclarationList);
    return updateVariableStatement(node, _modifiers, _declarationList);
  },
  [SyntaxKind.VariableDeclaration]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isBindingName);
    const _exclamationToken = visitNode(node.exclamationToken, visitor, isExclamationToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updateVariableDeclaration(node, _name, _exclamationToken, _type, _initializer);
  },
  [SyntaxKind.VariableDeclarationList]: (node, visitor) => {
    const _declarations = visitNodes(node.declarations, visitor);
    return updateVariableDeclarationList(node, _declarations);
  },
  [SyntaxKind.Parameter]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _dotDotDotToken = visitNode(node.dotDotDotToken, visitor, isDotDotDotToken);
    const _name = visitNode(node.name, visitor, isBindingName);
    const _questionToken = visitNode(node.questionToken, visitor, isQuestionToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updateParameterDeclaration(node, _modifiers, _dotDotDotToken, _name, _questionToken, _type, _initializer);
  },
  [SyntaxKind.BindingElement]: (node, visitor) => {
    const _dotDotDotToken = visitNode(node.dotDotDotToken, visitor, isDotDotDotToken);
    const _propertyName = visitNode(node.propertyName, visitor, isPropertyName);
    const _name = visitNode(node.name, visitor, isBindingName);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updateBindingElement(node, _dotDotDotToken, _propertyName, _name, _initializer);
  },
  [SyntaxKind.MissingDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    return updateMissingDeclaration(node, _modifiers);
  },
  [SyntaxKind.FunctionDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _asteriskToken = visitNode(node.asteriskToken, visitor, isAsteriskToken);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateFunctionDeclaration(node, _modifiers, _asteriskToken, _name, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.ClassDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _heritageClauses = visitNodes(node.heritageClauses, visitor);
    const _members = visitNodes(node.members, visitor);
    return updateClassDeclaration(node, _modifiers, _name, _typeParameters, _heritageClauses, _members);
  },
  [SyntaxKind.ClassExpression]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _heritageClauses = visitNodes(node.heritageClauses, visitor);
    const _members = visitNodes(node.members, visitor);
    return updateClassExpression(node, _modifiers, _name, _typeParameters, _heritageClauses, _members);
  },
  [SyntaxKind.HeritageClause]: (node, visitor) => {
    const _types = visitNodes(node.types, visitor);
    return updateHeritageClause(node, _types);
  },
  [SyntaxKind.InterfaceDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _heritageClauses = visitNodes(node.heritageClauses, visitor);
    const _members = visitNodes(node.members, visitor);
    return updateInterfaceDeclaration(node, _modifiers, _name, _typeParameters, _heritageClauses, _members);
  },
  [SyntaxKind.TypeAliasDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateTypeAliasDeclaration(node, _modifiers, _name, _typeParameters, _type);
  },
  [SyntaxKind.EnumMember]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updateEnumMember(node, _name, _initializer);
  },
  [SyntaxKind.EnumDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _members = visitNodes(node.members, visitor);
    return updateEnumDeclaration(node, _modifiers, _name, _members);
  },
  [SyntaxKind.ModuleBlock]: (node, visitor) => {
    const _statements = visitNodes(node.statements, visitor);
    return updateModuleBlock(node, _statements);
  },
  [SyntaxKind.ImportDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _importClause = visitNode(node.importClause, visitor, isImportClause);
    const _moduleSpecifier = visitNode(node.moduleSpecifier, visitor, isExpression);
    const _attributes = visitNode(node.attributes, visitor, isImportAttributes);
    return updateImportDeclaration(node, _modifiers, _importClause, _moduleSpecifier, _attributes);
  },
  [SyntaxKind.ExternalModuleReference]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateExternalModuleReference(node, _expression);
  },
  [SyntaxKind.NamespaceImport]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isIdentifier);
    return updateNamespaceImport(node, _name);
  },
  [SyntaxKind.NamedImports]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateNamedImports(node, _elements);
  },
  [SyntaxKind.ExportAssignment]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateExportAssignment(node, _modifiers, _type, _expression);
  },
  [SyntaxKind.NamespaceExportDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    return updateNamespaceExportDeclaration(node, _modifiers, _name);
  },
  [SyntaxKind.NamespaceExport]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isModuleExportName);
    return updateNamespaceExport(node, _name);
  },
  [SyntaxKind.NamedExports]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateNamedExports(node, _elements);
  },
  [SyntaxKind.ExportSpecifier]: (node, visitor) => {
    const _propertyName = visitNode(node.propertyName, visitor, isModuleExportName);
    const _name = visitNode(node.name, visitor, isModuleExportName);
    return updateExportSpecifier(node, _propertyName, _name);
  },
  [SyntaxKind.CallSignature]: (node, visitor) => {
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateCallSignatureDeclaration(node, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.ConstructSignature]: (node, visitor) => {
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateConstructSignatureDeclaration(node, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.Constructor]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateConstructorDeclaration(node, _modifiers, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.GetAccessor]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateGetAccessorDeclaration(node, _modifiers, _name, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.SetAccessor]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateSetAccessorDeclaration(node, _modifiers, _name, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.IndexSignature]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateIndexSignatureDeclaration(node, _modifiers, _parameters, _type);
  },
  [SyntaxKind.MethodSignature]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateMethodSignatureDeclaration(node, _modifiers, _name, _postfixToken, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.MethodDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _asteriskToken = visitNode(node.asteriskToken, visitor, isAsteriskToken);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateMethodDeclaration(node, _modifiers, _asteriskToken, _name, _postfixToken, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.PropertySignature]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updatePropertySignatureDeclaration(node, _modifiers, _name, _postfixToken, _type, _initializer);
  },
  [SyntaxKind.PropertyDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updatePropertyDeclaration(node, _modifiers, _name, _postfixToken, _type, _initializer);
  },
  [SyntaxKind.ClassStaticBlockDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _body = visitNode(node.body, visitor, isBlock);
    return updateClassStaticBlockDeclaration(node, _modifiers, _body);
  },
  [SyntaxKind.BinaryExpression]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _left = visitNode(node.left, visitor, isExpression);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _operatorToken = visitNode(node.operatorToken, visitor, isBinaryOperatorToken);
    const _right = visitNode(node.right, visitor, isExpression);
    return updateBinaryExpression(node, _modifiers, _left, _type, _operatorToken, _right);
  },
  [SyntaxKind.PrefixUnaryExpression]: (node, visitor) => {
    const _operand = visitNode(node.operand, visitor, isExpression);
    return updatePrefixUnaryExpression(node, _operand);
  },
  [SyntaxKind.PostfixUnaryExpression]: (node, visitor) => {
    const _operand = visitNode(node.operand, visitor, isExpression);
    return updatePostfixUnaryExpression(node, _operand);
  },
  [SyntaxKind.YieldExpression]: (node, visitor) => {
    const _asteriskToken = visitNode(node.asteriskToken, visitor, isAsteriskToken);
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateYieldExpression(node, _asteriskToken, _expression);
  },
  [SyntaxKind.ArrowFunction]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _equalsGreaterThanToken = visitNode(node.equalsGreaterThanToken, visitor, isEqualsGreaterThanToken);
    const _body = visitNode(node.body, visitor, isConciseBody);
    return updateArrowFunction(node, _modifiers, _typeParameters, _parameters, _type, _equalsGreaterThanToken, _body);
  },
  [SyntaxKind.FunctionExpression]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _asteriskToken = visitNode(node.asteriskToken, visitor, isAsteriskToken);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _body = visitNode(node.body, visitor, isFunctionBody);
    return updateFunctionExpression(node, _modifiers, _asteriskToken, _name, _typeParameters, _parameters, _type, _body);
  },
  [SyntaxKind.AsExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateAsExpression(node, _expression, _type);
  },
  [SyntaxKind.SatisfiesExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateSatisfiesExpression(node, _expression, _type);
  },
  [SyntaxKind.ConditionalExpression]: (node, visitor) => {
    const _condition = visitNode(node.condition, visitor, isExpression);
    const _questionToken = visitNode(node.questionToken, visitor, isQuestionToken);
    const _whenTrue = visitNode(node.whenTrue, visitor, isExpression);
    const _colonToken = visitNode(node.colonToken, visitor, isColonToken);
    const _whenFalse = visitNode(node.whenFalse, visitor, isExpression);
    return updateConditionalExpression(node, _condition, _questionToken, _whenTrue, _colonToken, _whenFalse);
  },
  [SyntaxKind.PropertyAccessExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _questionDotToken = visitNode(node.questionDotToken, visitor, isQuestionDotToken);
    const _name = visitNode(node.name, visitor, isMemberName);
    return updatePropertyAccessExpression(node, _expression, _questionDotToken, _name);
  },
  [SyntaxKind.ElementAccessExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _questionDotToken = visitNode(node.questionDotToken, visitor, isQuestionDotToken);
    const _argumentExpression = visitNode(node.argumentExpression, visitor, isExpression);
    return updateElementAccessExpression(node, _expression, _questionDotToken, _argumentExpression);
  },
  [SyntaxKind.CallExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _questionDotToken = visitNode(node.questionDotToken, visitor, isQuestionDotToken);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    const _arguments = visitNodes(node.arguments, visitor);
    return updateCallExpression(node, _expression, _questionDotToken, _typeArguments, _arguments);
  },
  [SyntaxKind.NewExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    const _arguments = visitNodes(node.arguments, visitor);
    return updateNewExpression(node, _expression, _typeArguments, _arguments);
  },
  [SyntaxKind.MetaProperty]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isIdentifier);
    return updateMetaProperty(node, _name);
  },
  [SyntaxKind.NonNullExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateNonNullExpression(node, _expression);
  },
  [SyntaxKind.SpreadElement]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateSpreadElement(node, _expression);
  },
  [SyntaxKind.TemplateExpression]: (node, visitor) => {
    const _head = visitNode(node.head, visitor, isTemplateHead);
    const _templateSpans = visitNodes(node.templateSpans, visitor);
    return updateTemplateExpression(node, _head, _templateSpans);
  },
  [SyntaxKind.TemplateSpan]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _literal = visitNode(node.literal, visitor, isTemplateMiddleOrTail);
    return updateTemplateSpan(node, _expression, _literal);
  },
  [SyntaxKind.TaggedTemplateExpression]: (node, visitor) => {
    const _tag = visitNode(node.tag, visitor, isExpression);
    const _questionDotToken = visitNode(node.questionDotToken, visitor, isQuestionDotToken);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    const _template = visitNode(node.template, visitor, isTemplateLiteral);
    return updateTaggedTemplateExpression(node, _tag, _questionDotToken, _typeArguments, _template);
  },
  [SyntaxKind.ParenthesizedExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateParenthesizedExpression(node, _expression);
  },
  [SyntaxKind.ArrayLiteralExpression]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateArrayLiteralExpression(node, _elements);
  },
  [SyntaxKind.ObjectLiteralExpression]: (node, visitor) => {
    const _properties = visitNodes(node.properties, visitor);
    return updateObjectLiteralExpression(node, _properties);
  },
  [SyntaxKind.SpreadAssignment]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateSpreadAssignment(node, _expression);
  },
  [SyntaxKind.PropertyAssignment]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _initializer = visitNode(node.initializer, visitor, isExpression);
    return updatePropertyAssignment(node, _modifiers, _name, _postfixToken, _type, _initializer);
  },
  [SyntaxKind.ShorthandPropertyAssignment]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isPropertyName);
    const _postfixToken = visitNode(node.postfixToken, visitor, isQuestionOrExclamationToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _equalsToken = visitNode(node.equalsToken, visitor, isEqualsToken);
    const _objectAssignmentInitializer = visitNode(node.objectAssignmentInitializer, visitor, isExpression);
    return updateShorthandPropertyAssignment(node, _modifiers, _name, _postfixToken, _type, _equalsToken, _objectAssignmentInitializer);
  },
  [SyntaxKind.DeleteExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateDeleteExpression(node, _expression);
  },
  [SyntaxKind.TypeOfExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateTypeOfExpression(node, _expression);
  },
  [SyntaxKind.VoidExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateVoidExpression(node, _expression);
  },
  [SyntaxKind.AwaitExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateAwaitExpression(node, _expression);
  },
  [SyntaxKind.TypeAssertionExpression]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateTypeAssertion(node, _type, _expression);
  },
  [SyntaxKind.UnionType]: (node, visitor) => {
    const _types = visitNodes(node.types, visitor);
    return updateUnionTypeNode(node, _types);
  },
  [SyntaxKind.IntersectionType]: (node, visitor) => {
    const _types = visitNodes(node.types, visitor);
    return updateIntersectionTypeNode(node, _types);
  },
  [SyntaxKind.ConditionalType]: (node, visitor) => {
    const _checkType = visitNode(node.checkType, visitor, isTypeNode);
    const _extendsType = visitNode(node.extendsType, visitor, isTypeNode);
    const _trueType = visitNode(node.trueType, visitor, isTypeNode);
    const _falseType = visitNode(node.falseType, visitor, isTypeNode);
    return updateConditionalTypeNode(node, _checkType, _extendsType, _trueType, _falseType);
  },
  [SyntaxKind.TypeOperator]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateTypeOperatorNode(node, _type);
  },
  [SyntaxKind.InferType]: (node, visitor) => {
    const _typeParameter = visitNode(node.typeParameter, visitor, isTypeParameterDeclaration);
    return updateInferTypeNode(node, _typeParameter);
  },
  [SyntaxKind.ArrayType]: (node, visitor) => {
    const _elementType = visitNode(node.elementType, visitor, isTypeNode);
    return updateArrayTypeNode(node, _elementType);
  },
  [SyntaxKind.IndexedAccessType]: (node, visitor) => {
    const _objectType = visitNode(node.objectType, visitor, isTypeNode);
    const _indexType = visitNode(node.indexType, visitor, isTypeNode);
    return updateIndexedAccessTypeNode(node, _objectType, _indexType);
  },
  [SyntaxKind.TypeReference]: (node, visitor) => {
    const _typeName = visitNode(node.typeName, visitor, isEntityName);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    return updateTypeReferenceNode(node, _typeName, _typeArguments);
  },
  [SyntaxKind.ExpressionWithTypeArguments]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    return updateExpressionWithTypeArguments(node, _expression, _typeArguments);
  },
  [SyntaxKind.LiteralType]: (node, visitor) => {
    const _literal = visitNode(node.literal, visitor);
    return updateLiteralTypeNode(node, _literal);
  },
  [SyntaxKind.TypePredicate]: (node, visitor) => {
    const _assertsModifier = visitNode(node.assertsModifier, visitor, isAssertsKeyword);
    const _parameterName = visitNode(node.parameterName, visitor, isTypePredicateParameterName);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateTypePredicateNode(node, _assertsModifier, _parameterName, _type);
  },
  [SyntaxKind.ImportAttribute]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isImportAttributeName);
    const _value = visitNode(node.value, visitor, isExpression);
    return updateImportAttribute(node, _name, _value);
  },
  [SyntaxKind.ImportAttributes]: (node, visitor) => {
    const _attributes = visitNodes(node.attributes, visitor);
    return updateImportAttributes(node, _attributes);
  },
  [SyntaxKind.TypeQuery]: (node, visitor) => {
    const _exprName = visitNode(node.exprName, visitor, isEntityName);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    return updateTypeQueryNode(node, _exprName, _typeArguments);
  },
  [SyntaxKind.MappedType]: (node, visitor) => {
    const _readonlyToken = visitNode(node.readonlyToken, visitor, isReadonlyKeywordOrPlusOrMinusToken);
    const _typeParameter = visitNode(node.typeParameter, visitor, isTypeParameterDeclaration);
    const _nameType = visitNode(node.nameType, visitor, isTypeNode);
    const _questionToken = visitNode(node.questionToken, visitor, isQuestionOrPlusOrMinusToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _members = visitNodes(node.members, visitor);
    return updateMappedTypeNode(node, _readonlyToken, _typeParameter, _nameType, _questionToken, _type, _members);
  },
  [SyntaxKind.TypeLiteral]: (node, visitor) => {
    const _members = visitNodes(node.members, visitor);
    return updateTypeLiteralNode(node, _members);
  },
  [SyntaxKind.TupleType]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateTupleTypeNode(node, _elements);
  },
  [SyntaxKind.NamedTupleMember]: (node, visitor) => {
    const _dotDotDotToken = visitNode(node.dotDotDotToken, visitor, isDotDotDotToken);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _questionToken = visitNode(node.questionToken, visitor, isQuestionToken);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateNamedTupleMember(node, _dotDotDotToken, _name, _questionToken, _type);
  },
  [SyntaxKind.OptionalType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateOptionalTypeNode(node, _type);
  },
  [SyntaxKind.RestType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateRestTypeNode(node, _type);
  },
  [SyntaxKind.ParenthesizedType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateParenthesizedTypeNode(node, _type);
  },
  [SyntaxKind.FunctionType]: (node, visitor) => {
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateFunctionTypeNode(node, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.ConstructorType]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateConstructorTypeNode(node, _modifiers, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.TemplateLiteralType]: (node, visitor) => {
    const _head = visitNode(node.head, visitor, isTemplateHead);
    const _templateSpans = visitNodes(node.templateSpans, visitor);
    return updateTemplateLiteralTypeNode(node, _head, _templateSpans);
  },
  [SyntaxKind.TemplateLiteralTypeSpan]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    const _literal = visitNode(node.literal, visitor, isTemplateMiddleOrTail);
    return updateTemplateLiteralTypeSpan(node, _type, _literal);
  },
  [SyntaxKind.SyntheticExpression]: (node, visitor) => {
    const _tupleNameSource = visitNode(node.tupleNameSource, visitor);
    return updateSyntheticExpression(node, _tupleNameSource);
  },
  [SyntaxKind.PartiallyEmittedExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updatePartiallyEmittedExpression(node, _expression);
  },
  [SyntaxKind.JsxElement]: (node, visitor) => {
    const _openingElement = visitNode(node.openingElement, visitor, isJsxOpeningElement);
    const _children = visitNodes(node.children, visitor);
    const _closingElement = visitNode(node.closingElement, visitor, isJsxClosingElement);
    return updateJsxElement(node, _openingElement, _children, _closingElement);
  },
  [SyntaxKind.JsxAttributes]: (node, visitor) => {
    const _properties = visitNodes(node.properties, visitor);
    return updateJsxAttributes(node, _properties);
  },
  [SyntaxKind.JsxNamespacedName]: (node, visitor) => {
    const _namespace = visitNode(node.namespace, visitor, isIdentifier);
    const _name = visitNode(node.name, visitor, isIdentifier);
    return updateJsxNamespacedName(node, _namespace, _name);
  },
  [SyntaxKind.JsxOpeningElement]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isJsxTagNameExpression);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    const _attributes = visitNode(node.attributes, visitor, isJsxAttributes);
    return updateJsxOpeningElement(node, _tagName, _typeArguments, _attributes);
  },
  [SyntaxKind.JsxSelfClosingElement]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isJsxTagNameExpression);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    const _attributes = visitNode(node.attributes, visitor, isJsxAttributes);
    return updateJsxSelfClosingElement(node, _tagName, _typeArguments, _attributes);
  },
  [SyntaxKind.JsxFragment]: (node, visitor) => {
    const _openingFragment = visitNode(node.openingFragment, visitor, isJsxOpeningFragment);
    const _children = visitNodes(node.children, visitor);
    const _closingFragment = visitNode(node.closingFragment, visitor, isJsxClosingFragment);
    return updateJsxFragment(node, _openingFragment, _children, _closingFragment);
  },
  [SyntaxKind.JsxAttribute]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isJsxAttributeName);
    const _initializer = visitNode(node.initializer, visitor, isJsxAttributeValue);
    return updateJsxAttribute(node, _name, _initializer);
  },
  [SyntaxKind.JsxSpreadAttribute]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateJsxSpreadAttribute(node, _expression);
  },
  [SyntaxKind.JsxClosingElement]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isJsxTagNameExpression);
    return updateJsxClosingElement(node, _tagName);
  },
  [SyntaxKind.JsxExpression]: (node, visitor) => {
    const _dotDotDotToken = visitNode(node.dotDotDotToken, visitor, isDotDotDotToken);
    const _expression = visitNode(node.expression, visitor, isExpression);
    return updateJsxExpression(node, _dotDotDotToken, _expression);
  },
  [SyntaxKind.SyntaxList]: (node, visitor) => {
    const _children = visitNodesArray(node.children, visitor);
    return updateSyntaxList(node, _children);
  },
  [SyntaxKind.JSDoc]: (node, visitor) => {
    const _comment = visitNodes(node.comment, visitor);
    const _tags = visitNodes(node.tags, visitor);
    return updateJSDoc(node, _comment, _tags);
  },
  [SyntaxKind.JSDocTypeExpression]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocTypeExpression(node, _type);
  },
  [SyntaxKind.JSDocNonNullableType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocNonNullableType(node, _type);
  },
  [SyntaxKind.JSDocNullableType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocNullableType(node, _type);
  },
  [SyntaxKind.JSDocVariadicType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocVariadicType(node, _type);
  },
  [SyntaxKind.JSDocOptionalType]: (node, visitor) => {
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocOptionalType(node, _type);
  },
  [SyntaxKind.JSDocTypeTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocTypeTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocUnknownTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocUnknownTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocTemplateTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _constraint = visitNode(node.constraint, visitor);
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocTemplateTag(node, _tagName, _constraint, _typeParameters, _comment);
  },
  [SyntaxKind.JSDocReturnTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocReturnTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocPublicTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocPublicTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocPrivateTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocPrivateTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocProtectedTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocProtectedTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocReadonlyTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocReadonlyTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocOverrideTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocOverrideTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocDeprecatedTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocDeprecatedTag(node, _tagName, _comment);
  },
  [SyntaxKind.JSDocSeeTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _nameExpression = visitNode(node.nameExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocSeeTag(node, _tagName, _nameExpression, _comment);
  },
  [SyntaxKind.JSDocImplementsTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _className = visitNode(node.className, visitor, isExpressionWithTypeArguments);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocImplementsTag(node, _tagName, _className, _comment);
  },
  [SyntaxKind.JSDocAugmentsTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _className = visitNode(node.className, visitor, isExpressionWithTypeArguments);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocAugmentsTag(node, _tagName, _className, _comment);
  },
  [SyntaxKind.JSDocSatisfiesTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocSatisfiesTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocThrowsTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocThrowsTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocThisTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocThisTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocImportTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _importClause = visitNode(node.importClause, visitor, isImportClause);
    const _moduleSpecifier = visitNode(node.moduleSpecifier, visitor, isExpression);
    const _attributes = visitNode(node.attributes, visitor, isImportAttributes);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocImportTag(node, _tagName, _importClause, _moduleSpecifier, _attributes, _comment);
  },
  [SyntaxKind.JSDocCallbackTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _name = visitNode(node.name, visitor, isJSDocFullName);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocCallbackTag(node, _tagName, _typeExpression, _name, _comment);
  },
  [SyntaxKind.JSDocOverloadTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocOverloadTag(node, _tagName, _typeExpression, _comment);
  },
  [SyntaxKind.JSDocTypedefTag]: (node, visitor) => {
    const _tagName = visitNode(node.tagName, visitor, isIdentifier);
    const _typeExpression = visitNode(node.typeExpression, visitor);
    const _name = visitNode(node.name, visitor, isJSDocFullName);
    const _comment = visitNodes(node.comment, visitor);
    return updateJSDocTypedefTag(node, _tagName, _typeExpression, _name, _comment);
  },
  [SyntaxKind.JSDocSignature]: (node, visitor) => {
    const _typeParameters = visitNodes(node.typeParameters, visitor);
    const _parameters = visitNodes(node.parameters, visitor);
    const _type = visitNode(node.type, visitor, isTypeNode);
    return updateJSDocSignature(node, _typeParameters, _parameters, _type);
  },
  [SyntaxKind.JSDocNameReference]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isEntityName);
    return updateJSDocNameReference(node, _name);
  },
  [SyntaxKind.SourceFile]: (node, visitor) => {
    const _statements = visitNodes(node.statements, visitor);
    const _endOfFileToken = visitNode(node.endOfFileToken, visitor, isEndOfFile);
    return updateSourceFile(node, _statements, _endOfFileToken);
  },
  [SyntaxKind.ModuleDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isModuleName);
    const _body = visitNode(node.body, visitor, isModuleBody);
    return updateModuleDeclaration(node, _modifiers, _name, _body);
  },
  [SyntaxKind.ImportEqualsDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _moduleReference = visitNode(node.moduleReference, visitor, isModuleReference);
    return updateImportEqualsDeclaration(node, _modifiers, _name, _moduleReference);
  },
  [SyntaxKind.ExportDeclaration]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _exportClause = visitNode(node.exportClause, visitor, isNamedExportBindings);
    const _moduleSpecifier = visitNode(node.moduleSpecifier, visitor, isExpression);
    const _attributes = visitNode(node.attributes, visitor, isImportAttributes);
    return updateExportDeclaration(node, _modifiers, _exportClause, _moduleSpecifier, _attributes);
  },
  [SyntaxKind.ImportType]: (node, visitor) => {
    const _argument = visitNode(node.argument, visitor, isTypeNode);
    const _attributes = visitNode(node.attributes, visitor, isImportAttributes);
    const _qualifier = visitNode(node.qualifier, visitor, isEntityName);
    const _typeArguments = visitNodes(node.typeArguments, visitor);
    return updateImportTypeNode(node, _argument, _attributes, _qualifier, _typeArguments);
  },
  [SyntaxKind.ImportClause]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _namedBindings = visitNode(node.namedBindings, visitor, isNamedImportBindings);
    return updateImportClause(node, _name, _namedBindings);
  },
  [SyntaxKind.ImportSpecifier]: (node, visitor) => {
    const _propertyName = visitNode(node.propertyName, visitor, isModuleExportName);
    const _name = visitNode(node.name, visitor, isIdentifier);
    return updateImportSpecifier(node, _propertyName, _name);
  },
  [SyntaxKind.JSDocLink]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isEntityName);
    return updateJSDocLink(node, _name);
  },
  [SyntaxKind.JSDocLinkPlain]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isEntityName);
    return updateJSDocLinkPlain(node, _name);
  },
  [SyntaxKind.JSDocLinkCode]: (node, visitor) => {
    const _name = visitNode(node.name, visitor, isEntityName);
    return updateJSDocLinkCode(node, _name);
  },
  [SyntaxKind.TypeParameter]: (node, visitor) => {
    const _modifiers = visitNodes(node.modifiers, visitor);
    const _name = visitNode(node.name, visitor, isIdentifier);
    const _constraint = visitNode(node.constraint, visitor, isTypeNode);
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _defaultType = visitNode(node.defaultType, visitor, isTypeNode);
    return updateTypeParameterDeclaration(node, _modifiers, _name, _constraint, _expression, _defaultType);
  },
  [SyntaxKind.SyntheticReferenceExpression]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _thisArg = visitNode(node.thisArg, visitor, isExpression);
    return updateSyntheticReferenceExpression(node, _expression, _thisArg);
  },
  [SyntaxKind.JSDocTypeLiteral]: (node, visitor) => {
    const _jsdocPropertyTags = visitNodesArray(node.jsdocPropertyTags, visitor);
    return updateJSDocTypeLiteral(node, _jsdocPropertyTags);
  },
  [SyntaxKind.ForInStatement]: (node, visitor) => {
    const _awaitModifier = visitNode(node.awaitModifier, visitor, isAwaitKeyword);
    const _initializer = visitNode(node.initializer, visitor, isForInitializer);
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateForInStatement(node, _awaitModifier, _initializer, _expression, _statement);
  },
  [SyntaxKind.ForOfStatement]: (node, visitor) => {
    const _awaitModifier = visitNode(node.awaitModifier, visitor, isAwaitKeyword);
    const _initializer = visitNode(node.initializer, visitor, isForInitializer);
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statement = visitNode(node.statement, visitor, isStatement);
    return updateForOfStatement(node, _awaitModifier, _initializer, _expression, _statement);
  },
  [SyntaxKind.CaseClause]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statements = visitNodes(node.statements, visitor);
    return updateCaseClause(node, _expression, _statements);
  },
  [SyntaxKind.DefaultClause]: (node, visitor) => {
    const _expression = visitNode(node.expression, visitor, isExpression);
    const _statements = visitNodes(node.statements, visitor);
    return updateDefaultClause(node, _expression, _statements);
  },
  [SyntaxKind.ObjectBindingPattern]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateObjectBindingPattern(node, _elements);
  },
  [SyntaxKind.ArrayBindingPattern]: (node, visitor) => {
    const _elements = visitNodes(node.elements, visitor);
    return updateArrayBindingPattern(node, _elements);
  },
  [SyntaxKind.JSDocParameterTag]: visitEachChildOfJSDocParameterOrPropertyTag,
  [SyntaxKind.JSDocPropertyTag]: visitEachChildOfJSDocParameterOrPropertyTag
};

// node_modules/typescript/dist/ast/visitor.js
function visitNodeForEachChild(cbNode, node) {
  return node ? cbNode(node) : void 0;
}
function visitNodesForEachChild(cbNode, cbNodes, nodes) {
  if (!nodes)
    return void 0;
  if (cbNodes)
    return cbNodes(nodes);
  for (const node of nodes) {
    const result = cbNode(node);
    if (result)
      return result;
  }
  return void 0;
}
function forEachChildOfJSDocParameterOrPropertyTag(data, cbNode, cbNodes) {
  return visitNodeForEachChild(cbNode, data.tagName) || (data.isNameFirst ? visitNodeForEachChild(cbNode, data.name) || visitNodeForEachChild(cbNode, data.typeExpression) : visitNodeForEachChild(cbNode, data.typeExpression) || visitNodeForEachChild(cbNode, data.name)) || visitNodesForEachChild(cbNode, cbNodes, data.comment);
}
function visitEachChildOfJSDocParameterOrPropertyTag(node, visitor) {
  const _tagName = visitNode(node.tagName, visitor, isIdentifier);
  const _name = visitNode(node.name, visitor, isEntityName);
  const _typeExpression = visitNode(node.typeExpression, visitor, isTypeNode);
  const _comment = visitNodes(node.comment, visitor);
  return node.kind === SyntaxKind.JSDocParameterTag ? updateJSDocParameterTag(node, _tagName, _name, _typeExpression, _comment) : updateJSDocPropertyTag(node, _tagName, _name, _typeExpression, _comment);
}

// node_modules/typescript/dist/ast/factory.generated.js
var NodeObject = class {
  kind;
  flags = 0;
  pos = -1;
  end = -1;
  parent = void 0;
  _data;
  constructor(kind, data) {
    this.kind = kind;
    this._data = data;
  }
  get ambientModuleNames() {
    return this._data?.ambientModuleNames;
  }
  get argument() {
    return this._data?.argument;
  }
  get argumentExpression() {
    return this._data?.argumentExpression;
  }
  get arguments() {
    return this._data?.arguments;
  }
  get assertsModifier() {
    return this._data?.assertsModifier;
  }
  get asteriskToken() {
    return this._data?.asteriskToken;
  }
  get attributes() {
    return this._data?.attributes;
  }
  get awaitModifier() {
    return this._data?.awaitModifier;
  }
  get block() {
    return this._data?.block;
  }
  get body() {
    return this._data?.body;
  }
  get caseBlock() {
    return this._data?.caseBlock;
  }
  get catchClause() {
    return this._data?.catchClause;
  }
  get checkType() {
    return this._data?.checkType;
  }
  get children() {
    return this._data?.children;
  }
  get className() {
    return this._data?.className;
  }
  get clauses() {
    return this._data?.clauses;
  }
  get closingElement() {
    return this._data?.closingElement;
  }
  get closingFragment() {
    return this._data?.closingFragment;
  }
  get colonToken() {
    return this._data?.colonToken;
  }
  get comment() {
    return this._data?.comment;
  }
  get condition() {
    return this._data?.condition;
  }
  get constraint() {
    return this._data?.constraint;
  }
  get containsOnlyTriviaWhiteSpaces() {
    return this._data?.containsOnlyTriviaWhiteSpaces;
  }
  get declarationList() {
    return this._data?.declarationList;
  }
  get declarations() {
    return this._data?.declarations;
  }
  get defaultType() {
    return this._data?.defaultType;
  }
  get dotDotDotToken() {
    return this._data?.dotDotDotToken;
  }
  get elementType() {
    return this._data?.elementType;
  }
  get elements() {
    return this._data?.elements;
  }
  get elseStatement() {
    return this._data?.elseStatement;
  }
  get endOfFileToken() {
    return this._data?.endOfFileToken;
  }
  get equalsGreaterThanToken() {
    return this._data?.equalsGreaterThanToken;
  }
  get equalsToken() {
    return this._data?.equalsToken;
  }
  get exclamationToken() {
    return this._data?.exclamationToken;
  }
  get exportClause() {
    return this._data?.exportClause;
  }
  get exprName() {
    return this._data?.exprName;
  }
  get expression() {
    return this._data?.expression;
  }
  get extendsType() {
    return this._data?.extendsType;
  }
  get externalModuleIndicator() {
    return this._data?.externalModuleIndicator;
  }
  get falseType() {
    return this._data?.falseType;
  }
  get fileName() {
    return this._data?.fileName;
  }
  get finallyBlock() {
    return this._data?.finallyBlock;
  }
  get head() {
    return this._data?.head;
  }
  get heritageClauses() {
    return this._data?.heritageClauses;
  }
  get importClause() {
    return this._data?.importClause;
  }
  get imports() {
    return this._data?.imports;
  }
  get incrementor() {
    return this._data?.incrementor;
  }
  get indexType() {
    return this._data?.indexType;
  }
  get initializer() {
    return this._data?.initializer;
  }
  get isArrayType() {
    return this._data?.isArrayType;
  }
  get isBracketed() {
    return this._data?.isBracketed;
  }
  get isDeclarationFile() {
    return this._data?.isDeclarationFile;
  }
  get isExportEquals() {
    return this._data?.isExportEquals;
  }
  get isNameFirst() {
    return this._data?.isNameFirst;
  }
  get isSpread() {
    return this._data?.isSpread;
  }
  get isTypeOf() {
    return this._data?.isTypeOf;
  }
  get isTypeOnly() {
    return this._data?.isTypeOnly;
  }
  get jsdocPropertyTags() {
    return this._data?.jsdocPropertyTags;
  }
  get keyword() {
    return this._data?.keyword;
  }
  get keywordToken() {
    return this._data?.keywordToken;
  }
  get label() {
    return this._data?.label;
  }
  get languageVariant() {
    return this._data?.languageVariant;
  }
  get left() {
    return this._data?.left;
  }
  get libReferenceDirectives() {
    return this._data?.libReferenceDirectives;
  }
  get literal() {
    return this._data?.literal;
  }
  get members() {
    return this._data?.members;
  }
  get modifiers() {
    return this._data?.modifiers;
  }
  get moduleAugmentations() {
    return this._data?.moduleAugmentations;
  }
  get moduleReference() {
    return this._data?.moduleReference;
  }
  get moduleSpecifier() {
    return this._data?.moduleSpecifier;
  }
  get multiLine() {
    return this._data?.multiLine;
  }
  get name() {
    return this._data?.name;
  }
  get nameExpression() {
    return this._data?.nameExpression;
  }
  get nameType() {
    return this._data?.nameType;
  }
  get namedBindings() {
    return this._data?.namedBindings;
  }
  get namespace() {
    return this._data?.namespace;
  }
  get objectAssignmentInitializer() {
    return this._data?.objectAssignmentInitializer;
  }
  get objectType() {
    return this._data?.objectType;
  }
  get openingElement() {
    return this._data?.openingElement;
  }
  get openingFragment() {
    return this._data?.openingFragment;
  }
  get operand() {
    return this._data?.operand;
  }
  get operator() {
    return this._data?.operator;
  }
  get operatorToken() {
    return this._data?.operatorToken;
  }
  get parameterName() {
    return this._data?.parameterName;
  }
  get parameters() {
    return this._data?.parameters;
  }
  get path() {
    return this._data?.path;
  }
  get phaseModifier() {
    return this._data?.phaseModifier;
  }
  get postfixToken() {
    return this._data?.postfixToken;
  }
  get properties() {
    return this._data?.properties;
  }
  get propertyName() {
    return this._data?.propertyName;
  }
  get qualifier() {
    return this._data?.qualifier;
  }
  get questionDotToken() {
    return this._data?.questionDotToken;
  }
  get questionToken() {
    return this._data?.questionToken;
  }
  get rawText() {
    return this._data?.rawText;
  }
  get readonlyToken() {
    return this._data?.readonlyToken;
  }
  get referencedFiles() {
    return this._data?.referencedFiles;
  }
  get right() {
    return this._data?.right;
  }
  get scriptKind() {
    return this._data?.scriptKind;
  }
  get statement() {
    return this._data?.statement;
  }
  get statements() {
    return this._data?.statements;
  }
  get tag() {
    return this._data?.tag;
  }
  get tagName() {
    return this._data?.tagName;
  }
  get tags() {
    return this._data?.tags;
  }
  get template() {
    return this._data?.template;
  }
  get templateFlags() {
    return this._data?.templateFlags;
  }
  get templateSpans() {
    return this._data?.templateSpans;
  }
  get text() {
    return this._data?.text;
  }
  get thenStatement() {
    return this._data?.thenStatement;
  }
  get thisArg() {
    return this._data?.thisArg;
  }
  get token() {
    return this._data?.token;
  }
  get tokenCache() {
    return this._data?.tokenCache;
  }
  get tokenFlags() {
    return this._data?.tokenFlags;
  }
  get trueType() {
    return this._data?.trueType;
  }
  get tryBlock() {
    return this._data?.tryBlock;
  }
  get tupleNameSource() {
    return this._data?.tupleNameSource;
  }
  get type() {
    return this._data?.type;
  }
  get typeArguments() {
    return this._data?.typeArguments;
  }
  get typeExpression() {
    return this._data?.typeExpression;
  }
  get typeName() {
    return this._data?.typeName;
  }
  get typeParameter() {
    return this._data?.typeParameter;
  }
  get typeParameters() {
    return this._data?.typeParameters;
  }
  get typeReferenceDirectives() {
    return this._data?.typeReferenceDirectives;
  }
  get types() {
    return this._data?.types;
  }
  get value() {
    return this._data?.value;
  }
  get variableDeclaration() {
    return this._data?.variableDeclaration;
  }
  get whenFalse() {
    return this._data?.whenFalse;
  }
  get whenTrue() {
    return this._data?.whenTrue;
  }
  forEachChild(visitor, visitArray) {
    const fn = forEachChildTable[this.kind];
    return fn ? fn(this._data, visitor, visitArray) : void 0;
  }
  getSourceFile() {
    let node = this;
    while (node.parent)
      node = node.parent;
    return node;
  }
  getStart(sourceFile, includeJsDocComment) {
    return getTokenPosOfNode(this, sourceFile ?? this.getSourceFile(), includeJsDocComment);
  }
  getFullStart() {
    return this.pos;
  }
  getEnd() {
    return this.end;
  }
  getWidth(sourceFile) {
    return this.getEnd() - this.getStart(sourceFile);
  }
  getFullWidth() {
    return this.end - this.pos;
  }
  getLeadingTriviaWidth(sourceFile) {
    return this.getStart(sourceFile) - this.pos;
  }
  getFullText(sourceFile) {
    return (sourceFile ?? this.getSourceFile()).text.substring(this.pos, this.end);
  }
  getText(sourceFile) {
    sourceFile ??= this.getSourceFile();
    return sourceFile.text.substring(this.getStart(sourceFile), this.end);
  }
};
function isNodeArray(array) {
  return "pos" in array && "end" in array;
}
function createNodeArray(elements, pos = -1, end = -1) {
  if (isNodeArray(elements))
    return elements;
  const arr = elements.slice();
  arr.pos = pos;
  arr.end = end;
  return arr;
}
function cloneNode(node) {
  const data = cloneNodeData(node);
  const clone = new NodeObject(node.kind, data);
  clone.flags = node.flags;
  clone.pos = node.pos;
  clone.end = node.end;
  return clone;
}
function cloneNodeData(node) {
  const n = node;
  switch (node.kind) {
    case SyntaxKind.Identifier:
      return { text: n.text };
    case SyntaxKind.PrivateIdentifier:
      return { text: n.text };
    case SyntaxKind.QualifiedName:
      return { left: n.left, right: n.right };
    case SyntaxKind.ComputedPropertyName:
      return { expression: n.expression };
    case SyntaxKind.Decorator:
      return { expression: n.expression };
    case SyntaxKind.IfStatement:
      return { expression: n.expression, thenStatement: n.thenStatement, elseStatement: n.elseStatement };
    case SyntaxKind.DoStatement:
      return { statement: n.statement, expression: n.expression };
    case SyntaxKind.WhileStatement:
      return { expression: n.expression, statement: n.statement };
    case SyntaxKind.ForStatement:
      return { initializer: n.initializer, condition: n.condition, incrementor: n.incrementor, statement: n.statement };
    case SyntaxKind.BreakStatement:
      return { label: n.label };
    case SyntaxKind.ContinueStatement:
      return { label: n.label };
    case SyntaxKind.ReturnStatement:
      return { expression: n.expression };
    case SyntaxKind.WithStatement:
      return { expression: n.expression, statement: n.statement };
    case SyntaxKind.SwitchStatement:
      return { expression: n.expression, caseBlock: n.caseBlock };
    case SyntaxKind.CaseBlock:
      return { clauses: n.clauses };
    case SyntaxKind.ThrowStatement:
      return { expression: n.expression };
    case SyntaxKind.TryStatement:
      return { tryBlock: n.tryBlock, catchClause: n.catchClause, finallyBlock: n.finallyBlock };
    case SyntaxKind.CatchClause:
      return { variableDeclaration: n.variableDeclaration, block: n.block };
    case SyntaxKind.LabeledStatement:
      return { label: n.label, statement: n.statement };
    case SyntaxKind.ExpressionStatement:
      return { expression: n.expression };
    case SyntaxKind.Block:
      return { statements: n.statements, multiLine: n.multiLine };
    case SyntaxKind.VariableStatement:
      return { modifiers: n.modifiers, declarationList: n.declarationList };
    case SyntaxKind.VariableDeclaration:
      return { name: n.name, exclamationToken: n.exclamationToken, type: n.type, initializer: n.initializer };
    case SyntaxKind.VariableDeclarationList:
      return { declarations: n.declarations };
    case SyntaxKind.Parameter:
      return { modifiers: n.modifiers, dotDotDotToken: n.dotDotDotToken, name: n.name, questionToken: n.questionToken, type: n.type, initializer: n.initializer };
    case SyntaxKind.BindingElement:
      return { dotDotDotToken: n.dotDotDotToken, propertyName: n.propertyName, name: n.name, initializer: n.initializer };
    case SyntaxKind.MissingDeclaration:
      return { modifiers: n.modifiers };
    case SyntaxKind.FunctionDeclaration:
      return { modifiers: n.modifiers, asteriskToken: n.asteriskToken, name: n.name, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.ClassDeclaration:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, heritageClauses: n.heritageClauses, members: n.members };
    case SyntaxKind.ClassExpression:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, heritageClauses: n.heritageClauses, members: n.members };
    case SyntaxKind.HeritageClause:
      return { token: n.token, types: n.types };
    case SyntaxKind.InterfaceDeclaration:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, heritageClauses: n.heritageClauses, members: n.members };
    case SyntaxKind.TypeAliasDeclaration:
    case SyntaxKind.JSTypeAliasDeclaration:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, type: n.type };
    case SyntaxKind.EnumMember:
      return { name: n.name, initializer: n.initializer };
    case SyntaxKind.EnumDeclaration:
      return { modifiers: n.modifiers, name: n.name, members: n.members };
    case SyntaxKind.ModuleBlock:
      return { statements: n.statements };
    case SyntaxKind.ImportDeclaration:
    case SyntaxKind.JSImportDeclaration:
      return { modifiers: n.modifiers, importClause: n.importClause, moduleSpecifier: n.moduleSpecifier, attributes: n.attributes };
    case SyntaxKind.ExternalModuleReference:
      return { expression: n.expression };
    case SyntaxKind.NamespaceImport:
      return { name: n.name };
    case SyntaxKind.NamedImports:
      return { elements: n.elements };
    case SyntaxKind.ExportAssignment:
      return { modifiers: n.modifiers, isExportEquals: n.isExportEquals, type: n.type, expression: n.expression };
    case SyntaxKind.NamespaceExportDeclaration:
      return { modifiers: n.modifiers, name: n.name };
    case SyntaxKind.NamespaceExport:
      return { name: n.name };
    case SyntaxKind.NamedExports:
      return { elements: n.elements };
    case SyntaxKind.ExportSpecifier:
      return { isTypeOnly: n.isTypeOnly, propertyName: n.propertyName, name: n.name };
    case SyntaxKind.CallSignature:
      return { typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.ConstructSignature:
      return { typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.Constructor:
      return { modifiers: n.modifiers, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.GetAccessor:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.SetAccessor:
      return { modifiers: n.modifiers, name: n.name, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.IndexSignature:
      return { modifiers: n.modifiers, parameters: n.parameters, type: n.type };
    case SyntaxKind.MethodSignature:
      return { modifiers: n.modifiers, name: n.name, postfixToken: n.postfixToken, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.MethodDeclaration:
      return { modifiers: n.modifiers, asteriskToken: n.asteriskToken, name: n.name, postfixToken: n.postfixToken, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.PropertySignature:
      return { modifiers: n.modifiers, name: n.name, postfixToken: n.postfixToken, type: n.type, initializer: n.initializer };
    case SyntaxKind.PropertyDeclaration:
      return { modifiers: n.modifiers, name: n.name, postfixToken: n.postfixToken, type: n.type, initializer: n.initializer };
    case SyntaxKind.ClassStaticBlockDeclaration:
      return { modifiers: n.modifiers, body: n.body };
    case SyntaxKind.StringLiteral:
      return { text: n.text, tokenFlags: n.tokenFlags };
    case SyntaxKind.NumericLiteral:
      return { text: n.text, tokenFlags: n.tokenFlags };
    case SyntaxKind.BigIntLiteral:
      return { text: n.text, tokenFlags: n.tokenFlags };
    case SyntaxKind.RegularExpressionLiteral:
      return { text: n.text, tokenFlags: n.tokenFlags };
    case SyntaxKind.NoSubstitutionTemplateLiteral:
      return { text: n.text, templateFlags: n.templateFlags };
    case SyntaxKind.BinaryExpression:
      return { modifiers: n.modifiers, left: n.left, type: n.type, operatorToken: n.operatorToken, right: n.right };
    case SyntaxKind.PrefixUnaryExpression:
      return { operator: n.operator, operand: n.operand };
    case SyntaxKind.PostfixUnaryExpression:
      return { operand: n.operand, operator: n.operator };
    case SyntaxKind.YieldExpression:
      return { asteriskToken: n.asteriskToken, expression: n.expression };
    case SyntaxKind.ArrowFunction:
      return { modifiers: n.modifiers, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, equalsGreaterThanToken: n.equalsGreaterThanToken, body: n.body };
    case SyntaxKind.FunctionExpression:
      return { modifiers: n.modifiers, asteriskToken: n.asteriskToken, name: n.name, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type, body: n.body };
    case SyntaxKind.AsExpression:
      return { expression: n.expression, type: n.type };
    case SyntaxKind.SatisfiesExpression:
      return { expression: n.expression, type: n.type };
    case SyntaxKind.ConditionalExpression:
      return { condition: n.condition, questionToken: n.questionToken, whenTrue: n.whenTrue, colonToken: n.colonToken, whenFalse: n.whenFalse };
    case SyntaxKind.PropertyAccessExpression:
      return { expression: n.expression, questionDotToken: n.questionDotToken, name: n.name };
    case SyntaxKind.ElementAccessExpression:
      return { expression: n.expression, questionDotToken: n.questionDotToken, argumentExpression: n.argumentExpression };
    case SyntaxKind.CallExpression:
      return { expression: n.expression, questionDotToken: n.questionDotToken, typeArguments: n.typeArguments, arguments: n.arguments };
    case SyntaxKind.NewExpression:
      return { expression: n.expression, typeArguments: n.typeArguments, arguments: n.arguments };
    case SyntaxKind.MetaProperty:
      return { keywordToken: n.keywordToken, name: n.name };
    case SyntaxKind.NonNullExpression:
      return { expression: n.expression };
    case SyntaxKind.SpreadElement:
      return { expression: n.expression };
    case SyntaxKind.TemplateExpression:
      return { head: n.head, templateSpans: n.templateSpans };
    case SyntaxKind.TemplateSpan:
      return { expression: n.expression, literal: n.literal };
    case SyntaxKind.TaggedTemplateExpression:
      return { tag: n.tag, questionDotToken: n.questionDotToken, typeArguments: n.typeArguments, template: n.template };
    case SyntaxKind.ParenthesizedExpression:
      return { expression: n.expression };
    case SyntaxKind.ArrayLiteralExpression:
      return { elements: n.elements, multiLine: n.multiLine };
    case SyntaxKind.ObjectLiteralExpression:
      return { properties: n.properties, multiLine: n.multiLine };
    case SyntaxKind.SpreadAssignment:
      return { expression: n.expression };
    case SyntaxKind.PropertyAssignment:
      return { modifiers: n.modifiers, name: n.name, postfixToken: n.postfixToken, type: n.type, initializer: n.initializer };
    case SyntaxKind.ShorthandPropertyAssignment:
      return { modifiers: n.modifiers, name: n.name, postfixToken: n.postfixToken, type: n.type, equalsToken: n.equalsToken, objectAssignmentInitializer: n.objectAssignmentInitializer };
    case SyntaxKind.DeleteExpression:
      return { expression: n.expression };
    case SyntaxKind.TypeOfExpression:
      return { expression: n.expression };
    case SyntaxKind.VoidExpression:
      return { expression: n.expression };
    case SyntaxKind.AwaitExpression:
      return { expression: n.expression };
    case SyntaxKind.TypeAssertionExpression:
      return { type: n.type, expression: n.expression };
    case SyntaxKind.UnionType:
      return { types: n.types };
    case SyntaxKind.IntersectionType:
      return { types: n.types };
    case SyntaxKind.ConditionalType:
      return { checkType: n.checkType, extendsType: n.extendsType, trueType: n.trueType, falseType: n.falseType };
    case SyntaxKind.TypeOperator:
      return { operator: n.operator, type: n.type };
    case SyntaxKind.InferType:
      return { typeParameter: n.typeParameter };
    case SyntaxKind.ArrayType:
      return { elementType: n.elementType };
    case SyntaxKind.IndexedAccessType:
      return { objectType: n.objectType, indexType: n.indexType };
    case SyntaxKind.TypeReference:
      return { typeName: n.typeName, typeArguments: n.typeArguments };
    case SyntaxKind.ExpressionWithTypeArguments:
      return { expression: n.expression, typeArguments: n.typeArguments };
    case SyntaxKind.LiteralType:
      return { literal: n.literal };
    case SyntaxKind.TypePredicate:
      return { assertsModifier: n.assertsModifier, parameterName: n.parameterName, type: n.type };
    case SyntaxKind.ImportAttribute:
      return { name: n.name, value: n.value };
    case SyntaxKind.ImportAttributes:
      return { token: n.token, attributes: n.attributes, multiLine: n.multiLine };
    case SyntaxKind.TypeQuery:
      return { exprName: n.exprName, typeArguments: n.typeArguments };
    case SyntaxKind.MappedType:
      return { readonlyToken: n.readonlyToken, typeParameter: n.typeParameter, nameType: n.nameType, questionToken: n.questionToken, type: n.type, members: n.members };
    case SyntaxKind.TypeLiteral:
      return { members: n.members };
    case SyntaxKind.TupleType:
      return { elements: n.elements };
    case SyntaxKind.NamedTupleMember:
      return { dotDotDotToken: n.dotDotDotToken, name: n.name, questionToken: n.questionToken, type: n.type };
    case SyntaxKind.OptionalType:
      return { type: n.type };
    case SyntaxKind.RestType:
      return { type: n.type };
    case SyntaxKind.ParenthesizedType:
      return { type: n.type };
    case SyntaxKind.FunctionType:
      return { typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.ConstructorType:
      return { modifiers: n.modifiers, typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.TemplateHead:
      return { text: n.text, rawText: n.rawText, templateFlags: n.templateFlags };
    case SyntaxKind.TemplateMiddle:
      return { text: n.text, rawText: n.rawText, templateFlags: n.templateFlags };
    case SyntaxKind.TemplateTail:
      return { text: n.text, rawText: n.rawText, templateFlags: n.templateFlags };
    case SyntaxKind.TemplateLiteralType:
      return { head: n.head, templateSpans: n.templateSpans };
    case SyntaxKind.TemplateLiteralTypeSpan:
      return { type: n.type, literal: n.literal };
    case SyntaxKind.SyntheticExpression:
      return { type: n.type, isSpread: n.isSpread, tupleNameSource: n.tupleNameSource };
    case SyntaxKind.PartiallyEmittedExpression:
      return { expression: n.expression };
    case SyntaxKind.JsxElement:
      return { openingElement: n.openingElement, children: n.children, closingElement: n.closingElement };
    case SyntaxKind.JsxAttributes:
      return { properties: n.properties };
    case SyntaxKind.JsxNamespacedName:
      return { namespace: n.namespace, name: n.name };
    case SyntaxKind.JsxOpeningElement:
      return { tagName: n.tagName, typeArguments: n.typeArguments, attributes: n.attributes };
    case SyntaxKind.JsxSelfClosingElement:
      return { tagName: n.tagName, typeArguments: n.typeArguments, attributes: n.attributes };
    case SyntaxKind.JsxFragment:
      return { openingFragment: n.openingFragment, children: n.children, closingFragment: n.closingFragment };
    case SyntaxKind.JsxAttribute:
      return { name: n.name, initializer: n.initializer };
    case SyntaxKind.JsxSpreadAttribute:
      return { expression: n.expression };
    case SyntaxKind.JsxClosingElement:
      return { tagName: n.tagName };
    case SyntaxKind.JsxExpression:
      return { dotDotDotToken: n.dotDotDotToken, expression: n.expression };
    case SyntaxKind.JsxText:
      return { text: n.text, containsOnlyTriviaWhiteSpaces: n.containsOnlyTriviaWhiteSpaces };
    case SyntaxKind.SyntaxList:
      return { children: n.children };
    case SyntaxKind.JSDoc:
      return { comment: n.comment, tags: n.tags };
    case SyntaxKind.JSDocTypeExpression:
      return { type: n.type };
    case SyntaxKind.JSDocNonNullableType:
      return { type: n.type };
    case SyntaxKind.JSDocNullableType:
      return { type: n.type };
    case SyntaxKind.JSDocVariadicType:
      return { type: n.type };
    case SyntaxKind.JSDocOptionalType:
      return { type: n.type };
    case SyntaxKind.JSDocTypeTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocUnknownTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocTemplateTag:
      return { tagName: n.tagName, constraint: n.constraint, typeParameters: n.typeParameters, comment: n.comment };
    case SyntaxKind.JSDocReturnTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocPublicTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocPrivateTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocProtectedTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocReadonlyTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocOverrideTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocDeprecatedTag:
      return { tagName: n.tagName, comment: n.comment };
    case SyntaxKind.JSDocSeeTag:
      return { tagName: n.tagName, nameExpression: n.nameExpression, comment: n.comment };
    case SyntaxKind.JSDocImplementsTag:
      return { tagName: n.tagName, className: n.className, comment: n.comment };
    case SyntaxKind.JSDocAugmentsTag:
      return { tagName: n.tagName, className: n.className, comment: n.comment };
    case SyntaxKind.JSDocSatisfiesTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocThrowsTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocThisTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocImportTag:
      return { tagName: n.tagName, importClause: n.importClause, moduleSpecifier: n.moduleSpecifier, attributes: n.attributes, comment: n.comment };
    case SyntaxKind.JSDocCallbackTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, name: n.name, comment: n.comment };
    case SyntaxKind.JSDocOverloadTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, comment: n.comment };
    case SyntaxKind.JSDocTypedefTag:
      return { tagName: n.tagName, typeExpression: n.typeExpression, name: n.name, comment: n.comment };
    case SyntaxKind.JSDocSignature:
      return { typeParameters: n.typeParameters, parameters: n.parameters, type: n.type };
    case SyntaxKind.JSDocNameReference:
      return { name: n.name };
    case SyntaxKind.ModuleDeclaration:
      return { modifiers: n.modifiers, keyword: n.keyword, name: n.name, body: n.body };
    case SyntaxKind.ImportEqualsDeclaration:
      return { modifiers: n.modifiers, isTypeOnly: n.isTypeOnly, name: n.name, moduleReference: n.moduleReference };
    case SyntaxKind.ExportDeclaration:
      return { modifiers: n.modifiers, isTypeOnly: n.isTypeOnly, exportClause: n.exportClause, moduleSpecifier: n.moduleSpecifier, attributes: n.attributes };
    case SyntaxKind.ImportType:
      return { isTypeOf: n.isTypeOf, argument: n.argument, attributes: n.attributes, qualifier: n.qualifier, typeArguments: n.typeArguments };
    case SyntaxKind.ImportClause:
      return { phaseModifier: n.phaseModifier, name: n.name, namedBindings: n.namedBindings };
    case SyntaxKind.ImportSpecifier:
      return { isTypeOnly: n.isTypeOnly, propertyName: n.propertyName, name: n.name };
    case SyntaxKind.JSDocText:
      return { text: n.text };
    case SyntaxKind.JSDocLink:
      return { name: n.name, text: n.text };
    case SyntaxKind.JSDocLinkPlain:
      return { name: n.name, text: n.text };
    case SyntaxKind.JSDocLinkCode:
      return { name: n.name, text: n.text };
    case SyntaxKind.TypeParameter:
      return { modifiers: n.modifiers, name: n.name, constraint: n.constraint, expression: n.expression, defaultType: n.defaultType };
    case SyntaxKind.SyntheticReferenceExpression:
      return { expression: n.expression, thisArg: n.thisArg };
    case SyntaxKind.JSDocTypeLiteral:
      return { jsdocPropertyTags: n.jsdocPropertyTags, isArrayType: n.isArrayType };
    case SyntaxKind.ForInStatement:
      return { awaitModifier: n.awaitModifier, initializer: n.initializer, expression: n.expression, statement: n.statement };
    case SyntaxKind.ForOfStatement:
      return { awaitModifier: n.awaitModifier, initializer: n.initializer, expression: n.expression, statement: n.statement };
    case SyntaxKind.CaseClause:
      return { expression: n.expression, statements: n.statements };
    case SyntaxKind.DefaultClause:
      return { expression: n.expression, statements: n.statements };
    case SyntaxKind.ObjectBindingPattern:
      return { elements: n.elements };
    case SyntaxKind.ArrayBindingPattern:
      return { elements: n.elements };
    case SyntaxKind.JSDocParameterTag:
      return { tagName: n.tagName, name: n.name, isBracketed: n.isBracketed, typeExpression: n.typeExpression, isNameFirst: n.isNameFirst, comment: n.comment };
    case SyntaxKind.JSDocPropertyTag:
      return { tagName: n.tagName, name: n.name, isBracketed: n.isBracketed, typeExpression: n.typeExpression, isNameFirst: n.isNameFirst, comment: n.comment };
    case SyntaxKind.SourceFile:
      return cloneSourceFileData(n);
    default:
      return void 0;
  }
}
var forEachChildTable = {
  [SyntaxKind.QualifiedName]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.left) || visitNode2(cbNode, data.right),
  [SyntaxKind.ComputedPropertyName]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.Decorator]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.IfStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.thenStatement) || visitNode2(cbNode, data.elseStatement),
  [SyntaxKind.DoStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.statement) || visitNode2(cbNode, data.expression),
  [SyntaxKind.WhileStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.statement),
  [SyntaxKind.ForStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.initializer) || visitNode2(cbNode, data.condition) || visitNode2(cbNode, data.incrementor) || visitNode2(cbNode, data.statement),
  [SyntaxKind.BreakStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.label),
  [SyntaxKind.ContinueStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.label),
  [SyntaxKind.ReturnStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.WithStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.statement),
  [SyntaxKind.SwitchStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.caseBlock),
  [SyntaxKind.CaseBlock]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.clauses),
  [SyntaxKind.ThrowStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.TryStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tryBlock) || visitNode2(cbNode, data.catchClause) || visitNode2(cbNode, data.finallyBlock),
  [SyntaxKind.CatchClause]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.variableDeclaration) || visitNode2(cbNode, data.block),
  [SyntaxKind.LabeledStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.label) || visitNode2(cbNode, data.statement),
  [SyntaxKind.ExpressionStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.Block]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.statements),
  [SyntaxKind.VariableStatement]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.declarationList),
  [SyntaxKind.VariableDeclaration]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name) || visitNode2(cbNode, data.exclamationToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.VariableDeclarationList]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.declarations),
  [SyntaxKind.Parameter]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.dotDotDotToken) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.questionToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.BindingElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.dotDotDotToken) || visitNode2(cbNode, data.propertyName) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.MissingDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers),
  [SyntaxKind.FunctionDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.asteriskToken) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.ClassDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.heritageClauses) || visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.ClassExpression]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.heritageClauses) || visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.HeritageClause]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.types),
  [SyntaxKind.InterfaceDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.heritageClauses) || visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.TypeAliasDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.JSTypeAliasDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.EnumMember]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.EnumDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.ModuleBlock]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.statements),
  [SyntaxKind.ImportDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.importClause) || visitNode2(cbNode, data.moduleSpecifier) || visitNode2(cbNode, data.attributes),
  [SyntaxKind.JSImportDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.importClause) || visitNode2(cbNode, data.moduleSpecifier) || visitNode2(cbNode, data.attributes),
  [SyntaxKind.ExternalModuleReference]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.NamespaceImport]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.NamedImports]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.ExportAssignment]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.expression),
  [SyntaxKind.NamespaceExportDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name),
  [SyntaxKind.NamespaceExport]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.NamedExports]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.ExportSpecifier]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.propertyName) || visitNode2(cbNode, data.name),
  [SyntaxKind.CallSignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.ConstructSignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.Constructor]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.GetAccessor]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.SetAccessor]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.IndexSignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.MethodSignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.MethodDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.asteriskToken) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.PropertySignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.PropertyDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.ClassStaticBlockDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.body),
  [SyntaxKind.BinaryExpression]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.left) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.operatorToken) || visitNode2(cbNode, data.right),
  [SyntaxKind.PrefixUnaryExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.operand),
  [SyntaxKind.PostfixUnaryExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.operand),
  [SyntaxKind.YieldExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.asteriskToken) || visitNode2(cbNode, data.expression),
  [SyntaxKind.ArrowFunction]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.equalsGreaterThanToken) || visitNode2(cbNode, data.body),
  [SyntaxKind.FunctionExpression]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.asteriskToken) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.body),
  [SyntaxKind.AsExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.type),
  [SyntaxKind.SatisfiesExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.type),
  [SyntaxKind.ConditionalExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.condition) || visitNode2(cbNode, data.questionToken) || visitNode2(cbNode, data.whenTrue) || visitNode2(cbNode, data.colonToken) || visitNode2(cbNode, data.whenFalse),
  [SyntaxKind.PropertyAccessExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.questionDotToken) || visitNode2(cbNode, data.name),
  [SyntaxKind.ElementAccessExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.questionDotToken) || visitNode2(cbNode, data.argumentExpression),
  [SyntaxKind.CallExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.questionDotToken) || visitNodes2(cbNode, cbNodes, data.typeArguments) || visitNodes2(cbNode, cbNodes, data.arguments),
  [SyntaxKind.NewExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNodes2(cbNode, cbNodes, data.typeArguments) || visitNodes2(cbNode, cbNodes, data.arguments),
  [SyntaxKind.MetaProperty]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.NonNullExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.SpreadElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.TemplateExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.head) || visitNodes2(cbNode, cbNodes, data.templateSpans),
  [SyntaxKind.TemplateSpan]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.literal),
  [SyntaxKind.TaggedTemplateExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tag) || visitNode2(cbNode, data.questionDotToken) || visitNodes2(cbNode, cbNodes, data.typeArguments) || visitNode2(cbNode, data.template),
  [SyntaxKind.ParenthesizedExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.ArrayLiteralExpression]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.ObjectLiteralExpression]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.properties),
  [SyntaxKind.SpreadAssignment]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.PropertyAssignment]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.ShorthandPropertyAssignment]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.postfixToken) || visitNode2(cbNode, data.type) || visitNode2(cbNode, data.equalsToken) || visitNode2(cbNode, data.objectAssignmentInitializer),
  [SyntaxKind.DeleteExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.TypeOfExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.VoidExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.AwaitExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.TypeAssertionExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type) || visitNode2(cbNode, data.expression),
  [SyntaxKind.UnionType]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.types),
  [SyntaxKind.IntersectionType]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.types),
  [SyntaxKind.ConditionalType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.checkType) || visitNode2(cbNode, data.extendsType) || visitNode2(cbNode, data.trueType) || visitNode2(cbNode, data.falseType),
  [SyntaxKind.TypeOperator]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.InferType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.typeParameter),
  [SyntaxKind.ArrayType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.elementType),
  [SyntaxKind.IndexedAccessType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.objectType) || visitNode2(cbNode, data.indexType),
  [SyntaxKind.TypeReference]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.typeName) || visitNodes2(cbNode, cbNodes, data.typeArguments),
  [SyntaxKind.ExpressionWithTypeArguments]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNodes2(cbNode, cbNodes, data.typeArguments),
  [SyntaxKind.LiteralType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.literal),
  [SyntaxKind.TypePredicate]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.assertsModifier) || visitNode2(cbNode, data.parameterName) || visitNode2(cbNode, data.type),
  [SyntaxKind.ImportAttribute]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name) || visitNode2(cbNode, data.value),
  [SyntaxKind.ImportAttributes]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.attributes),
  [SyntaxKind.TypeQuery]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.exprName) || visitNodes2(cbNode, cbNodes, data.typeArguments),
  [SyntaxKind.MappedType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.readonlyToken) || visitNode2(cbNode, data.typeParameter) || visitNode2(cbNode, data.nameType) || visitNode2(cbNode, data.questionToken) || visitNode2(cbNode, data.type) || visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.TypeLiteral]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.members),
  [SyntaxKind.TupleType]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.NamedTupleMember]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.dotDotDotToken) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.questionToken) || visitNode2(cbNode, data.type),
  [SyntaxKind.OptionalType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.RestType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.ParenthesizedType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.FunctionType]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.ConstructorType]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.TemplateLiteralType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.head) || visitNodes2(cbNode, cbNodes, data.templateSpans),
  [SyntaxKind.TemplateLiteralTypeSpan]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type) || visitNode2(cbNode, data.literal),
  [SyntaxKind.SyntheticExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tupleNameSource),
  [SyntaxKind.PartiallyEmittedExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.JsxElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.openingElement) || visitNodes2(cbNode, cbNodes, data.children) || visitNode2(cbNode, data.closingElement),
  [SyntaxKind.JsxAttributes]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.properties),
  [SyntaxKind.JsxNamespacedName]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.namespace) || visitNode2(cbNode, data.name),
  [SyntaxKind.JsxOpeningElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.typeArguments) || visitNode2(cbNode, data.attributes),
  [SyntaxKind.JsxSelfClosingElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.typeArguments) || visitNode2(cbNode, data.attributes),
  [SyntaxKind.JsxFragment]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.openingFragment) || visitNodes2(cbNode, cbNodes, data.children) || visitNode2(cbNode, data.closingFragment),
  [SyntaxKind.JsxAttribute]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name) || visitNode2(cbNode, data.initializer),
  [SyntaxKind.JsxSpreadAttribute]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression),
  [SyntaxKind.JsxClosingElement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName),
  [SyntaxKind.JsxExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.dotDotDotToken) || visitNode2(cbNode, data.expression),
  [SyntaxKind.SyntaxList]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.children),
  [SyntaxKind.JSDoc]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.comment) || visitNodes2(cbNode, cbNodes, data.tags),
  [SyntaxKind.JSDocTypeExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocNonNullableType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocNullableType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocVariadicType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocOptionalType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocTypeTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocUnknownTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocTemplateTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.constraint) || visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocReturnTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocPublicTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocPrivateTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocProtectedTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocReadonlyTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocOverrideTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocDeprecatedTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocSeeTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.nameExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocImplementsTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.className) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocAugmentsTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.className) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocSatisfiesTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocThrowsTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocThisTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocImportTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.importClause) || visitNode2(cbNode, data.moduleSpecifier) || visitNode2(cbNode, data.attributes) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocCallbackTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocOverloadTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocTypedefTag]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.tagName) || visitNode2(cbNode, data.typeExpression) || visitNode2(cbNode, data.name) || visitNodes2(cbNode, cbNodes, data.comment),
  [SyntaxKind.JSDocSignature]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.typeParameters) || visitNodes2(cbNode, cbNodes, data.parameters) || visitNode2(cbNode, data.type),
  [SyntaxKind.JSDocNameReference]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.ModuleDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.body),
  [SyntaxKind.ImportEqualsDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.moduleReference),
  [SyntaxKind.ExportDeclaration]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.exportClause) || visitNode2(cbNode, data.moduleSpecifier) || visitNode2(cbNode, data.attributes),
  [SyntaxKind.ImportType]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.argument) || visitNode2(cbNode, data.attributes) || visitNode2(cbNode, data.qualifier) || visitNodes2(cbNode, cbNodes, data.typeArguments),
  [SyntaxKind.ImportClause]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name) || visitNode2(cbNode, data.namedBindings),
  [SyntaxKind.ImportSpecifier]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.propertyName) || visitNode2(cbNode, data.name),
  [SyntaxKind.JSDocLink]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.JSDocLinkPlain]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.JSDocLinkCode]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.name),
  [SyntaxKind.TypeParameter]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.modifiers) || visitNode2(cbNode, data.name) || visitNode2(cbNode, data.constraint) || visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.defaultType),
  [SyntaxKind.SyntheticReferenceExpression]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.thisArg),
  [SyntaxKind.JSDocTypeLiteral]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.jsdocPropertyTags),
  [SyntaxKind.ForInStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.awaitModifier) || visitNode2(cbNode, data.initializer) || visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.statement),
  [SyntaxKind.ForOfStatement]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.awaitModifier) || visitNode2(cbNode, data.initializer) || visitNode2(cbNode, data.expression) || visitNode2(cbNode, data.statement),
  [SyntaxKind.CaseClause]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNodes2(cbNode, cbNodes, data.statements),
  [SyntaxKind.DefaultClause]: (data, cbNode, cbNodes) => visitNode2(cbNode, data.expression) || visitNodes2(cbNode, cbNodes, data.statements),
  [SyntaxKind.ObjectBindingPattern]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.ArrayBindingPattern]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.elements),
  [SyntaxKind.JSDocParameterTag]: forEachChildOfJSDocParameterOrPropertyTag,
  [SyntaxKind.JSDocPropertyTag]: forEachChildOfJSDocParameterOrPropertyTag,
  [SyntaxKind.SourceFile]: (data, cbNode, cbNodes) => visitNodes2(cbNode, cbNodes, data.statements) || visitNode2(cbNode, data.endOfFileToken)
};
function visitNode2(cbNode, node) {
  return node ? cbNode(node) : void 0;
}
function visitNodes2(cbNode, cbNodes, nodes) {
  if (!nodes)
    return void 0;
  if (cbNodes)
    return cbNodes(nodes);
  for (const node of nodes) {
    const result = cbNode(node);
    if (result)
      return result;
  }
  return void 0;
}
function createToken(kind) {
  return new NodeObject(kind, void 0);
}
function createQualifiedName(left, right) {
  return new NodeObject(SyntaxKind.QualifiedName, {
    left,
    right
  });
}
function createComputedPropertyName(expression) {
  return new NodeObject(SyntaxKind.ComputedPropertyName, {
    expression
  });
}
function createDecorator(expression) {
  return new NodeObject(SyntaxKind.Decorator, {
    expression
  });
}
function createIfStatement(expression, thenStatement, elseStatement) {
  return new NodeObject(SyntaxKind.IfStatement, {
    expression,
    thenStatement,
    elseStatement
  });
}
function createDoStatement(statement, expression) {
  return new NodeObject(SyntaxKind.DoStatement, {
    statement,
    expression
  });
}
function createWhileStatement(expression, statement) {
  return new NodeObject(SyntaxKind.WhileStatement, {
    expression,
    statement
  });
}
function createForStatement(initializer, condition, incrementor, statement) {
  return new NodeObject(SyntaxKind.ForStatement, {
    initializer,
    condition,
    incrementor,
    statement
  });
}
function createBreakStatement(label) {
  return new NodeObject(SyntaxKind.BreakStatement, {
    label
  });
}
function createContinueStatement(label) {
  return new NodeObject(SyntaxKind.ContinueStatement, {
    label
  });
}
function createReturnStatement(expression) {
  return new NodeObject(SyntaxKind.ReturnStatement, {
    expression
  });
}
function createWithStatement(expression, statement) {
  return new NodeObject(SyntaxKind.WithStatement, {
    expression,
    statement
  });
}
function createSwitchStatement(expression, caseBlock) {
  return new NodeObject(SyntaxKind.SwitchStatement, {
    expression,
    caseBlock
  });
}
function createCaseBlock(clauses) {
  return new NodeObject(SyntaxKind.CaseBlock, {
    clauses: createNodeArray(clauses)
  });
}
function createThrowStatement(expression) {
  return new NodeObject(SyntaxKind.ThrowStatement, {
    expression
  });
}
function createTryStatement(tryBlock, catchClause, finallyBlock) {
  return new NodeObject(SyntaxKind.TryStatement, {
    tryBlock,
    catchClause,
    finallyBlock
  });
}
function createCatchClause(variableDeclaration, block) {
  return new NodeObject(SyntaxKind.CatchClause, {
    variableDeclaration,
    block
  });
}
function createLabeledStatement(label, statement) {
  return new NodeObject(SyntaxKind.LabeledStatement, {
    label,
    statement
  });
}
function createExpressionStatement(expression) {
  return new NodeObject(SyntaxKind.ExpressionStatement, {
    expression
  });
}
function createBlock(statements, multiLine) {
  return new NodeObject(SyntaxKind.Block, {
    statements: createNodeArray(statements),
    multiLine
  });
}
function createVariableStatement(modifiers, declarationList) {
  return new NodeObject(SyntaxKind.VariableStatement, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    declarationList
  });
}
function createVariableDeclaration(name, exclamationToken, type, initializer) {
  return new NodeObject(SyntaxKind.VariableDeclaration, {
    name,
    exclamationToken,
    type,
    initializer
  });
}
function createVariableDeclarationList(declarations, flags) {
  const node = new NodeObject(SyntaxKind.VariableDeclarationList, {
    declarations: createNodeArray(declarations)
  });
  node.flags = flags;
  return node;
}
function createParameterDeclaration(modifiers, dotDotDotToken, name, questionToken, type, initializer) {
  return new NodeObject(SyntaxKind.Parameter, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    dotDotDotToken,
    name,
    questionToken,
    type,
    initializer
  });
}
function createBindingElement(dotDotDotToken, propertyName, name, initializer) {
  return new NodeObject(SyntaxKind.BindingElement, {
    dotDotDotToken,
    propertyName,
    name,
    initializer
  });
}
function createMissingDeclaration(modifiers) {
  return new NodeObject(SyntaxKind.MissingDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0
  });
}
function createFunctionDeclaration(modifiers, asteriskToken, name, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.FunctionDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    asteriskToken,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createClassDeclaration(modifiers, name, typeParameters, heritageClauses, members) {
  return new NodeObject(SyntaxKind.ClassDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    heritageClauses: heritageClauses ? createNodeArray(heritageClauses) : void 0,
    members: createNodeArray(members)
  });
}
function createClassExpression(modifiers, name, typeParameters, heritageClauses, members) {
  return new NodeObject(SyntaxKind.ClassExpression, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    heritageClauses: heritageClauses ? createNodeArray(heritageClauses) : void 0,
    members: createNodeArray(members)
  });
}
function createHeritageClause(token, types) {
  return new NodeObject(SyntaxKind.HeritageClause, {
    token,
    types: createNodeArray(types)
  });
}
function createInterfaceDeclaration(modifiers, name, typeParameters, heritageClauses, members) {
  return new NodeObject(SyntaxKind.InterfaceDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    heritageClauses: heritageClauses ? createNodeArray(heritageClauses) : void 0,
    members: createNodeArray(members)
  });
}
function createTypeAliasDeclaration(modifiers, name, typeParameters, type) {
  return new NodeObject(SyntaxKind.TypeAliasDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    type
  });
}
function createEnumMember(name, initializer) {
  return new NodeObject(SyntaxKind.EnumMember, {
    name,
    initializer
  });
}
function createEnumDeclaration(modifiers, name, members) {
  return new NodeObject(SyntaxKind.EnumDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    members: createNodeArray(members)
  });
}
function createModuleBlock(statements) {
  return new NodeObject(SyntaxKind.ModuleBlock, {
    statements: createNodeArray(statements)
  });
}
function createImportDeclaration(modifiers, importClause, moduleSpecifier, attributes) {
  return new NodeObject(SyntaxKind.ImportDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    importClause,
    moduleSpecifier,
    attributes
  });
}
function createExternalModuleReference(expression) {
  return new NodeObject(SyntaxKind.ExternalModuleReference, {
    expression
  });
}
function createNamespaceImport(name) {
  return new NodeObject(SyntaxKind.NamespaceImport, {
    name
  });
}
function createNamedImports(elements) {
  return new NodeObject(SyntaxKind.NamedImports, {
    elements: createNodeArray(elements)
  });
}
function createExportAssignment(modifiers, isExportEquals = false, type, expression) {
  return new NodeObject(SyntaxKind.ExportAssignment, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    isExportEquals,
    type,
    expression
  });
}
function createNamespaceExportDeclaration(modifiers, name) {
  return new NodeObject(SyntaxKind.NamespaceExportDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name
  });
}
function createNamespaceExport(name) {
  return new NodeObject(SyntaxKind.NamespaceExport, {
    name
  });
}
function createNamedExports(elements) {
  return new NodeObject(SyntaxKind.NamedExports, {
    elements: createNodeArray(elements)
  });
}
function createExportSpecifier(isTypeOnly = false, propertyName, name) {
  return new NodeObject(SyntaxKind.ExportSpecifier, {
    isTypeOnly,
    propertyName,
    name
  });
}
function createCallSignatureDeclaration(typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.CallSignature, {
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createConstructSignatureDeclaration(typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.ConstructSignature, {
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createConstructorDeclaration(modifiers, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.Constructor, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createGetAccessorDeclaration(modifiers, name, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.GetAccessor, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createSetAccessorDeclaration(modifiers, name, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.SetAccessor, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createIndexSignatureDeclaration(modifiers, parameters, type) {
  return new NodeObject(SyntaxKind.IndexSignature, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createMethodSignatureDeclaration(modifiers, name, postfixToken, typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.MethodSignature, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    postfixToken,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createMethodDeclaration(modifiers, asteriskToken, name, postfixToken, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.MethodDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    asteriskToken,
    name,
    postfixToken,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createPropertySignatureDeclaration(modifiers, name, postfixToken, type, initializer) {
  return new NodeObject(SyntaxKind.PropertySignature, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    postfixToken,
    type,
    initializer
  });
}
function createPropertyDeclaration(modifiers, name, postfixToken, type, initializer) {
  return new NodeObject(SyntaxKind.PropertyDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    postfixToken,
    type,
    initializer
  });
}
function createClassStaticBlockDeclaration(modifiers, body) {
  return new NodeObject(SyntaxKind.ClassStaticBlockDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    body
  });
}
function createStringLiteral(text, tokenFlags) {
  return new NodeObject(SyntaxKind.StringLiteral, {
    text,
    tokenFlags
  });
}
function createNumericLiteral(text, tokenFlags) {
  return new NodeObject(SyntaxKind.NumericLiteral, {
    text,
    tokenFlags
  });
}
function createBinaryExpression(modifiers, left, type, operatorToken, right) {
  return new NodeObject(SyntaxKind.BinaryExpression, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    left,
    type,
    operatorToken,
    right
  });
}
function createPrefixUnaryExpression(operator, operand) {
  return new NodeObject(SyntaxKind.PrefixUnaryExpression, {
    operator,
    operand
  });
}
function createPostfixUnaryExpression(operand, operator) {
  return new NodeObject(SyntaxKind.PostfixUnaryExpression, {
    operand,
    operator
  });
}
function createYieldExpression(asteriskToken, expression) {
  return new NodeObject(SyntaxKind.YieldExpression, {
    asteriskToken,
    expression
  });
}
function createArrowFunction(modifiers, typeParameters, parameters, type, equalsGreaterThanToken, body) {
  return new NodeObject(SyntaxKind.ArrowFunction, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    equalsGreaterThanToken,
    body
  });
}
function createFunctionExpression(modifiers, asteriskToken, name, typeParameters, parameters, type, body) {
  return new NodeObject(SyntaxKind.FunctionExpression, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    asteriskToken,
    name,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type,
    body
  });
}
function createAsExpression(expression, type) {
  return new NodeObject(SyntaxKind.AsExpression, {
    expression,
    type
  });
}
function createSatisfiesExpression(expression, type) {
  return new NodeObject(SyntaxKind.SatisfiesExpression, {
    expression,
    type
  });
}
function createConditionalExpression(condition, questionToken, whenTrue, colonToken, whenFalse) {
  return new NodeObject(SyntaxKind.ConditionalExpression, {
    condition,
    questionToken,
    whenTrue,
    colonToken,
    whenFalse
  });
}
function createPropertyAccessExpression(expression, questionDotToken, name, flags) {
  const node = new NodeObject(SyntaxKind.PropertyAccessExpression, {
    expression,
    questionDotToken,
    name
  });
  node.flags = flags;
  return node;
}
function createElementAccessExpression(expression, questionDotToken, argumentExpression, flags) {
  const node = new NodeObject(SyntaxKind.ElementAccessExpression, {
    expression,
    questionDotToken,
    argumentExpression
  });
  node.flags = flags;
  return node;
}
function createCallExpression(expression, questionDotToken, typeArguments, arguments_, flags) {
  const node = new NodeObject(SyntaxKind.CallExpression, {
    expression,
    questionDotToken,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0,
    arguments: createNodeArray(arguments_)
  });
  node.flags = flags;
  return node;
}
function createNewExpression(expression, typeArguments, arguments_) {
  return new NodeObject(SyntaxKind.NewExpression, {
    expression,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0,
    arguments: arguments_ ? createNodeArray(arguments_) : void 0
  });
}
function createMetaProperty(keywordToken, name) {
  return new NodeObject(SyntaxKind.MetaProperty, {
    keywordToken,
    name
  });
}
function createNonNullExpression(expression, flags) {
  const node = new NodeObject(SyntaxKind.NonNullExpression, {
    expression
  });
  node.flags = flags;
  return node;
}
function createSpreadElement(expression) {
  return new NodeObject(SyntaxKind.SpreadElement, {
    expression
  });
}
function createTemplateExpression(head, templateSpans) {
  return new NodeObject(SyntaxKind.TemplateExpression, {
    head,
    templateSpans: createNodeArray(templateSpans)
  });
}
function createTemplateSpan(expression, literal) {
  return new NodeObject(SyntaxKind.TemplateSpan, {
    expression,
    literal
  });
}
function createTaggedTemplateExpression(tag, questionDotToken, typeArguments, template, flags) {
  const node = new NodeObject(SyntaxKind.TaggedTemplateExpression, {
    tag,
    questionDotToken,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0,
    template
  });
  node.flags = flags;
  return node;
}
function createParenthesizedExpression(expression) {
  return new NodeObject(SyntaxKind.ParenthesizedExpression, {
    expression
  });
}
function createArrayLiteralExpression(elements, multiLine) {
  return new NodeObject(SyntaxKind.ArrayLiteralExpression, {
    elements: createNodeArray(elements),
    multiLine
  });
}
function createObjectLiteralExpression(properties, multiLine) {
  return new NodeObject(SyntaxKind.ObjectLiteralExpression, {
    properties: createNodeArray(properties),
    multiLine
  });
}
function createSpreadAssignment(expression) {
  return new NodeObject(SyntaxKind.SpreadAssignment, {
    expression
  });
}
function createPropertyAssignment(modifiers, name, postfixToken, type, initializer) {
  return new NodeObject(SyntaxKind.PropertyAssignment, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    postfixToken,
    type,
    initializer
  });
}
function createShorthandPropertyAssignment(modifiers, name, postfixToken, type, equalsToken, objectAssignmentInitializer) {
  return new NodeObject(SyntaxKind.ShorthandPropertyAssignment, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    postfixToken,
    type,
    equalsToken,
    objectAssignmentInitializer
  });
}
function createDeleteExpression(expression) {
  return new NodeObject(SyntaxKind.DeleteExpression, {
    expression
  });
}
function createTypeOfExpression(expression) {
  return new NodeObject(SyntaxKind.TypeOfExpression, {
    expression
  });
}
function createVoidExpression(expression) {
  return new NodeObject(SyntaxKind.VoidExpression, {
    expression
  });
}
function createAwaitExpression(expression) {
  return new NodeObject(SyntaxKind.AwaitExpression, {
    expression
  });
}
function createTypeAssertion(type, expression) {
  return new NodeObject(SyntaxKind.TypeAssertionExpression, {
    type,
    expression
  });
}
function createUnionTypeNode(types) {
  return new NodeObject(SyntaxKind.UnionType, {
    types: createNodeArray(types)
  });
}
function createIntersectionTypeNode(types) {
  return new NodeObject(SyntaxKind.IntersectionType, {
    types: createNodeArray(types)
  });
}
function createConditionalTypeNode(checkType, extendsType, trueType, falseType) {
  return new NodeObject(SyntaxKind.ConditionalType, {
    checkType,
    extendsType,
    trueType,
    falseType
  });
}
function createTypeOperatorNode(operator, type) {
  return new NodeObject(SyntaxKind.TypeOperator, {
    operator,
    type
  });
}
function createInferTypeNode(typeParameter) {
  return new NodeObject(SyntaxKind.InferType, {
    typeParameter
  });
}
function createArrayTypeNode(elementType) {
  return new NodeObject(SyntaxKind.ArrayType, {
    elementType
  });
}
function createIndexedAccessTypeNode(objectType, indexType) {
  return new NodeObject(SyntaxKind.IndexedAccessType, {
    objectType,
    indexType
  });
}
function createTypeReferenceNode(typeName, typeArguments) {
  return new NodeObject(SyntaxKind.TypeReference, {
    typeName,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0
  });
}
function createExpressionWithTypeArguments(expression, typeArguments) {
  return new NodeObject(SyntaxKind.ExpressionWithTypeArguments, {
    expression,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0
  });
}
function createLiteralTypeNode(literal) {
  return new NodeObject(SyntaxKind.LiteralType, {
    literal
  });
}
function createTypePredicateNode(assertsModifier, parameterName, type) {
  return new NodeObject(SyntaxKind.TypePredicate, {
    assertsModifier,
    parameterName,
    type
  });
}
function createImportAttribute(name, value) {
  return new NodeObject(SyntaxKind.ImportAttribute, {
    name,
    value
  });
}
function createImportAttributes(token, attributes, multiLine) {
  return new NodeObject(SyntaxKind.ImportAttributes, {
    token,
    attributes: createNodeArray(attributes),
    multiLine
  });
}
function createTypeQueryNode(exprName, typeArguments) {
  return new NodeObject(SyntaxKind.TypeQuery, {
    exprName,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0
  });
}
function createMappedTypeNode(readonlyToken, typeParameter, nameType, questionToken, type, members) {
  return new NodeObject(SyntaxKind.MappedType, {
    readonlyToken,
    typeParameter,
    nameType,
    questionToken,
    type,
    members: members ? createNodeArray(members) : void 0
  });
}
function createTypeLiteralNode(members) {
  return new NodeObject(SyntaxKind.TypeLiteral, {
    members: createNodeArray(members)
  });
}
function createTupleTypeNode(elements) {
  return new NodeObject(SyntaxKind.TupleType, {
    elements: createNodeArray(elements)
  });
}
function createNamedTupleMember(dotDotDotToken, name, questionToken, type) {
  return new NodeObject(SyntaxKind.NamedTupleMember, {
    dotDotDotToken,
    name,
    questionToken,
    type
  });
}
function createOptionalTypeNode(type) {
  return new NodeObject(SyntaxKind.OptionalType, {
    type
  });
}
function createRestTypeNode(type) {
  return new NodeObject(SyntaxKind.RestType, {
    type
  });
}
function createParenthesizedTypeNode(type) {
  return new NodeObject(SyntaxKind.ParenthesizedType, {
    type
  });
}
function createFunctionTypeNode(typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.FunctionType, {
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createConstructorTypeNode(modifiers, typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.ConstructorType, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createTemplateLiteralTypeNode(head, templateSpans) {
  return new NodeObject(SyntaxKind.TemplateLiteralType, {
    head,
    templateSpans: createNodeArray(templateSpans)
  });
}
function createTemplateLiteralTypeSpan(type, literal) {
  return new NodeObject(SyntaxKind.TemplateLiteralTypeSpan, {
    type,
    literal
  });
}
function createSyntheticExpression(type, isSpread, tupleNameSource) {
  return new NodeObject(SyntaxKind.SyntheticExpression, {
    type,
    isSpread,
    tupleNameSource
  });
}
function createPartiallyEmittedExpression(expression) {
  return new NodeObject(SyntaxKind.PartiallyEmittedExpression, {
    expression
  });
}
function createJsxElement(openingElement, children, closingElement) {
  return new NodeObject(SyntaxKind.JsxElement, {
    openingElement,
    children: createNodeArray(children),
    closingElement
  });
}
function createJsxAttributes(properties) {
  return new NodeObject(SyntaxKind.JsxAttributes, {
    properties: createNodeArray(properties)
  });
}
function createJsxNamespacedName(namespace, name) {
  return new NodeObject(SyntaxKind.JsxNamespacedName, {
    namespace,
    name
  });
}
function createJsxOpeningElement(tagName, typeArguments, attributes) {
  return new NodeObject(SyntaxKind.JsxOpeningElement, {
    tagName,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0,
    attributes
  });
}
function createJsxSelfClosingElement(tagName, typeArguments, attributes) {
  return new NodeObject(SyntaxKind.JsxSelfClosingElement, {
    tagName,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0,
    attributes
  });
}
function createJsxFragment(openingFragment, children, closingFragment) {
  return new NodeObject(SyntaxKind.JsxFragment, {
    openingFragment,
    children: createNodeArray(children),
    closingFragment
  });
}
function createJsxAttribute(name, initializer) {
  return new NodeObject(SyntaxKind.JsxAttribute, {
    name,
    initializer
  });
}
function createJsxSpreadAttribute(expression) {
  return new NodeObject(SyntaxKind.JsxSpreadAttribute, {
    expression
  });
}
function createJsxClosingElement(tagName) {
  return new NodeObject(SyntaxKind.JsxClosingElement, {
    tagName
  });
}
function createJsxExpression(dotDotDotToken, expression) {
  return new NodeObject(SyntaxKind.JsxExpression, {
    dotDotDotToken,
    expression
  });
}
function createSyntaxList(children) {
  return new NodeObject(SyntaxKind.SyntaxList, {
    children
  });
}
function createJSDoc(comment, tags) {
  return new NodeObject(SyntaxKind.JSDoc, {
    comment: createNodeArray(comment),
    tags: tags ? createNodeArray(tags) : void 0
  });
}
function createJSDocTypeExpression(type) {
  return new NodeObject(SyntaxKind.JSDocTypeExpression, {
    type
  });
}
function createJSDocNonNullableType(type) {
  return new NodeObject(SyntaxKind.JSDocNonNullableType, {
    type
  });
}
function createJSDocNullableType(type) {
  return new NodeObject(SyntaxKind.JSDocNullableType, {
    type
  });
}
function createJSDocVariadicType(type) {
  return new NodeObject(SyntaxKind.JSDocVariadicType, {
    type
  });
}
function createJSDocOptionalType(type) {
  return new NodeObject(SyntaxKind.JSDocOptionalType, {
    type
  });
}
function createJSDocTypeTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocTypeTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocUnknownTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocUnknownTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocTemplateTag(tagName, constraint, typeParameters, comment) {
  return new NodeObject(SyntaxKind.JSDocTemplateTag, {
    tagName,
    constraint,
    typeParameters: createNodeArray(typeParameters),
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocReturnTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocReturnTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocPublicTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocPublicTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocPrivateTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocPrivateTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocProtectedTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocProtectedTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocReadonlyTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocReadonlyTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocOverrideTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocOverrideTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocDeprecatedTag(tagName, comment) {
  return new NodeObject(SyntaxKind.JSDocDeprecatedTag, {
    tagName,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocSeeTag(tagName, nameExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocSeeTag, {
    tagName,
    nameExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocImplementsTag(tagName, className, comment) {
  return new NodeObject(SyntaxKind.JSDocImplementsTag, {
    tagName,
    className,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocAugmentsTag(tagName, className, comment) {
  return new NodeObject(SyntaxKind.JSDocAugmentsTag, {
    tagName,
    className,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocSatisfiesTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocSatisfiesTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocThrowsTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocThrowsTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocThisTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocThisTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocImportTag(tagName, importClause, moduleSpecifier, attributes, comment) {
  return new NodeObject(SyntaxKind.JSDocImportTag, {
    tagName,
    importClause,
    moduleSpecifier,
    attributes,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocCallbackTag(tagName, typeExpression, name, comment) {
  return new NodeObject(SyntaxKind.JSDocCallbackTag, {
    tagName,
    typeExpression,
    name,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocOverloadTag(tagName, typeExpression, comment) {
  return new NodeObject(SyntaxKind.JSDocOverloadTag, {
    tagName,
    typeExpression,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocTypedefTag(tagName, typeExpression, name, comment) {
  return new NodeObject(SyntaxKind.JSDocTypedefTag, {
    tagName,
    typeExpression,
    name,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocSignature(typeParameters, parameters, type) {
  return new NodeObject(SyntaxKind.JSDocSignature, {
    typeParameters: typeParameters ? createNodeArray(typeParameters) : void 0,
    parameters: createNodeArray(parameters),
    type
  });
}
function createJSDocNameReference(name) {
  return new NodeObject(SyntaxKind.JSDocNameReference, {
    name
  });
}
function createModuleDeclaration(modifiers, keyword, name, body) {
  return new NodeObject(SyntaxKind.ModuleDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    keyword,
    name,
    body
  });
}
function createImportEqualsDeclaration(modifiers, isTypeOnly = false, name, moduleReference) {
  return new NodeObject(SyntaxKind.ImportEqualsDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    isTypeOnly,
    name,
    moduleReference
  });
}
function createExportDeclaration(modifiers, isTypeOnly, exportClause, moduleSpecifier, attributes) {
  return new NodeObject(SyntaxKind.ExportDeclaration, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    isTypeOnly,
    exportClause,
    moduleSpecifier,
    attributes
  });
}
function createImportTypeNode(isTypeOf = false, argument, attributes, qualifier, typeArguments) {
  return new NodeObject(SyntaxKind.ImportType, {
    isTypeOf,
    argument,
    attributes,
    qualifier,
    typeArguments: typeArguments ? createNodeArray(typeArguments) : void 0
  });
}
function createImportClause(phaseModifier, name, namedBindings) {
  return new NodeObject(SyntaxKind.ImportClause, {
    phaseModifier,
    name,
    namedBindings
  });
}
function createImportSpecifier(isTypeOnly = false, propertyName, name) {
  return new NodeObject(SyntaxKind.ImportSpecifier, {
    isTypeOnly,
    propertyName,
    name
  });
}
function createJSDocLink(name, text) {
  return new NodeObject(SyntaxKind.JSDocLink, {
    name,
    text
  });
}
function createJSDocLinkPlain(name, text) {
  return new NodeObject(SyntaxKind.JSDocLinkPlain, {
    name,
    text
  });
}
function createJSDocLinkCode(name, text) {
  return new NodeObject(SyntaxKind.JSDocLinkCode, {
    name,
    text
  });
}
function createTypeParameterDeclaration(modifiers, name, constraint, expression, defaultType) {
  return new NodeObject(SyntaxKind.TypeParameter, {
    modifiers: modifiers ? createNodeArray(modifiers) : void 0,
    name,
    constraint,
    expression,
    defaultType
  });
}
function createSyntheticReferenceExpression(expression, thisArg) {
  return new NodeObject(SyntaxKind.SyntheticReferenceExpression, {
    expression,
    thisArg
  });
}
function createJSDocTypeLiteral(jsdocPropertyTags, isArrayType) {
  return new NodeObject(SyntaxKind.JSDocTypeLiteral, {
    jsdocPropertyTags,
    isArrayType
  });
}
function createForInStatement(awaitModifier, initializer, expression, statement) {
  return new NodeObject(SyntaxKind.ForInStatement, {
    awaitModifier,
    initializer,
    expression,
    statement
  });
}
function createForOfStatement(awaitModifier, initializer, expression, statement) {
  return new NodeObject(SyntaxKind.ForOfStatement, {
    awaitModifier,
    initializer,
    expression,
    statement
  });
}
function createCaseClause(expression, statements) {
  return new NodeObject(SyntaxKind.CaseClause, {
    expression,
    statements: createNodeArray(statements)
  });
}
function createDefaultClause(expression, statements) {
  return new NodeObject(SyntaxKind.DefaultClause, {
    expression,
    statements: createNodeArray(statements)
  });
}
function createObjectBindingPattern(elements) {
  return new NodeObject(SyntaxKind.ObjectBindingPattern, {
    elements: createNodeArray(elements)
  });
}
function createArrayBindingPattern(elements) {
  return new NodeObject(SyntaxKind.ArrayBindingPattern, {
    elements: createNodeArray(elements)
  });
}
function createJSDocParameterTag(tagName, name, isBracketed, typeExpression, isNameFirst, comment) {
  return new NodeObject(SyntaxKind.JSDocParameterTag, {
    tagName,
    name,
    isBracketed,
    typeExpression,
    isNameFirst,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function createJSDocPropertyTag(tagName, name, isBracketed, typeExpression, isNameFirst, comment) {
  return new NodeObject(SyntaxKind.JSDocPropertyTag, {
    tagName,
    name,
    isBracketed,
    typeExpression,
    isNameFirst,
    comment: comment ? createNodeArray(comment) : void 0
  });
}
function updateQualifiedName(node, left, right) {
  return node.left !== left || node.right !== right ? createQualifiedName(left, right) : node;
}
function updateComputedPropertyName(node, expression) {
  return node.expression !== expression ? createComputedPropertyName(expression) : node;
}
function updateDecorator(node, expression) {
  return node.expression !== expression ? createDecorator(expression) : node;
}
function updateIfStatement(node, expression, thenStatement, elseStatement) {
  return node.expression !== expression || node.thenStatement !== thenStatement || node.elseStatement !== elseStatement ? createIfStatement(expression, thenStatement, elseStatement) : node;
}
function updateDoStatement(node, statement, expression) {
  return node.statement !== statement || node.expression !== expression ? createDoStatement(statement, expression) : node;
}
function updateWhileStatement(node, expression, statement) {
  return node.expression !== expression || node.statement !== statement ? createWhileStatement(expression, statement) : node;
}
function updateForStatement(node, initializer, condition, incrementor, statement) {
  return node.initializer !== initializer || node.condition !== condition || node.incrementor !== incrementor || node.statement !== statement ? createForStatement(initializer, condition, incrementor, statement) : node;
}
function updateBreakStatement(node, label) {
  return node.label !== label ? createBreakStatement(label) : node;
}
function updateContinueStatement(node, label) {
  return node.label !== label ? createContinueStatement(label) : node;
}
function updateReturnStatement(node, expression) {
  return node.expression !== expression ? createReturnStatement(expression) : node;
}
function updateWithStatement(node, expression, statement) {
  return node.expression !== expression || node.statement !== statement ? createWithStatement(expression, statement) : node;
}
function updateSwitchStatement(node, expression, caseBlock) {
  return node.expression !== expression || node.caseBlock !== caseBlock ? createSwitchStatement(expression, caseBlock) : node;
}
function updateCaseBlock(node, clauses) {
  return node.clauses !== clauses ? createCaseBlock(clauses) : node;
}
function updateThrowStatement(node, expression) {
  return node.expression !== expression ? createThrowStatement(expression) : node;
}
function updateTryStatement(node, tryBlock, catchClause, finallyBlock) {
  return node.tryBlock !== tryBlock || node.catchClause !== catchClause || node.finallyBlock !== finallyBlock ? createTryStatement(tryBlock, catchClause, finallyBlock) : node;
}
function updateCatchClause(node, variableDeclaration, block) {
  return node.variableDeclaration !== variableDeclaration || node.block !== block ? createCatchClause(variableDeclaration, block) : node;
}
function updateLabeledStatement(node, label, statement) {
  return node.label !== label || node.statement !== statement ? createLabeledStatement(label, statement) : node;
}
function updateExpressionStatement(node, expression) {
  return node.expression !== expression ? createExpressionStatement(expression) : node;
}
function updateBlock(node, statements) {
  return node.statements !== statements ? createBlock(statements, node.multiLine) : node;
}
function updateVariableStatement(node, modifiers, declarationList) {
  return node.modifiers !== modifiers || node.declarationList !== declarationList ? createVariableStatement(modifiers, declarationList) : node;
}
function updateVariableDeclaration(node, name, exclamationToken, type, initializer) {
  return node.name !== name || node.exclamationToken !== exclamationToken || node.type !== type || node.initializer !== initializer ? createVariableDeclaration(name, exclamationToken, type, initializer) : node;
}
function updateVariableDeclarationList(node, declarations) {
  return node.declarations !== declarations ? createVariableDeclarationList(declarations, node.flags) : node;
}
function updateParameterDeclaration(node, modifiers, dotDotDotToken, name, questionToken, type, initializer) {
  return node.modifiers !== modifiers || node.dotDotDotToken !== dotDotDotToken || node.name !== name || node.questionToken !== questionToken || node.type !== type || node.initializer !== initializer ? createParameterDeclaration(modifiers, dotDotDotToken, name, questionToken, type, initializer) : node;
}
function updateBindingElement(node, dotDotDotToken, propertyName, name, initializer) {
  return node.dotDotDotToken !== dotDotDotToken || node.propertyName !== propertyName || node.name !== name || node.initializer !== initializer ? createBindingElement(dotDotDotToken, propertyName, name, initializer) : node;
}
function updateMissingDeclaration(node, modifiers) {
  return node.modifiers !== modifiers ? createMissingDeclaration(modifiers) : node;
}
function updateFunctionDeclaration(node, modifiers, asteriskToken, name, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.asteriskToken !== asteriskToken || node.name !== name || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createFunctionDeclaration(modifiers, asteriskToken, name, typeParameters, parameters, type, body) : node;
}
function updateClassDeclaration(node, modifiers, name, typeParameters, heritageClauses, members) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.heritageClauses !== heritageClauses || node.members !== members ? createClassDeclaration(modifiers, name, typeParameters, heritageClauses, members) : node;
}
function updateClassExpression(node, modifiers, name, typeParameters, heritageClauses, members) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.heritageClauses !== heritageClauses || node.members !== members ? createClassExpression(modifiers, name, typeParameters, heritageClauses, members) : node;
}
function updateHeritageClause(node, types) {
  return node.types !== types ? createHeritageClause(node.token, types) : node;
}
function updateInterfaceDeclaration(node, modifiers, name, typeParameters, heritageClauses, members) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.heritageClauses !== heritageClauses || node.members !== members ? createInterfaceDeclaration(modifiers, name, typeParameters, heritageClauses, members) : node;
}
function updateTypeAliasDeclaration(node, modifiers, name, typeParameters, type) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.type !== type ? createTypeAliasDeclaration(modifiers, name, typeParameters, type) : node;
}
function updateEnumMember(node, name, initializer) {
  return node.name !== name || node.initializer !== initializer ? createEnumMember(name, initializer) : node;
}
function updateEnumDeclaration(node, modifiers, name, members) {
  return node.modifiers !== modifiers || node.name !== name || node.members !== members ? createEnumDeclaration(modifiers, name, members) : node;
}
function updateModuleBlock(node, statements) {
  return node.statements !== statements ? createModuleBlock(statements) : node;
}
function updateImportDeclaration(node, modifiers, importClause, moduleSpecifier, attributes) {
  return node.modifiers !== modifiers || node.importClause !== importClause || node.moduleSpecifier !== moduleSpecifier || node.attributes !== attributes ? createImportDeclaration(modifiers, importClause, moduleSpecifier, attributes) : node;
}
function updateExternalModuleReference(node, expression) {
  return node.expression !== expression ? createExternalModuleReference(expression) : node;
}
function updateNamespaceImport(node, name) {
  return node.name !== name ? createNamespaceImport(name) : node;
}
function updateNamedImports(node, elements) {
  return node.elements !== elements ? createNamedImports(elements) : node;
}
function updateExportAssignment(node, modifiers, type, expression) {
  return node.modifiers !== modifiers || node.type !== type || node.expression !== expression ? createExportAssignment(modifiers, node.isExportEquals, type, expression) : node;
}
function updateNamespaceExportDeclaration(node, modifiers, name) {
  return node.modifiers !== modifiers || node.name !== name ? createNamespaceExportDeclaration(modifiers, name) : node;
}
function updateNamespaceExport(node, name) {
  return node.name !== name ? createNamespaceExport(name) : node;
}
function updateNamedExports(node, elements) {
  return node.elements !== elements ? createNamedExports(elements) : node;
}
function updateExportSpecifier(node, propertyName, name) {
  return node.propertyName !== propertyName || node.name !== name ? createExportSpecifier(node.isTypeOnly, propertyName, name) : node;
}
function updateCallSignatureDeclaration(node, typeParameters, parameters, type) {
  return node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createCallSignatureDeclaration(typeParameters, parameters, type) : node;
}
function updateConstructSignatureDeclaration(node, typeParameters, parameters, type) {
  return node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createConstructSignatureDeclaration(typeParameters, parameters, type) : node;
}
function updateConstructorDeclaration(node, modifiers, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createConstructorDeclaration(modifiers, typeParameters, parameters, type, body) : node;
}
function updateGetAccessorDeclaration(node, modifiers, name, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createGetAccessorDeclaration(modifiers, name, typeParameters, parameters, type, body) : node;
}
function updateSetAccessorDeclaration(node, modifiers, name, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.name !== name || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createSetAccessorDeclaration(modifiers, name, typeParameters, parameters, type, body) : node;
}
function updateIndexSignatureDeclaration(node, modifiers, parameters, type) {
  return node.modifiers !== modifiers || node.parameters !== parameters || node.type !== type ? createIndexSignatureDeclaration(modifiers, parameters, type) : node;
}
function updateMethodSignatureDeclaration(node, modifiers, name, postfixToken, typeParameters, parameters, type) {
  return node.modifiers !== modifiers || node.name !== name || node.postfixToken !== postfixToken || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createMethodSignatureDeclaration(modifiers, name, postfixToken, typeParameters, parameters, type) : node;
}
function updateMethodDeclaration(node, modifiers, asteriskToken, name, postfixToken, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.asteriskToken !== asteriskToken || node.name !== name || node.postfixToken !== postfixToken || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createMethodDeclaration(modifiers, asteriskToken, name, postfixToken, typeParameters, parameters, type, body) : node;
}
function updatePropertySignatureDeclaration(node, modifiers, name, postfixToken, type, initializer) {
  return node.modifiers !== modifiers || node.name !== name || node.postfixToken !== postfixToken || node.type !== type || node.initializer !== initializer ? createPropertySignatureDeclaration(modifiers, name, postfixToken, type, initializer) : node;
}
function updatePropertyDeclaration(node, modifiers, name, postfixToken, type, initializer) {
  return node.modifiers !== modifiers || node.name !== name || node.postfixToken !== postfixToken || node.type !== type || node.initializer !== initializer ? createPropertyDeclaration(modifiers, name, postfixToken, type, initializer) : node;
}
function updateClassStaticBlockDeclaration(node, modifiers, body) {
  return node.modifiers !== modifiers || node.body !== body ? createClassStaticBlockDeclaration(modifiers, body) : node;
}
function updateBinaryExpression(node, modifiers, left, type, operatorToken, right) {
  return node.modifiers !== modifiers || node.left !== left || node.type !== type || node.operatorToken !== operatorToken || node.right !== right ? createBinaryExpression(modifiers, left, type, operatorToken, right) : node;
}
function updatePrefixUnaryExpression(node, operand) {
  return node.operand !== operand ? createPrefixUnaryExpression(node.operator, operand) : node;
}
function updatePostfixUnaryExpression(node, operand) {
  return node.operand !== operand ? createPostfixUnaryExpression(operand, node.operator) : node;
}
function updateYieldExpression(node, asteriskToken, expression) {
  return node.asteriskToken !== asteriskToken || node.expression !== expression ? createYieldExpression(asteriskToken, expression) : node;
}
function updateArrowFunction(node, modifiers, typeParameters, parameters, type, equalsGreaterThanToken, body) {
  return node.modifiers !== modifiers || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.equalsGreaterThanToken !== equalsGreaterThanToken || node.body !== body ? createArrowFunction(modifiers, typeParameters, parameters, type, equalsGreaterThanToken, body) : node;
}
function updateFunctionExpression(node, modifiers, asteriskToken, name, typeParameters, parameters, type, body) {
  return node.modifiers !== modifiers || node.asteriskToken !== asteriskToken || node.name !== name || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type || node.body !== body ? createFunctionExpression(modifiers, asteriskToken, name, typeParameters, parameters, type, body) : node;
}
function updateAsExpression(node, expression, type) {
  return node.expression !== expression || node.type !== type ? createAsExpression(expression, type) : node;
}
function updateSatisfiesExpression(node, expression, type) {
  return node.expression !== expression || node.type !== type ? createSatisfiesExpression(expression, type) : node;
}
function updateConditionalExpression(node, condition, questionToken, whenTrue, colonToken, whenFalse) {
  return node.condition !== condition || node.questionToken !== questionToken || node.whenTrue !== whenTrue || node.colonToken !== colonToken || node.whenFalse !== whenFalse ? createConditionalExpression(condition, questionToken, whenTrue, colonToken, whenFalse) : node;
}
function updatePropertyAccessExpression(node, expression, questionDotToken, name) {
  return node.expression !== expression || node.questionDotToken !== questionDotToken || node.name !== name ? createPropertyAccessExpression(expression, questionDotToken, name, node.flags) : node;
}
function updateElementAccessExpression(node, expression, questionDotToken, argumentExpression) {
  return node.expression !== expression || node.questionDotToken !== questionDotToken || node.argumentExpression !== argumentExpression ? createElementAccessExpression(expression, questionDotToken, argumentExpression, node.flags) : node;
}
function updateCallExpression(node, expression, questionDotToken, typeArguments, arguments_) {
  return node.expression !== expression || node.questionDotToken !== questionDotToken || node.typeArguments !== typeArguments || node.arguments !== arguments_ ? createCallExpression(expression, questionDotToken, typeArguments, arguments_, node.flags) : node;
}
function updateNewExpression(node, expression, typeArguments, arguments_) {
  return node.expression !== expression || node.typeArguments !== typeArguments || node.arguments !== arguments_ ? createNewExpression(expression, typeArguments, arguments_) : node;
}
function updateMetaProperty(node, name) {
  return node.name !== name ? createMetaProperty(node.keywordToken, name) : node;
}
function updateNonNullExpression(node, expression) {
  return node.expression !== expression ? createNonNullExpression(expression, node.flags) : node;
}
function updateSpreadElement(node, expression) {
  return node.expression !== expression ? createSpreadElement(expression) : node;
}
function updateTemplateExpression(node, head, templateSpans) {
  return node.head !== head || node.templateSpans !== templateSpans ? createTemplateExpression(head, templateSpans) : node;
}
function updateTemplateSpan(node, expression, literal) {
  return node.expression !== expression || node.literal !== literal ? createTemplateSpan(expression, literal) : node;
}
function updateTaggedTemplateExpression(node, tag, questionDotToken, typeArguments, template) {
  return node.tag !== tag || node.questionDotToken !== questionDotToken || node.typeArguments !== typeArguments || node.template !== template ? createTaggedTemplateExpression(tag, questionDotToken, typeArguments, template, node.flags) : node;
}
function updateParenthesizedExpression(node, expression) {
  return node.expression !== expression ? createParenthesizedExpression(expression) : node;
}
function updateArrayLiteralExpression(node, elements) {
  return node.elements !== elements ? createArrayLiteralExpression(elements, node.multiLine) : node;
}
function updateObjectLiteralExpression(node, properties) {
  return node.properties !== properties ? createObjectLiteralExpression(properties, node.multiLine) : node;
}
function updateSpreadAssignment(node, expression) {
  return node.expression !== expression ? createSpreadAssignment(expression) : node;
}
function updatePropertyAssignment(node, modifiers, name, postfixToken, type, initializer) {
  return node.modifiers !== modifiers || node.name !== name || node.postfixToken !== postfixToken || node.type !== type || node.initializer !== initializer ? createPropertyAssignment(modifiers, name, postfixToken, type, initializer) : node;
}
function updateShorthandPropertyAssignment(node, modifiers, name, postfixToken, type, equalsToken, objectAssignmentInitializer) {
  return node.modifiers !== modifiers || node.name !== name || node.postfixToken !== postfixToken || node.type !== type || node.equalsToken !== equalsToken || node.objectAssignmentInitializer !== objectAssignmentInitializer ? createShorthandPropertyAssignment(modifiers, name, postfixToken, type, equalsToken, objectAssignmentInitializer) : node;
}
function updateDeleteExpression(node, expression) {
  return node.expression !== expression ? createDeleteExpression(expression) : node;
}
function updateTypeOfExpression(node, expression) {
  return node.expression !== expression ? createTypeOfExpression(expression) : node;
}
function updateVoidExpression(node, expression) {
  return node.expression !== expression ? createVoidExpression(expression) : node;
}
function updateAwaitExpression(node, expression) {
  return node.expression !== expression ? createAwaitExpression(expression) : node;
}
function updateTypeAssertion(node, type, expression) {
  return node.type !== type || node.expression !== expression ? createTypeAssertion(type, expression) : node;
}
function updateUnionTypeNode(node, types) {
  return node.types !== types ? createUnionTypeNode(types) : node;
}
function updateIntersectionTypeNode(node, types) {
  return node.types !== types ? createIntersectionTypeNode(types) : node;
}
function updateConditionalTypeNode(node, checkType, extendsType, trueType, falseType) {
  return node.checkType !== checkType || node.extendsType !== extendsType || node.trueType !== trueType || node.falseType !== falseType ? createConditionalTypeNode(checkType, extendsType, trueType, falseType) : node;
}
function updateTypeOperatorNode(node, type) {
  return node.type !== type ? createTypeOperatorNode(node.operator, type) : node;
}
function updateInferTypeNode(node, typeParameter) {
  return node.typeParameter !== typeParameter ? createInferTypeNode(typeParameter) : node;
}
function updateArrayTypeNode(node, elementType) {
  return node.elementType !== elementType ? createArrayTypeNode(elementType) : node;
}
function updateIndexedAccessTypeNode(node, objectType, indexType) {
  return node.objectType !== objectType || node.indexType !== indexType ? createIndexedAccessTypeNode(objectType, indexType) : node;
}
function updateTypeReferenceNode(node, typeName, typeArguments) {
  return node.typeName !== typeName || node.typeArguments !== typeArguments ? createTypeReferenceNode(typeName, typeArguments) : node;
}
function updateExpressionWithTypeArguments(node, expression, typeArguments) {
  return node.expression !== expression || node.typeArguments !== typeArguments ? createExpressionWithTypeArguments(expression, typeArguments) : node;
}
function updateLiteralTypeNode(node, literal) {
  return node.literal !== literal ? createLiteralTypeNode(literal) : node;
}
function updateTypePredicateNode(node, assertsModifier, parameterName, type) {
  return node.assertsModifier !== assertsModifier || node.parameterName !== parameterName || node.type !== type ? createTypePredicateNode(assertsModifier, parameterName, type) : node;
}
function updateImportAttribute(node, name, value) {
  return node.name !== name || node.value !== value ? createImportAttribute(name, value) : node;
}
function updateImportAttributes(node, attributes) {
  return node.attributes !== attributes ? createImportAttributes(node.token, attributes, node.multiLine) : node;
}
function updateTypeQueryNode(node, exprName, typeArguments) {
  return node.exprName !== exprName || node.typeArguments !== typeArguments ? createTypeQueryNode(exprName, typeArguments) : node;
}
function updateMappedTypeNode(node, readonlyToken, typeParameter, nameType, questionToken, type, members) {
  return node.readonlyToken !== readonlyToken || node.typeParameter !== typeParameter || node.nameType !== nameType || node.questionToken !== questionToken || node.type !== type || node.members !== members ? createMappedTypeNode(readonlyToken, typeParameter, nameType, questionToken, type, members) : node;
}
function updateTypeLiteralNode(node, members) {
  return node.members !== members ? createTypeLiteralNode(members) : node;
}
function updateTupleTypeNode(node, elements) {
  return node.elements !== elements ? createTupleTypeNode(elements) : node;
}
function updateNamedTupleMember(node, dotDotDotToken, name, questionToken, type) {
  return node.dotDotDotToken !== dotDotDotToken || node.name !== name || node.questionToken !== questionToken || node.type !== type ? createNamedTupleMember(dotDotDotToken, name, questionToken, type) : node;
}
function updateOptionalTypeNode(node, type) {
  return node.type !== type ? createOptionalTypeNode(type) : node;
}
function updateRestTypeNode(node, type) {
  return node.type !== type ? createRestTypeNode(type) : node;
}
function updateParenthesizedTypeNode(node, type) {
  return node.type !== type ? createParenthesizedTypeNode(type) : node;
}
function updateFunctionTypeNode(node, typeParameters, parameters, type) {
  return node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createFunctionTypeNode(typeParameters, parameters, type) : node;
}
function updateConstructorTypeNode(node, modifiers, typeParameters, parameters, type) {
  return node.modifiers !== modifiers || node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createConstructorTypeNode(modifiers, typeParameters, parameters, type) : node;
}
function updateTemplateLiteralTypeNode(node, head, templateSpans) {
  return node.head !== head || node.templateSpans !== templateSpans ? createTemplateLiteralTypeNode(head, templateSpans) : node;
}
function updateTemplateLiteralTypeSpan(node, type, literal) {
  return node.type !== type || node.literal !== literal ? createTemplateLiteralTypeSpan(type, literal) : node;
}
function updateSyntheticExpression(node, tupleNameSource) {
  return node.tupleNameSource !== tupleNameSource ? createSyntheticExpression(node.type, node.isSpread, tupleNameSource) : node;
}
function updatePartiallyEmittedExpression(node, expression) {
  return node.expression !== expression ? createPartiallyEmittedExpression(expression) : node;
}
function updateJsxElement(node, openingElement, children, closingElement) {
  return node.openingElement !== openingElement || node.children !== children || node.closingElement !== closingElement ? createJsxElement(openingElement, children, closingElement) : node;
}
function updateJsxAttributes(node, properties) {
  return node.properties !== properties ? createJsxAttributes(properties) : node;
}
function updateJsxNamespacedName(node, namespace, name) {
  return node.namespace !== namespace || node.name !== name ? createJsxNamespacedName(namespace, name) : node;
}
function updateJsxOpeningElement(node, tagName, typeArguments, attributes) {
  return node.tagName !== tagName || node.typeArguments !== typeArguments || node.attributes !== attributes ? createJsxOpeningElement(tagName, typeArguments, attributes) : node;
}
function updateJsxSelfClosingElement(node, tagName, typeArguments, attributes) {
  return node.tagName !== tagName || node.typeArguments !== typeArguments || node.attributes !== attributes ? createJsxSelfClosingElement(tagName, typeArguments, attributes) : node;
}
function updateJsxFragment(node, openingFragment, children, closingFragment) {
  return node.openingFragment !== openingFragment || node.children !== children || node.closingFragment !== closingFragment ? createJsxFragment(openingFragment, children, closingFragment) : node;
}
function updateJsxAttribute(node, name, initializer) {
  return node.name !== name || node.initializer !== initializer ? createJsxAttribute(name, initializer) : node;
}
function updateJsxSpreadAttribute(node, expression) {
  return node.expression !== expression ? createJsxSpreadAttribute(expression) : node;
}
function updateJsxClosingElement(node, tagName) {
  return node.tagName !== tagName ? createJsxClosingElement(tagName) : node;
}
function updateJsxExpression(node, dotDotDotToken, expression) {
  return node.dotDotDotToken !== dotDotDotToken || node.expression !== expression ? createJsxExpression(dotDotDotToken, expression) : node;
}
function updateSyntaxList(node, children) {
  return node.children !== children ? createSyntaxList(children) : node;
}
function updateJSDoc(node, comment, tags) {
  return node.comment !== comment || node.tags !== tags ? createJSDoc(comment, tags) : node;
}
function updateJSDocTypeExpression(node, type) {
  return node.type !== type ? createJSDocTypeExpression(type) : node;
}
function updateJSDocNonNullableType(node, type) {
  return node.type !== type ? createJSDocNonNullableType(type) : node;
}
function updateJSDocNullableType(node, type) {
  return node.type !== type ? createJSDocNullableType(type) : node;
}
function updateJSDocVariadicType(node, type) {
  return node.type !== type ? createJSDocVariadicType(type) : node;
}
function updateJSDocOptionalType(node, type) {
  return node.type !== type ? createJSDocOptionalType(type) : node;
}
function updateJSDocTypeTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocTypeTag(tagName, typeExpression, comment) : node;
}
function updateJSDocUnknownTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocUnknownTag(tagName, comment) : node;
}
function updateJSDocTemplateTag(node, tagName, constraint, typeParameters, comment) {
  return node.tagName !== tagName || node.constraint !== constraint || node.typeParameters !== typeParameters || node.comment !== comment ? createJSDocTemplateTag(tagName, constraint, typeParameters, comment) : node;
}
function updateJSDocReturnTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocReturnTag(tagName, typeExpression, comment) : node;
}
function updateJSDocPublicTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocPublicTag(tagName, comment) : node;
}
function updateJSDocPrivateTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocPrivateTag(tagName, comment) : node;
}
function updateJSDocProtectedTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocProtectedTag(tagName, comment) : node;
}
function updateJSDocReadonlyTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocReadonlyTag(tagName, comment) : node;
}
function updateJSDocOverrideTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocOverrideTag(tagName, comment) : node;
}
function updateJSDocDeprecatedTag(node, tagName, comment) {
  return node.tagName !== tagName || node.comment !== comment ? createJSDocDeprecatedTag(tagName, comment) : node;
}
function updateJSDocSeeTag(node, tagName, nameExpression, comment) {
  return node.tagName !== tagName || node.nameExpression !== nameExpression || node.comment !== comment ? createJSDocSeeTag(tagName, nameExpression, comment) : node;
}
function updateJSDocImplementsTag(node, tagName, className, comment) {
  return node.tagName !== tagName || node.className !== className || node.comment !== comment ? createJSDocImplementsTag(tagName, className, comment) : node;
}
function updateJSDocAugmentsTag(node, tagName, className, comment) {
  return node.tagName !== tagName || node.className !== className || node.comment !== comment ? createJSDocAugmentsTag(tagName, className, comment) : node;
}
function updateJSDocSatisfiesTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocSatisfiesTag(tagName, typeExpression, comment) : node;
}
function updateJSDocThrowsTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocThrowsTag(tagName, typeExpression, comment) : node;
}
function updateJSDocThisTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocThisTag(tagName, typeExpression, comment) : node;
}
function updateJSDocImportTag(node, tagName, importClause, moduleSpecifier, attributes, comment) {
  return node.tagName !== tagName || node.importClause !== importClause || node.moduleSpecifier !== moduleSpecifier || node.attributes !== attributes || node.comment !== comment ? createJSDocImportTag(tagName, importClause, moduleSpecifier, attributes, comment) : node;
}
function updateJSDocCallbackTag(node, tagName, typeExpression, name, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.name !== name || node.comment !== comment ? createJSDocCallbackTag(tagName, typeExpression, name, comment) : node;
}
function updateJSDocOverloadTag(node, tagName, typeExpression, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocOverloadTag(tagName, typeExpression, comment) : node;
}
function updateJSDocTypedefTag(node, tagName, typeExpression, name, comment) {
  return node.tagName !== tagName || node.typeExpression !== typeExpression || node.name !== name || node.comment !== comment ? createJSDocTypedefTag(tagName, typeExpression, name, comment) : node;
}
function updateJSDocSignature(node, typeParameters, parameters, type) {
  return node.typeParameters !== typeParameters || node.parameters !== parameters || node.type !== type ? createJSDocSignature(typeParameters, parameters, type) : node;
}
function updateJSDocNameReference(node, name) {
  return node.name !== name ? createJSDocNameReference(name) : node;
}
function updateModuleDeclaration(node, modifiers, name, body) {
  return node.modifiers !== modifiers || node.name !== name || node.body !== body ? createModuleDeclaration(modifiers, node.keyword, name, body) : node;
}
function updateImportEqualsDeclaration(node, modifiers, name, moduleReference) {
  return node.modifiers !== modifiers || node.name !== name || node.moduleReference !== moduleReference ? createImportEqualsDeclaration(modifiers, node.isTypeOnly, name, moduleReference) : node;
}
function updateExportDeclaration(node, modifiers, exportClause, moduleSpecifier, attributes) {
  return node.modifiers !== modifiers || node.exportClause !== exportClause || node.moduleSpecifier !== moduleSpecifier || node.attributes !== attributes ? createExportDeclaration(modifiers, node.isTypeOnly, exportClause, moduleSpecifier, attributes) : node;
}
function updateImportTypeNode(node, argument, attributes, qualifier, typeArguments) {
  return node.argument !== argument || node.attributes !== attributes || node.qualifier !== qualifier || node.typeArguments !== typeArguments ? createImportTypeNode(node.isTypeOf, argument, attributes, qualifier, typeArguments) : node;
}
function updateImportClause(node, name, namedBindings) {
  return node.name !== name || node.namedBindings !== namedBindings ? createImportClause(node.phaseModifier, name, namedBindings) : node;
}
function updateImportSpecifier(node, propertyName, name) {
  return node.propertyName !== propertyName || node.name !== name ? createImportSpecifier(node.isTypeOnly, propertyName, name) : node;
}
function updateJSDocLink(node, name) {
  return node.name !== name ? createJSDocLink(name, node.text) : node;
}
function updateJSDocLinkPlain(node, name) {
  return node.name !== name ? createJSDocLinkPlain(name, node.text) : node;
}
function updateJSDocLinkCode(node, name) {
  return node.name !== name ? createJSDocLinkCode(name, node.text) : node;
}
function updateTypeParameterDeclaration(node, modifiers, name, constraint, expression, defaultType) {
  return node.modifiers !== modifiers || node.name !== name || node.constraint !== constraint || node.expression !== expression || node.defaultType !== defaultType ? createTypeParameterDeclaration(modifiers, name, constraint, expression, defaultType) : node;
}
function updateSyntheticReferenceExpression(node, expression, thisArg) {
  return node.expression !== expression || node.thisArg !== thisArg ? createSyntheticReferenceExpression(expression, thisArg) : node;
}
function updateJSDocTypeLiteral(node, jsdocPropertyTags) {
  return node.jsdocPropertyTags !== jsdocPropertyTags ? createJSDocTypeLiteral(jsdocPropertyTags, node.isArrayType) : node;
}
function updateForInStatement(node, awaitModifier, initializer, expression, statement) {
  return node.awaitModifier !== awaitModifier || node.initializer !== initializer || node.expression !== expression || node.statement !== statement ? createForInStatement(awaitModifier, initializer, expression, statement) : node;
}
function updateForOfStatement(node, awaitModifier, initializer, expression, statement) {
  return node.awaitModifier !== awaitModifier || node.initializer !== initializer || node.expression !== expression || node.statement !== statement ? createForOfStatement(awaitModifier, initializer, expression, statement) : node;
}
function updateCaseClause(node, expression, statements) {
  return node.expression !== expression || node.statements !== statements ? createCaseClause(expression, statements) : node;
}
function updateDefaultClause(node, expression, statements) {
  return node.expression !== expression || node.statements !== statements ? createDefaultClause(expression, statements) : node;
}
function updateObjectBindingPattern(node, elements) {
  return node.elements !== elements ? createObjectBindingPattern(elements) : node;
}
function updateArrayBindingPattern(node, elements) {
  return node.elements !== elements ? createArrayBindingPattern(elements) : node;
}
function updateJSDocParameterTag(node, tagName, name, typeExpression, comment) {
  return node.tagName !== tagName || node.name !== name || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocParameterTag(tagName, name, node.isBracketed, typeExpression, node.isNameFirst, comment) : node;
}
function updateJSDocPropertyTag(node, tagName, name, typeExpression, comment) {
  return node.tagName !== tagName || node.name !== name || node.typeExpression !== typeExpression || node.comment !== comment ? createJSDocPropertyTag(tagName, name, node.isBracketed, typeExpression, node.isNameFirst, comment) : node;
}
function cloneSourceFileWithChanges(source, statements, endOfFileToken) {
  return new NodeObject(SyntaxKind.SourceFile, {
    ...cloneSourceFileData(source),
    statements: createNodeArray(statements),
    endOfFileToken
  });
}
function updateSourceFile(node, statements, endOfFileToken) {
  return node.statements !== statements || node.endOfFileToken !== endOfFileToken ? cloneSourceFileWithChanges(node, statements, endOfFileToken) : node;
}

// node_modules/typescript/dist/ast/scanner.js
var EscapeSequenceScanningFlags = {
  String: 1 << 0,
  ReportErrors: 1 << 1,
  RegularExpression: 1 << 2,
  AnnexB: 1 << 3,
  AnyUnicodeMode: 1 << 4,
  AtomEscape: 1 << 5,
  ReportInvalidEscapeErrors: 1 << 2 | 1 << 1,
  AllowExtendedUnicodeEscape: 1 << 0 | 1 << 4
};
function tokenIsIdentifierOrKeyword(token) {
  return token >= SyntaxKind.Identifier;
}
function tokenIsIdentifierOrKeywordOrGreaterThan(token) {
  return token === SyntaxKind.GreaterThanToken || tokenIsIdentifierOrKeyword(token);
}
var textToKeywordObj = {
  abstract: SyntaxKind.AbstractKeyword,
  accessor: SyntaxKind.AccessorKeyword,
  any: SyntaxKind.AnyKeyword,
  as: SyntaxKind.AsKeyword,
  asserts: SyntaxKind.AssertsKeyword,
  assert: SyntaxKind.AssertKeyword,
  bigint: SyntaxKind.BigIntKeyword,
  boolean: SyntaxKind.BooleanKeyword,
  break: SyntaxKind.BreakKeyword,
  case: SyntaxKind.CaseKeyword,
  catch: SyntaxKind.CatchKeyword,
  class: SyntaxKind.ClassKeyword,
  continue: SyntaxKind.ContinueKeyword,
  const: SyntaxKind.ConstKeyword,
  ["constructor"]: SyntaxKind.ConstructorKeyword,
  debugger: SyntaxKind.DebuggerKeyword,
  declare: SyntaxKind.DeclareKeyword,
  default: SyntaxKind.DefaultKeyword,
  defer: SyntaxKind.DeferKeyword,
  delete: SyntaxKind.DeleteKeyword,
  do: SyntaxKind.DoKeyword,
  else: SyntaxKind.ElseKeyword,
  enum: SyntaxKind.EnumKeyword,
  export: SyntaxKind.ExportKeyword,
  extends: SyntaxKind.ExtendsKeyword,
  false: SyntaxKind.FalseKeyword,
  finally: SyntaxKind.FinallyKeyword,
  for: SyntaxKind.ForKeyword,
  from: SyntaxKind.FromKeyword,
  function: SyntaxKind.FunctionKeyword,
  get: SyntaxKind.GetKeyword,
  if: SyntaxKind.IfKeyword,
  implements: SyntaxKind.ImplementsKeyword,
  import: SyntaxKind.ImportKeyword,
  in: SyntaxKind.InKeyword,
  infer: SyntaxKind.InferKeyword,
  instanceof: SyntaxKind.InstanceOfKeyword,
  interface: SyntaxKind.InterfaceKeyword,
  intrinsic: SyntaxKind.IntrinsicKeyword,
  is: SyntaxKind.IsKeyword,
  keyof: SyntaxKind.KeyOfKeyword,
  let: SyntaxKind.LetKeyword,
  module: SyntaxKind.ModuleKeyword,
  namespace: SyntaxKind.NamespaceKeyword,
  never: SyntaxKind.NeverKeyword,
  new: SyntaxKind.NewKeyword,
  null: SyntaxKind.NullKeyword,
  number: SyntaxKind.NumberKeyword,
  object: SyntaxKind.ObjectKeyword,
  package: SyntaxKind.PackageKeyword,
  private: SyntaxKind.PrivateKeyword,
  protected: SyntaxKind.ProtectedKeyword,
  public: SyntaxKind.PublicKeyword,
  override: SyntaxKind.OverrideKeyword,
  out: SyntaxKind.OutKeyword,
  readonly: SyntaxKind.ReadonlyKeyword,
  require: SyntaxKind.RequireKeyword,
  global: SyntaxKind.GlobalKeyword,
  return: SyntaxKind.ReturnKeyword,
  satisfies: SyntaxKind.SatisfiesKeyword,
  set: SyntaxKind.SetKeyword,
  static: SyntaxKind.StaticKeyword,
  string: SyntaxKind.StringKeyword,
  super: SyntaxKind.SuperKeyword,
  switch: SyntaxKind.SwitchKeyword,
  symbol: SyntaxKind.SymbolKeyword,
  this: SyntaxKind.ThisKeyword,
  throw: SyntaxKind.ThrowKeyword,
  true: SyntaxKind.TrueKeyword,
  try: SyntaxKind.TryKeyword,
  type: SyntaxKind.TypeKeyword,
  typeof: SyntaxKind.TypeOfKeyword,
  undefined: SyntaxKind.UndefinedKeyword,
  unique: SyntaxKind.UniqueKeyword,
  unknown: SyntaxKind.UnknownKeyword,
  using: SyntaxKind.UsingKeyword,
  var: SyntaxKind.VarKeyword,
  void: SyntaxKind.VoidKeyword,
  while: SyntaxKind.WhileKeyword,
  with: SyntaxKind.WithKeyword,
  yield: SyntaxKind.YieldKeyword,
  async: SyntaxKind.AsyncKeyword,
  await: SyntaxKind.AwaitKeyword,
  of: SyntaxKind.OfKeyword
};
var textToKeyword = new Map(Object.entries(textToKeywordObj));
var textToToken = new Map(Object.entries({
  ...textToKeywordObj,
  "{": SyntaxKind.OpenBraceToken,
  "}": SyntaxKind.CloseBraceToken,
  "(": SyntaxKind.OpenParenToken,
  ")": SyntaxKind.CloseParenToken,
  "[": SyntaxKind.OpenBracketToken,
  "]": SyntaxKind.CloseBracketToken,
  ".": SyntaxKind.DotToken,
  "...": SyntaxKind.DotDotDotToken,
  ";": SyntaxKind.SemicolonToken,
  ",": SyntaxKind.CommaToken,
  "<": SyntaxKind.LessThanToken,
  ">": SyntaxKind.GreaterThanToken,
  "<=": SyntaxKind.LessThanEqualsToken,
  ">=": SyntaxKind.GreaterThanEqualsToken,
  "==": SyntaxKind.EqualsEqualsToken,
  "!=": SyntaxKind.ExclamationEqualsToken,
  "===": SyntaxKind.EqualsEqualsEqualsToken,
  "!==": SyntaxKind.ExclamationEqualsEqualsToken,
  "=>": SyntaxKind.EqualsGreaterThanToken,
  "+": SyntaxKind.PlusToken,
  "-": SyntaxKind.MinusToken,
  "**": SyntaxKind.AsteriskAsteriskToken,
  "*": SyntaxKind.AsteriskToken,
  "/": SyntaxKind.SlashToken,
  "%": SyntaxKind.PercentToken,
  "++": SyntaxKind.PlusPlusToken,
  "--": SyntaxKind.MinusMinusToken,
  "<<": SyntaxKind.LessThanLessThanToken,
  "</": SyntaxKind.LessThanSlashToken,
  ">>": SyntaxKind.GreaterThanGreaterThanToken,
  ">>>": SyntaxKind.GreaterThanGreaterThanGreaterThanToken,
  "&": SyntaxKind.AmpersandToken,
  "|": SyntaxKind.BarToken,
  "^": SyntaxKind.CaretToken,
  "!": SyntaxKind.ExclamationToken,
  "~": SyntaxKind.TildeToken,
  "&&": SyntaxKind.AmpersandAmpersandToken,
  "||": SyntaxKind.BarBarToken,
  "?": SyntaxKind.QuestionToken,
  "??": SyntaxKind.QuestionQuestionToken,
  "?.": SyntaxKind.QuestionDotToken,
  ":": SyntaxKind.ColonToken,
  "=": SyntaxKind.EqualsToken,
  "+=": SyntaxKind.PlusEqualsToken,
  "-=": SyntaxKind.MinusEqualsToken,
  "*=": SyntaxKind.AsteriskEqualsToken,
  "**=": SyntaxKind.AsteriskAsteriskEqualsToken,
  "/=": SyntaxKind.SlashEqualsToken,
  "%=": SyntaxKind.PercentEqualsToken,
  "<<=": SyntaxKind.LessThanLessThanEqualsToken,
  ">>=": SyntaxKind.GreaterThanGreaterThanEqualsToken,
  ">>>=": SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken,
  "&=": SyntaxKind.AmpersandEqualsToken,
  "|=": SyntaxKind.BarEqualsToken,
  "^=": SyntaxKind.CaretEqualsToken,
  "||=": SyntaxKind.BarBarEqualsToken,
  "&&=": SyntaxKind.AmpersandAmpersandEqualsToken,
  "??=": SyntaxKind.QuestionQuestionEqualsToken,
  "@": SyntaxKind.AtToken,
  "#": SyntaxKind.HashToken,
  "`": SyntaxKind.BacktickToken
}));
var charCodeToRegExpFlag = /* @__PURE__ */ new Map([
  [CharacterCodes.d, RegularExpressionFlags.HasIndices],
  [CharacterCodes.g, RegularExpressionFlags.Global],
  [CharacterCodes.i, RegularExpressionFlags.IgnoreCase],
  [CharacterCodes.m, RegularExpressionFlags.Multiline],
  [CharacterCodes.s, RegularExpressionFlags.DotAll],
  [CharacterCodes.u, RegularExpressionFlags.Unicode],
  [CharacterCodes.v, RegularExpressionFlags.UnicodeSets],
  [CharacterCodes.y, RegularExpressionFlags.Sticky]
]);
var unicodeESNextIdentifierStart = [65, 90, 97, 122, 170, 170, 181, 181, 186, 186, 192, 214, 216, 246, 248, 705, 710, 721, 736, 740, 748, 748, 750, 750, 880, 884, 886, 887, 890, 893, 895, 895, 902, 902, 904, 906, 908, 908, 910, 929, 931, 1013, 1015, 1153, 1162, 1327, 1329, 1366, 1369, 1369, 1376, 1416, 1488, 1514, 1519, 1522, 1568, 1610, 1646, 1647, 1649, 1747, 1749, 1749, 1765, 1766, 1774, 1775, 1786, 1788, 1791, 1791, 1808, 1808, 1810, 1839, 1869, 1957, 1969, 1969, 1994, 2026, 2036, 2037, 2042, 2042, 2048, 2069, 2074, 2074, 2084, 2084, 2088, 2088, 2112, 2136, 2144, 2154, 2160, 2183, 2185, 2190, 2208, 2249, 2308, 2361, 2365, 2365, 2384, 2384, 2392, 2401, 2417, 2432, 2437, 2444, 2447, 2448, 2451, 2472, 2474, 2480, 2482, 2482, 2486, 2489, 2493, 2493, 2510, 2510, 2524, 2525, 2527, 2529, 2544, 2545, 2556, 2556, 2565, 2570, 2575, 2576, 2579, 2600, 2602, 2608, 2610, 2611, 2613, 2614, 2616, 2617, 2649, 2652, 2654, 2654, 2674, 2676, 2693, 2701, 2703, 2705, 2707, 2728, 2730, 2736, 2738, 2739, 2741, 2745, 2749, 2749, 2768, 2768, 2784, 2785, 2809, 2809, 2821, 2828, 2831, 2832, 2835, 2856, 2858, 2864, 2866, 2867, 2869, 2873, 2877, 2877, 2908, 2909, 2911, 2913, 2929, 2929, 2947, 2947, 2949, 2954, 2958, 2960, 2962, 2965, 2969, 2970, 2972, 2972, 2974, 2975, 2979, 2980, 2984, 2986, 2990, 3001, 3024, 3024, 3077, 3084, 3086, 3088, 3090, 3112, 3114, 3129, 3133, 3133, 3160, 3162, 3165, 3165, 3168, 3169, 3200, 3200, 3205, 3212, 3214, 3216, 3218, 3240, 3242, 3251, 3253, 3257, 3261, 3261, 3293, 3294, 3296, 3297, 3313, 3314, 3332, 3340, 3342, 3344, 3346, 3386, 3389, 3389, 3406, 3406, 3412, 3414, 3423, 3425, 3450, 3455, 3461, 3478, 3482, 3505, 3507, 3515, 3517, 3517, 3520, 3526, 3585, 3632, 3634, 3635, 3648, 3654, 3713, 3714, 3716, 3716, 3718, 3722, 3724, 3747, 3749, 3749, 3751, 3760, 3762, 3763, 3773, 3773, 3776, 3780, 3782, 3782, 3804, 3807, 3840, 3840, 3904, 3911, 3913, 3948, 3976, 3980, 4096, 4138, 4159, 4159, 4176, 4181, 4186, 4189, 4193, 4193, 4197, 4198, 4206, 4208, 4213, 4225, 4238, 4238, 4256, 4293, 4295, 4295, 4301, 4301, 4304, 4346, 4348, 4680, 4682, 4685, 4688, 4694, 4696, 4696, 4698, 4701, 4704, 4744, 4746, 4749, 4752, 4784, 4786, 4789, 4792, 4798, 4800, 4800, 4802, 4805, 4808, 4822, 4824, 4880, 4882, 4885, 4888, 4954, 4992, 5007, 5024, 5109, 5112, 5117, 5121, 5740, 5743, 5759, 5761, 5786, 5792, 5866, 5870, 5880, 5888, 5905, 5919, 5937, 5952, 5969, 5984, 5996, 5998, 6e3, 6016, 6067, 6103, 6103, 6108, 6108, 6176, 6264, 6272, 6312, 6314, 6314, 6320, 6389, 6400, 6430, 6480, 6509, 6512, 6516, 6528, 6571, 6576, 6601, 6656, 6678, 6688, 6740, 6823, 6823, 6917, 6963, 6981, 6988, 7043, 7072, 7086, 7087, 7098, 7141, 7168, 7203, 7245, 7247, 7258, 7293, 7296, 7304, 7312, 7354, 7357, 7359, 7401, 7404, 7406, 7411, 7413, 7414, 7418, 7418, 7424, 7615, 7680, 7957, 7960, 7965, 7968, 8005, 8008, 8013, 8016, 8023, 8025, 8025, 8027, 8027, 8029, 8029, 8031, 8061, 8064, 8116, 8118, 8124, 8126, 8126, 8130, 8132, 8134, 8140, 8144, 8147, 8150, 8155, 8160, 8172, 8178, 8180, 8182, 8188, 8305, 8305, 8319, 8319, 8336, 8348, 8450, 8450, 8455, 8455, 8458, 8467, 8469, 8469, 8472, 8477, 8484, 8484, 8486, 8486, 8488, 8488, 8490, 8505, 8508, 8511, 8517, 8521, 8526, 8526, 8544, 8584, 11264, 11492, 11499, 11502, 11506, 11507, 11520, 11557, 11559, 11559, 11565, 11565, 11568, 11623, 11631, 11631, 11648, 11670, 11680, 11686, 11688, 11694, 11696, 11702, 11704, 11710, 11712, 11718, 11720, 11726, 11728, 11734, 11736, 11742, 12293, 12295, 12321, 12329, 12337, 12341, 12344, 12348, 12353, 12438, 12443, 12447, 12449, 12538, 12540, 12543, 12549, 12591, 12593, 12686, 12704, 12735, 12784, 12799, 13312, 19903, 19968, 42124, 42192, 42237, 42240, 42508, 42512, 42527, 42538, 42539, 42560, 42606, 42623, 42653, 42656, 42735, 42775, 42783, 42786, 42888, 42891, 42954, 42960, 42961, 42963, 42963, 42965, 42969, 42994, 43009, 43011, 43013, 43015, 43018, 43020, 43042, 43072, 43123, 43138, 43187, 43250, 43255, 43259, 43259, 43261, 43262, 43274, 43301, 43312, 43334, 43360, 43388, 43396, 43442, 43471, 43471, 43488, 43492, 43494, 43503, 43514, 43518, 43520, 43560, 43584, 43586, 43588, 43595, 43616, 43638, 43642, 43642, 43646, 43695, 43697, 43697, 43701, 43702, 43705, 43709, 43712, 43712, 43714, 43714, 43739, 43741, 43744, 43754, 43762, 43764, 43777, 43782, 43785, 43790, 43793, 43798, 43808, 43814, 43816, 43822, 43824, 43866, 43868, 43881, 43888, 44002, 44032, 55203, 55216, 55238, 55243, 55291, 63744, 64109, 64112, 64217, 64256, 64262, 64275, 64279, 64285, 64285, 64287, 64296, 64298, 64310, 64312, 64316, 64318, 64318, 64320, 64321, 64323, 64324, 64326, 64433, 64467, 64829, 64848, 64911, 64914, 64967, 65008, 65019, 65136, 65140, 65142, 65276, 65313, 65338, 65345, 65370, 65382, 65470, 65474, 65479, 65482, 65487, 65490, 65495, 65498, 65500, 65536, 65547, 65549, 65574, 65576, 65594, 65596, 65597, 65599, 65613, 65616, 65629, 65664, 65786, 65856, 65908, 66176, 66204, 66208, 66256, 66304, 66335, 66349, 66378, 66384, 66421, 66432, 66461, 66464, 66499, 66504, 66511, 66513, 66517, 66560, 66717, 66736, 66771, 66776, 66811, 66816, 66855, 66864, 66915, 66928, 66938, 66940, 66954, 66956, 66962, 66964, 66965, 66967, 66977, 66979, 66993, 66995, 67001, 67003, 67004, 67072, 67382, 67392, 67413, 67424, 67431, 67456, 67461, 67463, 67504, 67506, 67514, 67584, 67589, 67592, 67592, 67594, 67637, 67639, 67640, 67644, 67644, 67647, 67669, 67680, 67702, 67712, 67742, 67808, 67826, 67828, 67829, 67840, 67861, 67872, 67897, 67968, 68023, 68030, 68031, 68096, 68096, 68112, 68115, 68117, 68119, 68121, 68149, 68192, 68220, 68224, 68252, 68288, 68295, 68297, 68324, 68352, 68405, 68416, 68437, 68448, 68466, 68480, 68497, 68608, 68680, 68736, 68786, 68800, 68850, 68864, 68899, 69248, 69289, 69296, 69297, 69376, 69404, 69415, 69415, 69424, 69445, 69488, 69505, 69552, 69572, 69600, 69622, 69635, 69687, 69745, 69746, 69749, 69749, 69763, 69807, 69840, 69864, 69891, 69926, 69956, 69956, 69959, 69959, 69968, 70002, 70006, 70006, 70019, 70066, 70081, 70084, 70106, 70106, 70108, 70108, 70144, 70161, 70163, 70187, 70207, 70208, 70272, 70278, 70280, 70280, 70282, 70285, 70287, 70301, 70303, 70312, 70320, 70366, 70405, 70412, 70415, 70416, 70419, 70440, 70442, 70448, 70450, 70451, 70453, 70457, 70461, 70461, 70480, 70480, 70493, 70497, 70656, 70708, 70727, 70730, 70751, 70753, 70784, 70831, 70852, 70853, 70855, 70855, 71040, 71086, 71128, 71131, 71168, 71215, 71236, 71236, 71296, 71338, 71352, 71352, 71424, 71450, 71488, 71494, 71680, 71723, 71840, 71903, 71935, 71942, 71945, 71945, 71948, 71955, 71957, 71958, 71960, 71983, 71999, 71999, 72001, 72001, 72096, 72103, 72106, 72144, 72161, 72161, 72163, 72163, 72192, 72192, 72203, 72242, 72250, 72250, 72272, 72272, 72284, 72329, 72349, 72349, 72368, 72440, 72704, 72712, 72714, 72750, 72768, 72768, 72818, 72847, 72960, 72966, 72968, 72969, 72971, 73008, 73030, 73030, 73056, 73061, 73063, 73064, 73066, 73097, 73112, 73112, 73440, 73458, 73474, 73474, 73476, 73488, 73490, 73523, 73648, 73648, 73728, 74649, 74752, 74862, 74880, 75075, 77712, 77808, 77824, 78895, 78913, 78918, 82944, 83526, 92160, 92728, 92736, 92766, 92784, 92862, 92880, 92909, 92928, 92975, 92992, 92995, 93027, 93047, 93053, 93071, 93760, 93823, 93952, 94026, 94032, 94032, 94099, 94111, 94176, 94177, 94179, 94179, 94208, 100343, 100352, 101589, 101632, 101640, 110576, 110579, 110581, 110587, 110589, 110590, 110592, 110882, 110898, 110898, 110928, 110930, 110933, 110933, 110948, 110951, 110960, 111355, 113664, 113770, 113776, 113788, 113792, 113800, 113808, 113817, 119808, 119892, 119894, 119964, 119966, 119967, 119970, 119970, 119973, 119974, 119977, 119980, 119982, 119993, 119995, 119995, 119997, 120003, 120005, 120069, 120071, 120074, 120077, 120084, 120086, 120092, 120094, 120121, 120123, 120126, 120128, 120132, 120134, 120134, 120138, 120144, 120146, 120485, 120488, 120512, 120514, 120538, 120540, 120570, 120572, 120596, 120598, 120628, 120630, 120654, 120656, 120686, 120688, 120712, 120714, 120744, 120746, 120770, 120772, 120779, 122624, 122654, 122661, 122666, 122928, 122989, 123136, 123180, 123191, 123197, 123214, 123214, 123536, 123565, 123584, 123627, 124112, 124139, 124896, 124902, 124904, 124907, 124909, 124910, 124912, 124926, 124928, 125124, 125184, 125251, 125259, 125259, 126464, 126467, 126469, 126495, 126497, 126498, 126500, 126500, 126503, 126503, 126505, 126514, 126516, 126519, 126521, 126521, 126523, 126523, 126530, 126530, 126535, 126535, 126537, 126537, 126539, 126539, 126541, 126543, 126545, 126546, 126548, 126548, 126551, 126551, 126553, 126553, 126555, 126555, 126557, 126557, 126559, 126559, 126561, 126562, 126564, 126564, 126567, 126570, 126572, 126578, 126580, 126583, 126585, 126588, 126590, 126590, 126592, 126601, 126603, 126619, 126625, 126627, 126629, 126633, 126635, 126651, 131072, 173791, 173824, 177977, 177984, 178205, 178208, 183969, 183984, 191456, 191472, 192093, 194560, 195101, 196608, 201546, 201552, 205743];
var unicodeESNextIdentifierPart = [48, 57, 65, 90, 95, 95, 97, 122, 170, 170, 181, 181, 183, 183, 186, 186, 192, 214, 216, 246, 248, 705, 710, 721, 736, 740, 748, 748, 750, 750, 768, 884, 886, 887, 890, 893, 895, 895, 902, 906, 908, 908, 910, 929, 931, 1013, 1015, 1153, 1155, 1159, 1162, 1327, 1329, 1366, 1369, 1369, 1376, 1416, 1425, 1469, 1471, 1471, 1473, 1474, 1476, 1477, 1479, 1479, 1488, 1514, 1519, 1522, 1552, 1562, 1568, 1641, 1646, 1747, 1749, 1756, 1759, 1768, 1770, 1788, 1791, 1791, 1808, 1866, 1869, 1969, 1984, 2037, 2042, 2042, 2045, 2045, 2048, 2093, 2112, 2139, 2144, 2154, 2160, 2183, 2185, 2190, 2200, 2273, 2275, 2403, 2406, 2415, 2417, 2435, 2437, 2444, 2447, 2448, 2451, 2472, 2474, 2480, 2482, 2482, 2486, 2489, 2492, 2500, 2503, 2504, 2507, 2510, 2519, 2519, 2524, 2525, 2527, 2531, 2534, 2545, 2556, 2556, 2558, 2558, 2561, 2563, 2565, 2570, 2575, 2576, 2579, 2600, 2602, 2608, 2610, 2611, 2613, 2614, 2616, 2617, 2620, 2620, 2622, 2626, 2631, 2632, 2635, 2637, 2641, 2641, 2649, 2652, 2654, 2654, 2662, 2677, 2689, 2691, 2693, 2701, 2703, 2705, 2707, 2728, 2730, 2736, 2738, 2739, 2741, 2745, 2748, 2757, 2759, 2761, 2763, 2765, 2768, 2768, 2784, 2787, 2790, 2799, 2809, 2815, 2817, 2819, 2821, 2828, 2831, 2832, 2835, 2856, 2858, 2864, 2866, 2867, 2869, 2873, 2876, 2884, 2887, 2888, 2891, 2893, 2901, 2903, 2908, 2909, 2911, 2915, 2918, 2927, 2929, 2929, 2946, 2947, 2949, 2954, 2958, 2960, 2962, 2965, 2969, 2970, 2972, 2972, 2974, 2975, 2979, 2980, 2984, 2986, 2990, 3001, 3006, 3010, 3014, 3016, 3018, 3021, 3024, 3024, 3031, 3031, 3046, 3055, 3072, 3084, 3086, 3088, 3090, 3112, 3114, 3129, 3132, 3140, 3142, 3144, 3146, 3149, 3157, 3158, 3160, 3162, 3165, 3165, 3168, 3171, 3174, 3183, 3200, 3203, 3205, 3212, 3214, 3216, 3218, 3240, 3242, 3251, 3253, 3257, 3260, 3268, 3270, 3272, 3274, 3277, 3285, 3286, 3293, 3294, 3296, 3299, 3302, 3311, 3313, 3315, 3328, 3340, 3342, 3344, 3346, 3396, 3398, 3400, 3402, 3406, 3412, 3415, 3423, 3427, 3430, 3439, 3450, 3455, 3457, 3459, 3461, 3478, 3482, 3505, 3507, 3515, 3517, 3517, 3520, 3526, 3530, 3530, 3535, 3540, 3542, 3542, 3544, 3551, 3558, 3567, 3570, 3571, 3585, 3642, 3648, 3662, 3664, 3673, 3713, 3714, 3716, 3716, 3718, 3722, 3724, 3747, 3749, 3749, 3751, 3773, 3776, 3780, 3782, 3782, 3784, 3790, 3792, 3801, 3804, 3807, 3840, 3840, 3864, 3865, 3872, 3881, 3893, 3893, 3895, 3895, 3897, 3897, 3902, 3911, 3913, 3948, 3953, 3972, 3974, 3991, 3993, 4028, 4038, 4038, 4096, 4169, 4176, 4253, 4256, 4293, 4295, 4295, 4301, 4301, 4304, 4346, 4348, 4680, 4682, 4685, 4688, 4694, 4696, 4696, 4698, 4701, 4704, 4744, 4746, 4749, 4752, 4784, 4786, 4789, 4792, 4798, 4800, 4800, 4802, 4805, 4808, 4822, 4824, 4880, 4882, 4885, 4888, 4954, 4957, 4959, 4969, 4977, 4992, 5007, 5024, 5109, 5112, 5117, 5121, 5740, 5743, 5759, 5761, 5786, 5792, 5866, 5870, 5880, 5888, 5909, 5919, 5940, 5952, 5971, 5984, 5996, 5998, 6e3, 6002, 6003, 6016, 6099, 6103, 6103, 6108, 6109, 6112, 6121, 6155, 6157, 6159, 6169, 6176, 6264, 6272, 6314, 6320, 6389, 6400, 6430, 6432, 6443, 6448, 6459, 6470, 6509, 6512, 6516, 6528, 6571, 6576, 6601, 6608, 6618, 6656, 6683, 6688, 6750, 6752, 6780, 6783, 6793, 6800, 6809, 6823, 6823, 6832, 6845, 6847, 6862, 6912, 6988, 6992, 7001, 7019, 7027, 7040, 7155, 7168, 7223, 7232, 7241, 7245, 7293, 7296, 7304, 7312, 7354, 7357, 7359, 7376, 7378, 7380, 7418, 7424, 7957, 7960, 7965, 7968, 8005, 8008, 8013, 8016, 8023, 8025, 8025, 8027, 8027, 8029, 8029, 8031, 8061, 8064, 8116, 8118, 8124, 8126, 8126, 8130, 8132, 8134, 8140, 8144, 8147, 8150, 8155, 8160, 8172, 8178, 8180, 8182, 8188, 8204, 8205, 8255, 8256, 8276, 8276, 8305, 8305, 8319, 8319, 8336, 8348, 8400, 8412, 8417, 8417, 8421, 8432, 8450, 8450, 8455, 8455, 8458, 8467, 8469, 8469, 8472, 8477, 8484, 8484, 8486, 8486, 8488, 8488, 8490, 8505, 8508, 8511, 8517, 8521, 8526, 8526, 8544, 8584, 11264, 11492, 11499, 11507, 11520, 11557, 11559, 11559, 11565, 11565, 11568, 11623, 11631, 11631, 11647, 11670, 11680, 11686, 11688, 11694, 11696, 11702, 11704, 11710, 11712, 11718, 11720, 11726, 11728, 11734, 11736, 11742, 11744, 11775, 12293, 12295, 12321, 12335, 12337, 12341, 12344, 12348, 12353, 12438, 12441, 12447, 12449, 12543, 12549, 12591, 12593, 12686, 12704, 12735, 12784, 12799, 13312, 19903, 19968, 42124, 42192, 42237, 42240, 42508, 42512, 42539, 42560, 42607, 42612, 42621, 42623, 42737, 42775, 42783, 42786, 42888, 42891, 42954, 42960, 42961, 42963, 42963, 42965, 42969, 42994, 43047, 43052, 43052, 43072, 43123, 43136, 43205, 43216, 43225, 43232, 43255, 43259, 43259, 43261, 43309, 43312, 43347, 43360, 43388, 43392, 43456, 43471, 43481, 43488, 43518, 43520, 43574, 43584, 43597, 43600, 43609, 43616, 43638, 43642, 43714, 43739, 43741, 43744, 43759, 43762, 43766, 43777, 43782, 43785, 43790, 43793, 43798, 43808, 43814, 43816, 43822, 43824, 43866, 43868, 43881, 43888, 44010, 44012, 44013, 44016, 44025, 44032, 55203, 55216, 55238, 55243, 55291, 63744, 64109, 64112, 64217, 64256, 64262, 64275, 64279, 64285, 64296, 64298, 64310, 64312, 64316, 64318, 64318, 64320, 64321, 64323, 64324, 64326, 64433, 64467, 64829, 64848, 64911, 64914, 64967, 65008, 65019, 65024, 65039, 65056, 65071, 65075, 65076, 65101, 65103, 65136, 65140, 65142, 65276, 65296, 65305, 65313, 65338, 65343, 65343, 65345, 65370, 65381, 65470, 65474, 65479, 65482, 65487, 65490, 65495, 65498, 65500, 65536, 65547, 65549, 65574, 65576, 65594, 65596, 65597, 65599, 65613, 65616, 65629, 65664, 65786, 65856, 65908, 66045, 66045, 66176, 66204, 66208, 66256, 66272, 66272, 66304, 66335, 66349, 66378, 66384, 66426, 66432, 66461, 66464, 66499, 66504, 66511, 66513, 66517, 66560, 66717, 66720, 66729, 66736, 66771, 66776, 66811, 66816, 66855, 66864, 66915, 66928, 66938, 66940, 66954, 66956, 66962, 66964, 66965, 66967, 66977, 66979, 66993, 66995, 67001, 67003, 67004, 67072, 67382, 67392, 67413, 67424, 67431, 67456, 67461, 67463, 67504, 67506, 67514, 67584, 67589, 67592, 67592, 67594, 67637, 67639, 67640, 67644, 67644, 67647, 67669, 67680, 67702, 67712, 67742, 67808, 67826, 67828, 67829, 67840, 67861, 67872, 67897, 67968, 68023, 68030, 68031, 68096, 68099, 68101, 68102, 68108, 68115, 68117, 68119, 68121, 68149, 68152, 68154, 68159, 68159, 68192, 68220, 68224, 68252, 68288, 68295, 68297, 68326, 68352, 68405, 68416, 68437, 68448, 68466, 68480, 68497, 68608, 68680, 68736, 68786, 68800, 68850, 68864, 68903, 68912, 68921, 69248, 69289, 69291, 69292, 69296, 69297, 69373, 69404, 69415, 69415, 69424, 69456, 69488, 69509, 69552, 69572, 69600, 69622, 69632, 69702, 69734, 69749, 69759, 69818, 69826, 69826, 69840, 69864, 69872, 69881, 69888, 69940, 69942, 69951, 69956, 69959, 69968, 70003, 70006, 70006, 70016, 70084, 70089, 70092, 70094, 70106, 70108, 70108, 70144, 70161, 70163, 70199, 70206, 70209, 70272, 70278, 70280, 70280, 70282, 70285, 70287, 70301, 70303, 70312, 70320, 70378, 70384, 70393, 70400, 70403, 70405, 70412, 70415, 70416, 70419, 70440, 70442, 70448, 70450, 70451, 70453, 70457, 70459, 70468, 70471, 70472, 70475, 70477, 70480, 70480, 70487, 70487, 70493, 70499, 70502, 70508, 70512, 70516, 70656, 70730, 70736, 70745, 70750, 70753, 70784, 70853, 70855, 70855, 70864, 70873, 71040, 71093, 71096, 71104, 71128, 71133, 71168, 71232, 71236, 71236, 71248, 71257, 71296, 71352, 71360, 71369, 71424, 71450, 71453, 71467, 71472, 71481, 71488, 71494, 71680, 71738, 71840, 71913, 71935, 71942, 71945, 71945, 71948, 71955, 71957, 71958, 71960, 71989, 71991, 71992, 71995, 72003, 72016, 72025, 72096, 72103, 72106, 72151, 72154, 72161, 72163, 72164, 72192, 72254, 72263, 72263, 72272, 72345, 72349, 72349, 72368, 72440, 72704, 72712, 72714, 72758, 72760, 72768, 72784, 72793, 72818, 72847, 72850, 72871, 72873, 72886, 72960, 72966, 72968, 72969, 72971, 73014, 73018, 73018, 73020, 73021, 73023, 73031, 73040, 73049, 73056, 73061, 73063, 73064, 73066, 73102, 73104, 73105, 73107, 73112, 73120, 73129, 73440, 73462, 73472, 73488, 73490, 73530, 73534, 73538, 73552, 73561, 73648, 73648, 73728, 74649, 74752, 74862, 74880, 75075, 77712, 77808, 77824, 78895, 78912, 78933, 82944, 83526, 92160, 92728, 92736, 92766, 92768, 92777, 92784, 92862, 92864, 92873, 92880, 92909, 92912, 92916, 92928, 92982, 92992, 92995, 93008, 93017, 93027, 93047, 93053, 93071, 93760, 93823, 93952, 94026, 94031, 94087, 94095, 94111, 94176, 94177, 94179, 94180, 94192, 94193, 94208, 100343, 100352, 101589, 101632, 101640, 110576, 110579, 110581, 110587, 110589, 110590, 110592, 110882, 110898, 110898, 110928, 110930, 110933, 110933, 110948, 110951, 110960, 111355, 113664, 113770, 113776, 113788, 113792, 113800, 113808, 113817, 113821, 113822, 118528, 118573, 118576, 118598, 119141, 119145, 119149, 119154, 119163, 119170, 119173, 119179, 119210, 119213, 119362, 119364, 119808, 119892, 119894, 119964, 119966, 119967, 119970, 119970, 119973, 119974, 119977, 119980, 119982, 119993, 119995, 119995, 119997, 120003, 120005, 120069, 120071, 120074, 120077, 120084, 120086, 120092, 120094, 120121, 120123, 120126, 120128, 120132, 120134, 120134, 120138, 120144, 120146, 120485, 120488, 120512, 120514, 120538, 120540, 120570, 120572, 120596, 120598, 120628, 120630, 120654, 120656, 120686, 120688, 120712, 120714, 120744, 120746, 120770, 120772, 120779, 120782, 120831, 121344, 121398, 121403, 121452, 121461, 121461, 121476, 121476, 121499, 121503, 121505, 121519, 122624, 122654, 122661, 122666, 122880, 122886, 122888, 122904, 122907, 122913, 122915, 122916, 122918, 122922, 122928, 122989, 123023, 123023, 123136, 123180, 123184, 123197, 123200, 123209, 123214, 123214, 123536, 123566, 123584, 123641, 124112, 124153, 124896, 124902, 124904, 124907, 124909, 124910, 124912, 124926, 124928, 125124, 125136, 125142, 125184, 125259, 125264, 125273, 126464, 126467, 126469, 126495, 126497, 126498, 126500, 126500, 126503, 126503, 126505, 126514, 126516, 126519, 126521, 126521, 126523, 126523, 126530, 126530, 126535, 126535, 126537, 126537, 126539, 126539, 126541, 126543, 126545, 126546, 126548, 126548, 126551, 126551, 126553, 126553, 126555, 126555, 126557, 126557, 126559, 126559, 126561, 126562, 126564, 126564, 126567, 126570, 126572, 126578, 126580, 126583, 126585, 126588, 126590, 126590, 126592, 126601, 126603, 126619, 126625, 126627, 126629, 126633, 126635, 126651, 130032, 130041, 131072, 173791, 173824, 177977, 177984, 178205, 178208, 183969, 183984, 191456, 191472, 192093, 194560, 195101, 196608, 201546, 201552, 205743, 917760, 917999];
var commentDirectiveRegExSingleLine = /^\/\/\/?\s*@(ts-expect-error|ts-ignore)/;
var commentDirectiveRegExMultiLine = /^(?:\/|\*)*\s*@(ts-expect-error|ts-ignore)/;
var jsDocTagTerminators = /* @__PURE__ */ new Set([" ", "	", "\n", "\r", "}", "*"]);
function hasJSDocTag(text, offset, ...tags) {
  for (const tag of tags) {
    if (text.startsWith(tag, offset)) {
      if (offset + tag.length === text.length) {
        return true;
      }
      if (jsDocTagTerminators.has(text[offset + tag.length])) {
        return true;
      }
    }
  }
  return false;
}
function scanJSDocCommentForTags(text, tokenFlags) {
  let offset = 0;
  while (true) {
    const i = text.indexOf("@", offset);
    if (i < 0) {
      return tokenFlags;
    }
    offset = i + 1;
    if (!(tokenFlags & TokenFlags.PrecedingJSDocWithDeprecated) && hasJSDocTag(text, offset, "deprecated")) {
      tokenFlags |= TokenFlags.PrecedingJSDocWithDeprecated;
    }
    if (!(tokenFlags & TokenFlags.PrecedingJSDocWithSeeOrLink) && hasJSDocTag(text, offset, "see", "link", "linkcode", "linkplain")) {
      tokenFlags |= TokenFlags.PrecedingJSDocWithSeeOrLink;
    }
    if ((tokenFlags & (TokenFlags.PrecedingJSDocWithDeprecated | TokenFlags.PrecedingJSDocWithSeeOrLink)) === (TokenFlags.PrecedingJSDocWithDeprecated | TokenFlags.PrecedingJSDocWithSeeOrLink)) {
      return tokenFlags;
    }
  }
}
function lookupInUnicodeMap(code, map) {
  if (code < map[0]) {
    return false;
  }
  let lo = 0;
  let hi = map.length;
  let mid;
  while (lo + 1 < hi) {
    mid = lo + (hi - lo) / 2;
    mid -= mid % 2;
    if (map[mid] <= code && code <= map[mid + 1]) {
      return true;
    }
    if (code < map[mid]) {
      hi = mid;
    } else {
      lo = mid + 2;
    }
  }
  return false;
}
function isUnicodeIdentifierStart(code) {
  return lookupInUnicodeMap(code, unicodeESNextIdentifierStart);
}
function isUnicodeIdentifierPart(code) {
  return lookupInUnicodeMap(code, unicodeESNextIdentifierPart);
}
function makeReverseMap(source) {
  const result = [];
  source.forEach((value, name) => {
    result[value] = name;
  });
  return result;
}
var tokenStrings = makeReverseMap(textToToken);
function tokenToString(t) {
  return tokenStrings[t];
}
function stringToToken(s) {
  return textToToken.get(s);
}
function characterCodeToRegularExpressionFlag(ch) {
  return charCodeToRegExpFlag.get(ch);
}
function computeLineStarts(text) {
  const result = [];
  let pos = 0;
  let lineStart = 0;
  while (pos < text.length) {
    const ch = text.charCodeAt(pos);
    pos++;
    switch (ch) {
      case CharacterCodes.carriageReturn:
        if (text.charCodeAt(pos) === CharacterCodes.lineFeed) {
          pos++;
        }
      // falls through
      case CharacterCodes.lineFeed:
        result.push(lineStart);
        lineStart = pos;
        break;
      default:
        if (ch > CharacterCodes.maxAsciiCharacter && isLineBreak(ch)) {
          result.push(lineStart);
          lineStart = pos;
        }
        break;
    }
  }
  result.push(lineStart);
  return result;
}
function isWhiteSpaceLike(ch) {
  return isWhiteSpaceSingleLine(ch) || isLineBreak(ch);
}
function isWhiteSpaceSingleLine(ch) {
  return ch === CharacterCodes.space || ch === CharacterCodes.tab || ch === CharacterCodes.verticalTab || ch === CharacterCodes.formFeed || ch === CharacterCodes.nonBreakingSpace || ch === CharacterCodes.nextLine || ch === CharacterCodes.ogham || ch >= CharacterCodes.enQuad && ch <= CharacterCodes.zeroWidthSpace || ch === CharacterCodes.narrowNoBreakSpace || ch === CharacterCodes.mathematicalSpace || ch === CharacterCodes.ideographicSpace || ch === CharacterCodes.byteOrderMark;
}
function isLineBreak(ch) {
  return ch === CharacterCodes.lineFeed || ch === CharacterCodes.carriageReturn || ch === CharacterCodes.lineSeparator || ch === CharacterCodes.paragraphSeparator;
}
function isDigit(ch) {
  return ch >= CharacterCodes._0 && ch <= CharacterCodes._9;
}
function isHexDigit(ch) {
  return isDigit(ch) || ch >= CharacterCodes.A && ch <= CharacterCodes.F || ch >= CharacterCodes.a && ch <= CharacterCodes.f;
}
function isOctalDigit(ch) {
  return ch >= CharacterCodes._0 && ch <= CharacterCodes._7;
}
function couldStartTrivia(text, pos) {
  const ch = text.charCodeAt(pos);
  switch (ch) {
    case CharacterCodes.carriageReturn:
    case CharacterCodes.lineFeed:
    case CharacterCodes.tab:
    case CharacterCodes.verticalTab:
    case CharacterCodes.formFeed:
    case CharacterCodes.space:
    case CharacterCodes.slash:
    case CharacterCodes.lessThan:
    case CharacterCodes.bar:
    case CharacterCodes.equals:
    case CharacterCodes.greaterThan:
      return true;
    case CharacterCodes.hash:
      return pos === 0;
    default:
      return ch > CharacterCodes.maxAsciiCharacter;
  }
}
function skipTrivia(text, pos, stopAfterLineBreak, stopAtComments, inJSDoc) {
  if (pos < 0) {
    return pos;
  }
  let canConsumeStar = false;
  while (true) {
    const ch = text.charCodeAt(pos);
    switch (ch) {
      case CharacterCodes.carriageReturn:
        if (text.charCodeAt(pos + 1) === CharacterCodes.lineFeed) {
          pos++;
        }
      // falls through
      case CharacterCodes.lineFeed:
        pos++;
        if (stopAfterLineBreak) {
          return pos;
        }
        canConsumeStar = !!inJSDoc;
        continue;
      case CharacterCodes.tab:
      case CharacterCodes.verticalTab:
      case CharacterCodes.formFeed:
      case CharacterCodes.space:
        pos++;
        continue;
      case CharacterCodes.slash:
        if (stopAtComments) {
          break;
        }
        if (text.charCodeAt(pos + 1) === CharacterCodes.slash) {
          pos += 2;
          while (pos < text.length) {
            if (isLineBreak(text.charCodeAt(pos))) {
              break;
            }
            pos++;
          }
          canConsumeStar = false;
          continue;
        }
        if (text.charCodeAt(pos + 1) === CharacterCodes.asterisk) {
          pos += 2;
          while (pos < text.length) {
            if (text.charCodeAt(pos) === CharacterCodes.asterisk && text.charCodeAt(pos + 1) === CharacterCodes.slash) {
              pos += 2;
              break;
            }
            pos++;
          }
          canConsumeStar = false;
          continue;
        }
        break;
      case CharacterCodes.lessThan:
      case CharacterCodes.bar:
      case CharacterCodes.equals:
      case CharacterCodes.greaterThan:
        if (isConflictMarkerTrivia(text, pos)) {
          pos = scanConflictMarkerTrivia(text, pos);
          canConsumeStar = false;
          continue;
        }
        break;
      case CharacterCodes.hash:
        if (pos === 0 && isShebangTrivia(text, pos)) {
          pos = scanShebangTrivia(text, pos);
          continue;
        }
        break;
      case CharacterCodes.asterisk:
        if (canConsumeStar) {
          pos++;
          canConsumeStar = false;
          continue;
        }
        break;
      default:
        if (ch > CharacterCodes.maxAsciiCharacter && isWhiteSpaceLike(ch)) {
          pos++;
          continue;
        }
        break;
    }
    return pos;
  }
}
function isConflictMarkerTrivia(text, pos) {
  if (pos >= text.length) {
    return false;
  }
  const ch = text.charCodeAt(pos);
  if (pos === 0 || isLineBreak(text.charCodeAt(pos - 1))) {
    if (ch === CharacterCodes.lessThan || ch === CharacterCodes.greaterThan || ch === CharacterCodes.equals) {
      if (pos + 6 < text.length && text.charCodeAt(pos + 1) === ch && text.charCodeAt(pos + 2) === ch && text.charCodeAt(pos + 3) === ch && text.charCodeAt(pos + 4) === ch && text.charCodeAt(pos + 5) === ch && text.charCodeAt(pos + 6) === ch) {
        return ch === CharacterCodes.equals || text.charCodeAt(pos + 7) === CharacterCodes.space;
      }
    }
    if (ch === CharacterCodes.bar && pos + 6 < text.length && text.charCodeAt(pos + 1) === ch && text.charCodeAt(pos + 2) === ch && text.charCodeAt(pos + 3) === ch && text.charCodeAt(pos + 4) === ch && text.charCodeAt(pos + 5) === ch && text.charCodeAt(pos + 6) === ch) {
      return true;
    }
  }
  return false;
}
function scanConflictMarkerTrivia(text, pos) {
  const ch = text.charCodeAt(pos);
  const len = text.length;
  if (ch === CharacterCodes.lessThan || ch === CharacterCodes.greaterThan) {
    while (pos < len && !isLineBreak(text.charCodeAt(pos))) {
      pos++;
    }
  } else {
    pos += 7;
    while (pos < len) {
      const currentChar = text.charCodeAt(pos);
      if ((currentChar === CharacterCodes.equals || currentChar === CharacterCodes.greaterThan) && isConflictMarkerTrivia(text, pos)) {
        break;
      }
      pos++;
    }
  }
  return pos;
}
function isShebangTrivia(text, pos) {
  return pos === 0 && text.charCodeAt(0) === CharacterCodes.hash && text.charCodeAt(1) === CharacterCodes.exclamation;
}
function scanShebangTrivia(text, pos) {
  pos += 2;
  while (pos < text.length) {
    if (isLineBreak(text.charCodeAt(pos))) {
      break;
    }
    pos++;
  }
  return pos;
}
function iterateCommentRanges(reduce, text, pos, trailing, cb, state, initial) {
  let pendingPos;
  let pendingEnd;
  let pendingKind;
  let pendingHasTrailingNewLine;
  let hasPendingCommentRange = false;
  let collecting = trailing;
  let accumulator = initial;
  if (pos === 0) {
    collecting = true;
    const shebang = getShebang(text);
    if (shebang) {
      pos = shebang.length;
    }
  }
  scan: while (pos >= 0 && pos < text.length) {
    const ch = text.charCodeAt(pos);
    switch (ch) {
      case CharacterCodes.carriageReturn:
        if (text.charCodeAt(pos + 1) === CharacterCodes.lineFeed) {
          pos++;
        }
      // falls through
      case CharacterCodes.lineFeed:
        pos++;
        if (trailing) {
          break scan;
        }
        collecting = true;
        if (hasPendingCommentRange) {
          pendingHasTrailingNewLine = true;
        }
        continue;
      case CharacterCodes.tab:
      case CharacterCodes.verticalTab:
      case CharacterCodes.formFeed:
      case CharacterCodes.space:
        pos++;
        continue;
      case CharacterCodes.slash:
        const nextChar = text.charCodeAt(pos + 1);
        let hasTrailingNewLine = false;
        if (nextChar === CharacterCodes.slash || nextChar === CharacterCodes.asterisk) {
          const kind = nextChar === CharacterCodes.slash ? SyntaxKind.SingleLineCommentTrivia : SyntaxKind.MultiLineCommentTrivia;
          const startPos = pos;
          pos += 2;
          if (nextChar === CharacterCodes.slash) {
            while (pos < text.length) {
              if (isLineBreak(text.charCodeAt(pos))) {
                hasTrailingNewLine = true;
                break;
              }
              pos++;
            }
          } else {
            while (pos < text.length) {
              if (text.charCodeAt(pos) === CharacterCodes.asterisk && text.charCodeAt(pos + 1) === CharacterCodes.slash) {
                pos += 2;
                break;
              }
              pos++;
            }
          }
          if (collecting) {
            if (hasPendingCommentRange) {
              accumulator = cb(pendingPos, pendingEnd, pendingKind, pendingHasTrailingNewLine, state, accumulator);
              if (!reduce && accumulator) {
                return accumulator;
              }
            }
            pendingPos = startPos;
            pendingEnd = pos;
            pendingKind = kind;
            pendingHasTrailingNewLine = hasTrailingNewLine;
            hasPendingCommentRange = true;
          }
          continue;
        }
        break scan;
      default:
        if (ch > CharacterCodes.maxAsciiCharacter && isWhiteSpaceLike(ch)) {
          if (hasPendingCommentRange && isLineBreak(ch)) {
            pendingHasTrailingNewLine = true;
          }
          pos++;
          continue;
        }
        break scan;
    }
  }
  if (hasPendingCommentRange) {
    accumulator = cb(pendingPos, pendingEnd, pendingKind, pendingHasTrailingNewLine, state, accumulator);
  }
  return accumulator;
}
function appendCommentRange(pos, end, kind, hasTrailingNewLine, _state, comments = []) {
  comments.push({ kind, pos, end, hasTrailingNewLine });
  return comments;
}
function forEachLeadingCommentRange(text, pos, cb, state) {
  return iterateCommentRanges(
    /*reduce*/
    false,
    text,
    pos,
    /*trailing*/
    false,
    cb,
    state,
    /*initial*/
    void 0
  );
}
function forEachTrailingCommentRange(text, pos, cb, state) {
  return iterateCommentRanges(
    /*reduce*/
    false,
    text,
    pos,
    /*trailing*/
    true,
    cb,
    state,
    /*initial*/
    void 0
  );
}
function reduceEachLeadingCommentRange(text, pos, cb, state, initial) {
  return iterateCommentRanges(
    /*reduce*/
    true,
    text,
    pos,
    /*trailing*/
    false,
    cb,
    state,
    initial
  );
}
function reduceEachTrailingCommentRange(text, pos, cb, state, initial) {
  return iterateCommentRanges(
    /*reduce*/
    true,
    text,
    pos,
    /*trailing*/
    true,
    cb,
    state,
    initial
  );
}
function getLeadingCommentRanges(text, pos) {
  return reduceEachLeadingCommentRange(
    text,
    pos,
    appendCommentRange,
    /*state*/
    void 0,
    /*initial*/
    void 0
  );
}
function getTrailingCommentRanges(text, pos) {
  return reduceEachTrailingCommentRange(
    text,
    pos,
    appendCommentRange,
    /*state*/
    void 0,
    /*initial*/
    void 0
  );
}
function getShebang(text) {
  const shebang = /^#!.*/;
  if (isShebangTrivia(text, 0)) {
    const match = shebang.exec(text);
    if (match) {
      return match[0];
    }
  }
  return void 0;
}
function isIdentifierStart(ch, _languageVersion) {
  return ch >= CharacterCodes.A && ch <= CharacterCodes.Z || ch >= CharacterCodes.a && ch <= CharacterCodes.z || ch === CharacterCodes.$ || ch === CharacterCodes._ || ch > CharacterCodes.maxAsciiCharacter && isUnicodeIdentifierStart(ch);
}
function isIdentifierPart(ch, _languageVersion, identifierVariant) {
  return ch >= CharacterCodes.A && ch <= CharacterCodes.Z || ch >= CharacterCodes.a && ch <= CharacterCodes.z || ch >= CharacterCodes._0 && ch <= CharacterCodes._9 || ch === CharacterCodes.$ || ch === CharacterCodes._ || // "-" and ":" are valid in JSX Identifiers
  (identifierVariant === LanguageVariant.JSX ? ch === CharacterCodes.minus || ch === CharacterCodes.colon : false) || ch > CharacterCodes.maxAsciiCharacter && isUnicodeIdentifierPart(ch);
}
function isIdentifierText(name, _languageVersion, identifierVariant) {
  let ch = name.codePointAt(0);
  if (!isIdentifierStart(ch)) {
    return false;
  }
  for (let i = charSize(ch); i < name.length; i += charSize(ch)) {
    if (!isIdentifierPart(ch = name.codePointAt(i), void 0, identifierVariant)) {
      return false;
    }
  }
  return true;
}
function codePointAt(s, i) {
  return s.codePointAt(i);
}
function charSize(ch) {
  if (ch >= 65536) {
    return 2;
  }
  if (ch === CharacterCodes.EOF) {
    return 0;
  }
  return 1;
}
function utf16EncodeAsString(codePoint) {
  return String.fromCodePoint(codePoint);
}
function parsePseudoBigInt(stringValue) {
  let log2Base;
  switch (stringValue.charCodeAt(1)) {
    case CharacterCodes.b:
    case CharacterCodes.B:
      log2Base = 1;
      break;
    case CharacterCodes.o:
    case CharacterCodes.O:
      log2Base = 3;
      break;
    case CharacterCodes.x:
    case CharacterCodes.X:
      log2Base = 4;
      break;
    default:
      const nIndex = stringValue.length - 1;
      let nonZeroStart = 0;
      while (stringValue.charCodeAt(nonZeroStart) === CharacterCodes._0) {
        nonZeroStart++;
      }
      return stringValue.slice(nonZeroStart, nIndex) || "0";
  }
  const startIndex = 2;
  const endIndex = stringValue.length - 1;
  const bitsNeeded = (endIndex - startIndex) * log2Base;
  const segments = new Uint16Array((bitsNeeded >>> 4) + (bitsNeeded & 15 ? 1 : 0));
  for (let i = endIndex - 1, bitOffset = 0; i >= startIndex; i--, bitOffset += log2Base) {
    const segment = bitOffset >>> 4;
    const digitChar = stringValue.charCodeAt(i);
    const digit = digitChar <= CharacterCodes._9 ? digitChar - CharacterCodes._0 : 10 + digitChar - (digitChar <= CharacterCodes.F ? CharacterCodes.A : CharacterCodes.a);
    const shiftedDigit = digit << (bitOffset & 15);
    segments[segment] |= shiftedDigit;
    const residual = shiftedDigit >>> 16;
    if (residual)
      segments[segment + 1] |= residual;
  }
  let base10Value = "";
  let firstNonzeroSegment = segments.length - 1;
  let segmentsRemaining = true;
  while (segmentsRemaining) {
    let mod10 = 0;
    segmentsRemaining = false;
    for (let segment = firstNonzeroSegment; segment >= 0; segment--) {
      const newSegment = mod10 << 16 | segments[segment];
      const segmentValue = newSegment / 10 | 0;
      segments[segment] = segmentValue;
      mod10 = newSegment - segmentValue * 10;
      if (segmentValue && !segmentsRemaining) {
        firstNonzeroSegment = segment;
        segmentsRemaining = true;
      }
    }
    base10Value = mod10 + base10Value;
  }
  return base10Value;
}
function createScanner(skipTrivia2, languageVariant = LanguageVariant.Standard, textInitial, start, length) {
  var text = textInitial;
  var pos;
  var end;
  var fullStartPos;
  var tokenStart;
  var token;
  var tokenValue;
  var tokenFlags;
  var commentDirectives;
  var skipJsDocLeadingAsterisks = 0;
  setText(text, start, length);
  var scanner = {
    getTokenFullStart: () => fullStartPos,
    getTokenEnd: () => pos,
    getToken: () => token,
    getTokenStart: () => tokenStart,
    getTokenText: () => text.substring(tokenStart, pos),
    getTokenValue: () => tokenValue,
    hasUnicodeEscape: () => (tokenFlags & TokenFlags.UnicodeEscape) !== 0,
    hasExtendedUnicodeEscape: () => (tokenFlags & TokenFlags.ExtendedUnicodeEscape) !== 0,
    hasPrecedingLineBreak: () => (tokenFlags & TokenFlags.PrecedingLineBreak) !== 0,
    hasPrecedingJSDocComment: () => (tokenFlags & TokenFlags.PrecedingJSDocComment) !== 0,
    hasPrecedingJSDocLeadingAsterisks: () => (tokenFlags & TokenFlags.PrecedingJSDocLeadingAsterisks) !== 0,
    hasPrecedingJSDocWithDeprecatedTag: () => (tokenFlags & TokenFlags.PrecedingJSDocWithDeprecated) !== 0,
    hasPrecedingJSDocWithSeeOrLink: () => (tokenFlags & TokenFlags.PrecedingJSDocWithSeeOrLink) !== 0,
    isIdentifier: () => token === SyntaxKind.Identifier || token > SyntaxKind.LastReservedWord,
    isReservedWord: () => token >= SyntaxKind.FirstReservedWord && token <= SyntaxKind.LastReservedWord,
    isUnterminated: () => (tokenFlags & TokenFlags.Unterminated) !== 0,
    getCommentDirectives: () => commentDirectives,
    getNumericLiteralFlags: () => tokenFlags & TokenFlags.NumericLiteralFlags,
    getTokenFlags: () => tokenFlags,
    reScanGreaterToken,
    reScanAsteriskEqualsToken,
    reScanSlashToken,
    reScanTemplateToken,
    reScanTemplateHeadOrNoSubstitutionTemplate,
    scanJsxIdentifier,
    scanJsxAttributeValue,
    reScanJsxAttributeValue,
    reScanJsxToken,
    reScanLessThanToken,
    reScanHashToken,
    reScanQuestionToken,
    reScanInvalidIdentifier,
    scanJsxToken,
    scanJsDocToken,
    scanJSDocCommentTextToken,
    scan,
    getText,
    clearCommentDirectives,
    setText,
    setLanguageVariant,
    resetTokenState,
    setSkipJsDocLeadingAsterisks,
    tryScan,
    lookAhead,
    scanRange
  };
  return scanner;
  function codePointUnchecked(pos2) {
    return codePointAt(text, pos2);
  }
  function codePointChecked(pos2) {
    return pos2 >= 0 && pos2 < end ? codePointUnchecked(pos2) : CharacterCodes.EOF;
  }
  function charCodeUnchecked(pos2) {
    return text.charCodeAt(pos2);
  }
  function charCodeChecked(pos2) {
    return pos2 >= 0 && pos2 < end ? charCodeUnchecked(pos2) : CharacterCodes.EOF;
  }
  function scanNumberFragment() {
    let start2 = pos;
    let allowSeparator = false;
    let isPreviousTokenSeparator = false;
    let result = "";
    while (true) {
      const ch = charCodeUnchecked(pos);
      if (ch === CharacterCodes._) {
        tokenFlags |= TokenFlags.ContainsSeparator;
        if (allowSeparator) {
          allowSeparator = false;
          isPreviousTokenSeparator = true;
          result += text.substring(start2, pos);
        } else {
          tokenFlags |= TokenFlags.ContainsInvalidSeparator;
        }
        pos++;
        start2 = pos;
        continue;
      }
      if (isDigit(ch)) {
        allowSeparator = true;
        isPreviousTokenSeparator = false;
        pos++;
        continue;
      }
      break;
    }
    if (charCodeUnchecked(pos - 1) === CharacterCodes._) {
      tokenFlags |= TokenFlags.ContainsInvalidSeparator;
    }
    return result + text.substring(start2, pos);
  }
  function scanNumber() {
    let start2 = pos;
    let mainFragment;
    if (charCodeUnchecked(pos) === CharacterCodes._0) {
      pos++;
      if (charCodeUnchecked(pos) === CharacterCodes._) {
        tokenFlags |= TokenFlags.ContainsSeparator | TokenFlags.ContainsInvalidSeparator;
        pos--;
        mainFragment = scanNumberFragment();
      } else if (!scanDigits()) {
        tokenFlags |= TokenFlags.ContainsLeadingZero;
        mainFragment = "" + +tokenValue;
      } else if (!tokenValue) {
        mainFragment = "0";
      } else {
        tokenValue = "" + parseInt(tokenValue, 8);
        tokenFlags |= TokenFlags.Octal;
        const withMinus = token === SyntaxKind.MinusToken;
        const literal = (withMinus ? "-" : "") + "0o" + (+tokenValue).toString(8);
        if (withMinus)
          start2--;
        return SyntaxKind.NumericLiteral;
      }
    } else {
      mainFragment = scanNumberFragment();
    }
    let decimalFragment;
    let scientificFragment;
    if (charCodeUnchecked(pos) === CharacterCodes.dot) {
      pos++;
      decimalFragment = scanNumberFragment();
    }
    let end2 = pos;
    if (charCodeUnchecked(pos) === CharacterCodes.E || charCodeUnchecked(pos) === CharacterCodes.e) {
      pos++;
      tokenFlags |= TokenFlags.Scientific;
      if (charCodeUnchecked(pos) === CharacterCodes.plus || charCodeUnchecked(pos) === CharacterCodes.minus)
        pos++;
      const preNumericPart = pos;
      const finalFragment = scanNumberFragment();
      if (finalFragment) {
        scientificFragment = text.substring(end2, preNumericPart) + finalFragment;
        end2 = pos;
      }
    }
    let result;
    if (tokenFlags & TokenFlags.ContainsSeparator) {
      result = mainFragment;
      if (decimalFragment) {
        result += "." + decimalFragment;
      }
      if (scientificFragment) {
        result += scientificFragment;
      }
    } else {
      result = text.substring(start2, end2);
    }
    if (tokenFlags & TokenFlags.ContainsLeadingZero) {
      tokenValue = "" + +result;
      return SyntaxKind.NumericLiteral;
    }
    if (decimalFragment !== void 0 || tokenFlags & TokenFlags.Scientific) {
      checkForIdentifierStartAfterNumericLiteral();
      tokenValue = "" + +result;
      return SyntaxKind.NumericLiteral;
    } else {
      tokenValue = result;
      const type = checkBigIntSuffix();
      checkForIdentifierStartAfterNumericLiteral();
      return type;
    }
  }
  function checkForIdentifierStartAfterNumericLiteral() {
    if (!isIdentifierStart(codePointUnchecked(pos))) {
      return;
    }
    const identifierStart = pos;
    const { length: length2 } = scanIdentifierParts();
    if (!(length2 === 1 && text[identifierStart] === "n")) {
      pos = identifierStart;
    }
  }
  function scanDigits() {
    const start2 = pos;
    let isOctal = true;
    while (isDigit(charCodeChecked(pos))) {
      if (!isOctalDigit(charCodeUnchecked(pos))) {
        isOctal = false;
      }
      pos++;
    }
    tokenValue = text.substring(start2, pos);
    return isOctal;
  }
  function scanExactNumberOfHexDigits(count, canHaveSeparators) {
    const valueString = scanHexDigits(
      count,
      /*scanAsManyAsPossible*/
      false,
      canHaveSeparators
    );
    return valueString ? parseInt(valueString, 16) : -1;
  }
  function scanMinimumNumberOfHexDigits(count, canHaveSeparators) {
    return scanHexDigits(
      count,
      /*scanAsManyAsPossible*/
      true,
      canHaveSeparators
    );
  }
  function scanHexDigits(minCount, scanAsManyAsPossible, canHaveSeparators) {
    let valueChars = [];
    let allowSeparator = false;
    while (valueChars.length < minCount || scanAsManyAsPossible) {
      let ch = charCodeUnchecked(pos);
      if (canHaveSeparators && ch === CharacterCodes._) {
        tokenFlags |= TokenFlags.ContainsSeparator;
        if (allowSeparator) {
          allowSeparator = false;
        }
        pos++;
        continue;
      }
      allowSeparator = canHaveSeparators;
      if (ch >= CharacterCodes.A && ch <= CharacterCodes.F) {
        ch += CharacterCodes.a - CharacterCodes.A;
      } else if (!(ch >= CharacterCodes._0 && ch <= CharacterCodes._9 || ch >= CharacterCodes.a && ch <= CharacterCodes.f)) {
        break;
      }
      valueChars.push(ch);
      pos++;
    }
    if (valueChars.length < minCount) {
      valueChars = [];
    }
    return String.fromCharCode(...valueChars);
  }
  function scanString(jsxAttributeString = false) {
    const quote = charCodeUnchecked(pos);
    pos++;
    let result = "";
    let start2 = pos;
    while (true) {
      if (pos >= end) {
        result += text.substring(start2, pos);
        tokenFlags |= TokenFlags.Unterminated;
        break;
      }
      const ch = charCodeUnchecked(pos);
      if (ch === quote) {
        result += text.substring(start2, pos);
        pos++;
        break;
      }
      if (ch === CharacterCodes.backslash && !jsxAttributeString) {
        result += text.substring(start2, pos);
        result += scanEscapeSequence(EscapeSequenceScanningFlags.String | EscapeSequenceScanningFlags.ReportErrors);
        start2 = pos;
        continue;
      }
      if ((ch === CharacterCodes.lineFeed || ch === CharacterCodes.carriageReturn) && !jsxAttributeString) {
        result += text.substring(start2, pos);
        tokenFlags |= TokenFlags.Unterminated;
        break;
      }
      pos++;
    }
    return result;
  }
  function scanTemplateAndSetTokenValue(shouldEmitInvalidEscapeError) {
    const startedWithBacktick = charCodeUnchecked(pos) === CharacterCodes.backtick;
    pos++;
    let start2 = pos;
    let contents = "";
    let resultingToken;
    while (true) {
      if (pos >= end) {
        contents += text.substring(start2, pos);
        tokenFlags |= TokenFlags.Unterminated;
        resultingToken = startedWithBacktick ? SyntaxKind.NoSubstitutionTemplateLiteral : SyntaxKind.TemplateTail;
        break;
      }
      const currChar = charCodeUnchecked(pos);
      if (currChar === CharacterCodes.backtick) {
        contents += text.substring(start2, pos);
        pos++;
        resultingToken = startedWithBacktick ? SyntaxKind.NoSubstitutionTemplateLiteral : SyntaxKind.TemplateTail;
        break;
      }
      if (currChar === CharacterCodes.$ && pos + 1 < end && charCodeUnchecked(pos + 1) === CharacterCodes.openBrace) {
        contents += text.substring(start2, pos);
        pos += 2;
        resultingToken = startedWithBacktick ? SyntaxKind.TemplateHead : SyntaxKind.TemplateMiddle;
        break;
      }
      if (currChar === CharacterCodes.backslash) {
        contents += text.substring(start2, pos);
        contents += scanEscapeSequence(EscapeSequenceScanningFlags.String | (shouldEmitInvalidEscapeError ? EscapeSequenceScanningFlags.ReportErrors : 0));
        start2 = pos;
        continue;
      }
      if (currChar === CharacterCodes.carriageReturn) {
        contents += text.substring(start2, pos);
        pos++;
        if (pos < end && charCodeUnchecked(pos) === CharacterCodes.lineFeed) {
          pos++;
        }
        contents += "\n";
        start2 = pos;
        continue;
      }
      pos++;
    }
    tokenValue = contents;
    return resultingToken;
  }
  function scanEscapeSequence(flags) {
    const start2 = pos;
    pos++;
    if (pos >= end) {
      return "";
    }
    const ch = charCodeUnchecked(pos);
    pos++;
    switch (ch) {
      case CharacterCodes._0:
        if (pos >= end || !isDigit(charCodeUnchecked(pos))) {
          return "\0";
        }
      // falls through
      case CharacterCodes._1:
      case CharacterCodes._2:
      case CharacterCodes._3:
        if (pos < end && isOctalDigit(charCodeUnchecked(pos))) {
          pos++;
        }
      // falls through
      case CharacterCodes._4:
      case CharacterCodes._5:
      case CharacterCodes._6:
      case CharacterCodes._7:
        if (pos < end && isOctalDigit(charCodeUnchecked(pos))) {
          pos++;
        }
        tokenFlags |= TokenFlags.ContainsInvalidEscape;
        if (flags & EscapeSequenceScanningFlags.ReportInvalidEscapeErrors) {
          const code = parseInt(text.substring(start2 + 1, pos), 8);
          return String.fromCharCode(code);
        }
        return text.substring(start2, pos);
      case CharacterCodes._8:
      case CharacterCodes._9:
        tokenFlags |= TokenFlags.ContainsInvalidEscape;
        if (flags & EscapeSequenceScanningFlags.ReportInvalidEscapeErrors) {
          return String.fromCharCode(ch);
        }
        return text.substring(start2, pos);
      case CharacterCodes.b:
        return "\b";
      case CharacterCodes.t:
        return "	";
      case CharacterCodes.n:
        return "\n";
      case CharacterCodes.v:
        return "\v";
      case CharacterCodes.f:
        return "\f";
      case CharacterCodes.r:
        return "\r";
      case CharacterCodes.singleQuote:
        return "'";
      case CharacterCodes.doubleQuote:
        return '"';
      case CharacterCodes.u:
        if (pos < end && charCodeUnchecked(pos) === CharacterCodes.openBrace) {
          pos -= 2;
          const result = scanExtendedUnicodeEscape();
          if (!(flags & EscapeSequenceScanningFlags.AllowExtendedUnicodeEscape)) {
            tokenFlags |= TokenFlags.ContainsInvalidEscape;
          }
          return result;
        }
        for (; pos < start2 + 6; pos++) {
          if (!(pos < end && isHexDigit(charCodeUnchecked(pos)))) {
            tokenFlags |= TokenFlags.ContainsInvalidEscape;
            return text.substring(start2, pos);
          }
        }
        tokenFlags |= TokenFlags.UnicodeEscape;
        {
          const escapedValue = parseInt(text.substring(start2 + 2, pos), 16);
          const escapedValueString = String.fromCharCode(escapedValue);
          if (flags & EscapeSequenceScanningFlags.AnyUnicodeMode && escapedValue >= 55296 && escapedValue <= 56319 && pos + 6 < end && text.substring(pos, pos + 2) === "\\u" && charCodeUnchecked(pos + 2) !== CharacterCodes.openBrace) {
            const nextStart = pos;
            let nextPos = pos + 2;
            for (; nextPos < nextStart + 6; nextPos++) {
              if (!isHexDigit(charCodeUnchecked(nextPos))) {
                return escapedValueString;
              }
            }
            const nextEscapedValue = parseInt(text.substring(nextStart + 2, nextPos), 16);
            if (nextEscapedValue >= 56320 && nextEscapedValue <= 57343) {
              pos = nextPos;
              return escapedValueString + String.fromCharCode(nextEscapedValue);
            }
          }
          return escapedValueString;
        }
      case CharacterCodes.x:
        for (; pos < start2 + 4; pos++) {
          if (!(pos < end && isHexDigit(charCodeUnchecked(pos)))) {
            tokenFlags |= TokenFlags.ContainsInvalidEscape;
            return text.substring(start2, pos);
          }
        }
        tokenFlags |= TokenFlags.HexEscape;
        return String.fromCharCode(parseInt(text.substring(start2 + 2, pos), 16));
      case CharacterCodes.carriageReturn:
        if (pos < end && charCodeUnchecked(pos) === CharacterCodes.lineFeed) {
          pos++;
        }
      // falls through
      case CharacterCodes.lineFeed:
      case CharacterCodes.lineSeparator:
      case CharacterCodes.paragraphSeparator:
        return "";
      default:
        return String.fromCharCode(ch);
    }
  }
  function scanExtendedUnicodeEscape() {
    const start2 = pos;
    pos += 3;
    const escapedValueString = scanMinimumNumberOfHexDigits(
      1,
      /*canHaveSeparators*/
      false
    );
    const escapedValue = escapedValueString ? parseInt(escapedValueString, 16) : -1;
    let isInvalidExtendedEscape = false;
    if (escapedValue < 0) {
      isInvalidExtendedEscape = true;
    } else if (escapedValue > 1114111) {
      isInvalidExtendedEscape = true;
    }
    if (pos >= end) {
      isInvalidExtendedEscape = true;
    } else if (charCodeUnchecked(pos) === CharacterCodes.closeBrace) {
      pos++;
    } else {
      isInvalidExtendedEscape = true;
    }
    if (isInvalidExtendedEscape) {
      tokenFlags |= TokenFlags.ContainsInvalidEscape;
      return text.substring(start2, pos);
    }
    tokenFlags |= TokenFlags.ExtendedUnicodeEscape;
    return utf16EncodeAsString(escapedValue);
  }
  function peekUnicodeEscape() {
    if (pos + 5 < end && charCodeUnchecked(pos + 1) === CharacterCodes.u) {
      const start2 = pos;
      pos += 2;
      const value = scanExactNumberOfHexDigits(
        4,
        /*canHaveSeparators*/
        false
      );
      pos = start2;
      return value;
    }
    return -1;
  }
  function peekExtendedUnicodeEscape() {
    if (codePointUnchecked(pos + 1) === CharacterCodes.u && codePointUnchecked(pos + 2) === CharacterCodes.openBrace) {
      const start2 = pos;
      pos += 3;
      const escapedValueString = scanMinimumNumberOfHexDigits(
        1,
        /*canHaveSeparators*/
        false
      );
      const escapedValue = escapedValueString ? parseInt(escapedValueString, 16) : -1;
      pos = start2;
      return escapedValue;
    }
    return -1;
  }
  function scanIdentifierParts() {
    let result = "";
    let start2 = pos;
    while (pos < end) {
      let ch = codePointUnchecked(pos);
      if (isIdentifierPart(ch)) {
        pos += charSize(ch);
      } else if (ch === CharacterCodes.backslash) {
        ch = peekExtendedUnicodeEscape();
        if (ch >= 0 && isIdentifierPart(ch)) {
          result += scanExtendedUnicodeEscape();
          start2 = pos;
          continue;
        }
        ch = peekUnicodeEscape();
        if (!(ch >= 0 && isIdentifierPart(ch))) {
          break;
        }
        tokenFlags |= TokenFlags.UnicodeEscape;
        result += text.substring(start2, pos);
        result += utf16EncodeAsString(ch);
        pos += 6;
        start2 = pos;
      } else {
        break;
      }
    }
    result += text.substring(start2, pos);
    return result;
  }
  function getIdentifierToken() {
    const len = tokenValue.length;
    if (len >= 2 && len <= 12) {
      const ch = tokenValue.charCodeAt(0);
      if (ch >= CharacterCodes.a && ch <= CharacterCodes.z) {
        const keyword = textToKeyword.get(tokenValue);
        if (keyword !== void 0) {
          return token = keyword;
        }
      }
    }
    return token = SyntaxKind.Identifier;
  }
  function scanBinaryOrOctalDigits(base) {
    let value = "";
    let separatorAllowed = false;
    let isPreviousTokenSeparator = false;
    while (true) {
      const ch = charCodeUnchecked(pos);
      if (ch === CharacterCodes._) {
        tokenFlags |= TokenFlags.ContainsSeparator;
        if (separatorAllowed) {
          separatorAllowed = false;
          isPreviousTokenSeparator = true;
        }
        pos++;
        continue;
      }
      separatorAllowed = true;
      if (!isDigit(ch) || ch - CharacterCodes._0 >= base) {
        break;
      }
      value += text[pos];
      pos++;
      isPreviousTokenSeparator = false;
    }
    return value;
  }
  function checkBigIntSuffix() {
    if (charCodeUnchecked(pos) === CharacterCodes.n) {
      tokenValue += "n";
      if (tokenFlags & TokenFlags.BinaryOrOctalSpecifier) {
        tokenValue = parsePseudoBigInt(tokenValue) + "n";
      }
      pos++;
      return SyntaxKind.BigIntLiteral;
    } else {
      const numericValue = tokenFlags & TokenFlags.BinarySpecifier ? parseInt(tokenValue.slice(2), 2) : tokenFlags & TokenFlags.OctalSpecifier ? parseInt(tokenValue.slice(2), 8) : +tokenValue;
      tokenValue = "" + numericValue;
      return SyntaxKind.NumericLiteral;
    }
  }
  function scan() {
    fullStartPos = pos;
    tokenFlags = TokenFlags.None;
    while (true) {
      tokenStart = pos;
      if (pos >= end) {
        return token = SyntaxKind.EndOfFile;
      }
      const ch = codePointUnchecked(pos);
      if (pos === 0) {
        if (ch === CharacterCodes.hash && isShebangTrivia(text, pos)) {
          pos = scanShebangTrivia(text, pos);
          if (skipTrivia2) {
            continue;
          } else {
            return token = SyntaxKind.Unknown;
          }
        }
      }
      switch (ch) {
        case CharacterCodes.lineFeed:
        case CharacterCodes.carriageReturn:
          tokenFlags |= TokenFlags.PrecedingLineBreak;
          if (skipTrivia2) {
            pos++;
            continue;
          } else {
            if (ch === CharacterCodes.carriageReturn && pos + 1 < end && charCodeUnchecked(pos + 1) === CharacterCodes.lineFeed) {
              pos += 2;
            } else {
              pos++;
            }
            return token = SyntaxKind.NewLineTrivia;
          }
        case CharacterCodes.tab:
        case CharacterCodes.verticalTab:
        case CharacterCodes.formFeed:
        case CharacterCodes.space:
        case CharacterCodes.nonBreakingSpace:
        case CharacterCodes.ogham:
        case CharacterCodes.enQuad:
        case CharacterCodes.emQuad:
        case CharacterCodes.enSpace:
        case CharacterCodes.emSpace:
        case CharacterCodes.threePerEmSpace:
        case CharacterCodes.fourPerEmSpace:
        case CharacterCodes.sixPerEmSpace:
        case CharacterCodes.figureSpace:
        case CharacterCodes.punctuationSpace:
        case CharacterCodes.thinSpace:
        case CharacterCodes.hairSpace:
        case CharacterCodes.zeroWidthSpace:
        case CharacterCodes.narrowNoBreakSpace:
        case CharacterCodes.mathematicalSpace:
        case CharacterCodes.ideographicSpace:
        case CharacterCodes.byteOrderMark:
          if (skipTrivia2) {
            pos++;
            continue;
          } else {
            while (pos < end && isWhiteSpaceSingleLine(charCodeUnchecked(pos))) {
              pos++;
            }
            return token = SyntaxKind.WhitespaceTrivia;
          }
        case CharacterCodes.exclamation:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.ExclamationEqualsEqualsToken;
            }
            return pos += 2, token = SyntaxKind.ExclamationEqualsToken;
          }
          pos++;
          return token = SyntaxKind.ExclamationToken;
        case CharacterCodes.doubleQuote:
        case CharacterCodes.singleQuote:
          tokenValue = scanString();
          return token = SyntaxKind.StringLiteral;
        case CharacterCodes.backtick:
          return token = scanTemplateAndSetTokenValue(
            /*shouldEmitInvalidEscapeError*/
            false
          );
        case CharacterCodes.percent:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.PercentEqualsToken;
          }
          pos++;
          return token = SyntaxKind.PercentToken;
        case CharacterCodes.ampersand:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.ampersand) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.AmpersandAmpersandEqualsToken;
            }
            return pos += 2, token = SyntaxKind.AmpersandAmpersandToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.AmpersandEqualsToken;
          }
          pos++;
          return token = SyntaxKind.AmpersandToken;
        case CharacterCodes.openParen:
          pos++;
          return token = SyntaxKind.OpenParenToken;
        case CharacterCodes.closeParen:
          pos++;
          return token = SyntaxKind.CloseParenToken;
        case CharacterCodes.asterisk:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.AsteriskEqualsToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.asterisk) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.AsteriskAsteriskEqualsToken;
            }
            return pos += 2, token = SyntaxKind.AsteriskAsteriskToken;
          }
          pos++;
          if (skipJsDocLeadingAsterisks && (tokenFlags & TokenFlags.PrecedingJSDocLeadingAsterisks) === 0 && tokenFlags & TokenFlags.PrecedingLineBreak) {
            tokenFlags |= TokenFlags.PrecedingJSDocLeadingAsterisks;
            continue;
          }
          return token = SyntaxKind.AsteriskToken;
        case CharacterCodes.plus:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.plus) {
            return pos += 2, token = SyntaxKind.PlusPlusToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.PlusEqualsToken;
          }
          pos++;
          return token = SyntaxKind.PlusToken;
        case CharacterCodes.comma:
          pos++;
          return token = SyntaxKind.CommaToken;
        case CharacterCodes.minus:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.minus) {
            return pos += 2, token = SyntaxKind.MinusMinusToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.MinusEqualsToken;
          }
          pos++;
          return token = SyntaxKind.MinusToken;
        case CharacterCodes.dot:
          if (isDigit(charCodeUnchecked(pos + 1))) {
            scanNumber();
            return token = SyntaxKind.NumericLiteral;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.dot && charCodeUnchecked(pos + 2) === CharacterCodes.dot) {
            return pos += 3, token = SyntaxKind.DotDotDotToken;
          }
          pos++;
          return token = SyntaxKind.DotToken;
        case CharacterCodes.slash:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.slash) {
            pos += 2;
            while (pos < end) {
              if (isLineBreak(charCodeUnchecked(pos))) {
                break;
              }
              pos++;
            }
            commentDirectives = appendIfCommentDirective(commentDirectives, text.slice(tokenStart, pos), commentDirectiveRegExSingleLine, tokenStart);
            if (skipTrivia2) {
              continue;
            } else {
              return token = SyntaxKind.SingleLineCommentTrivia;
            }
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.asterisk) {
            pos += 2;
            const isJSDoc2 = charCodeUnchecked(pos) === CharacterCodes.asterisk && charCodeUnchecked(pos + 1) !== CharacterCodes.slash;
            let commentClosed = false;
            let lastLineStart = tokenStart;
            while (pos < end) {
              const ch2 = charCodeUnchecked(pos);
              if (ch2 === CharacterCodes.asterisk && charCodeUnchecked(pos + 1) === CharacterCodes.slash) {
                pos += 2;
                commentClosed = true;
                break;
              }
              pos++;
              if (isLineBreak(ch2)) {
                lastLineStart = pos;
                tokenFlags |= TokenFlags.PrecedingLineBreak;
              }
            }
            if (isJSDoc2) {
              tokenFlags |= TokenFlags.PrecedingJSDocComment;
              tokenFlags = scanJSDocCommentForTags(text.slice(tokenStart, pos), tokenFlags);
            }
            commentDirectives = appendIfCommentDirective(commentDirectives, text.slice(lastLineStart, pos), commentDirectiveRegExMultiLine, lastLineStart);
            if (skipTrivia2) {
              continue;
            } else {
              if (!commentClosed) {
                tokenFlags |= TokenFlags.Unterminated;
              }
              return token = SyntaxKind.MultiLineCommentTrivia;
            }
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.SlashEqualsToken;
          }
          pos++;
          return token = SyntaxKind.SlashToken;
        case CharacterCodes._0:
          if (pos + 2 < end && (charCodeUnchecked(pos + 1) === CharacterCodes.X || charCodeUnchecked(pos + 1) === CharacterCodes.x)) {
            pos += 2;
            tokenValue = scanMinimumNumberOfHexDigits(
              1,
              /*canHaveSeparators*/
              true
            );
            if (!tokenValue) {
              tokenValue = "0";
            }
            tokenValue = "0x" + tokenValue;
            tokenFlags |= TokenFlags.HexSpecifier;
            return token = checkBigIntSuffix();
          } else if (pos + 2 < end && (charCodeUnchecked(pos + 1) === CharacterCodes.B || charCodeUnchecked(pos + 1) === CharacterCodes.b)) {
            pos += 2;
            tokenValue = scanBinaryOrOctalDigits(2);
            if (!tokenValue) {
              tokenValue = "0";
            }
            tokenValue = "0b" + tokenValue;
            tokenFlags |= TokenFlags.BinarySpecifier;
            return token = checkBigIntSuffix();
          } else if (pos + 2 < end && (charCodeUnchecked(pos + 1) === CharacterCodes.O || charCodeUnchecked(pos + 1) === CharacterCodes.o)) {
            pos += 2;
            tokenValue = scanBinaryOrOctalDigits(8);
            if (!tokenValue) {
              tokenValue = "0";
            }
            tokenValue = "0o" + tokenValue;
            tokenFlags |= TokenFlags.OctalSpecifier;
            return token = checkBigIntSuffix();
          }
        // falls through
        case CharacterCodes._1:
        case CharacterCodes._2:
        case CharacterCodes._3:
        case CharacterCodes._4:
        case CharacterCodes._5:
        case CharacterCodes._6:
        case CharacterCodes._7:
        case CharacterCodes._8:
        case CharacterCodes._9:
          return token = scanNumber();
        case CharacterCodes.colon:
          pos++;
          return token = SyntaxKind.ColonToken;
        case CharacterCodes.semicolon:
          pos++;
          return token = SyntaxKind.SemicolonToken;
        case CharacterCodes.lessThan:
          if (isConflictMarkerTrivia(text, pos)) {
            pos = scanConflictMarkerTrivia(text, pos);
            if (skipTrivia2) {
              continue;
            } else {
              return token = SyntaxKind.ConflictMarkerTrivia;
            }
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.lessThan) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.LessThanLessThanEqualsToken;
            }
            return pos += 2, token = SyntaxKind.LessThanLessThanToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.LessThanEqualsToken;
          }
          if (languageVariant === LanguageVariant.JSX && charCodeUnchecked(pos + 1) === CharacterCodes.slash && charCodeUnchecked(pos + 2) !== CharacterCodes.asterisk) {
            return pos += 2, token = SyntaxKind.LessThanSlashToken;
          }
          pos++;
          return token = SyntaxKind.LessThanToken;
        case CharacterCodes.equals:
          if (isConflictMarkerTrivia(text, pos)) {
            pos = scanConflictMarkerTrivia(text, pos);
            if (skipTrivia2) {
              continue;
            } else {
              return token = SyntaxKind.ConflictMarkerTrivia;
            }
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.EqualsEqualsEqualsToken;
            }
            return pos += 2, token = SyntaxKind.EqualsEqualsToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.greaterThan) {
            return pos += 2, token = SyntaxKind.EqualsGreaterThanToken;
          }
          pos++;
          return token = SyntaxKind.EqualsToken;
        case CharacterCodes.greaterThan:
          if (isConflictMarkerTrivia(text, pos)) {
            pos = scanConflictMarkerTrivia(text, pos);
            if (skipTrivia2) {
              continue;
            } else {
              return token = SyntaxKind.ConflictMarkerTrivia;
            }
          }
          pos++;
          return token = SyntaxKind.GreaterThanToken;
        case CharacterCodes.question:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.dot && !isDigit(charCodeUnchecked(pos + 2))) {
            return pos += 2, token = SyntaxKind.QuestionDotToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.question) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.QuestionQuestionEqualsToken;
            }
            return pos += 2, token = SyntaxKind.QuestionQuestionToken;
          }
          pos++;
          return token = SyntaxKind.QuestionToken;
        case CharacterCodes.openBracket:
          pos++;
          return token = SyntaxKind.OpenBracketToken;
        case CharacterCodes.closeBracket:
          pos++;
          return token = SyntaxKind.CloseBracketToken;
        case CharacterCodes.caret:
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.CaretEqualsToken;
          }
          pos++;
          return token = SyntaxKind.CaretToken;
        case CharacterCodes.openBrace:
          pos++;
          return token = SyntaxKind.OpenBraceToken;
        case CharacterCodes.bar:
          if (isConflictMarkerTrivia(text, pos)) {
            pos = scanConflictMarkerTrivia(text, pos);
            if (skipTrivia2) {
              continue;
            } else {
              return token = SyntaxKind.ConflictMarkerTrivia;
            }
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.bar) {
            if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
              return pos += 3, token = SyntaxKind.BarBarEqualsToken;
            }
            return pos += 2, token = SyntaxKind.BarBarToken;
          }
          if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
            return pos += 2, token = SyntaxKind.BarEqualsToken;
          }
          pos++;
          return token = SyntaxKind.BarToken;
        case CharacterCodes.closeBrace:
          pos++;
          return token = SyntaxKind.CloseBraceToken;
        case CharacterCodes.tilde:
          pos++;
          return token = SyntaxKind.TildeToken;
        case CharacterCodes.at:
          pos++;
          return token = SyntaxKind.AtToken;
        case CharacterCodes.backslash: {
          const extendedCookedChar = peekExtendedUnicodeEscape();
          if (extendedCookedChar >= 0 && isIdentifierStart(extendedCookedChar)) {
            tokenValue = scanExtendedUnicodeEscape() + scanIdentifierParts();
            return token = getIdentifierToken();
          }
          const cookedChar = peekUnicodeEscape();
          if (cookedChar >= 0 && isIdentifierStart(cookedChar)) {
            pos += 6;
            tokenFlags |= TokenFlags.UnicodeEscape;
            tokenValue = String.fromCharCode(cookedChar) + scanIdentifierParts();
            return token = getIdentifierToken();
          }
          pos++;
          return token = SyntaxKind.Unknown;
        }
        case CharacterCodes.hash:
          if (pos !== 0 && text[pos + 1] === "!") {
            pos++;
            return token = SyntaxKind.Unknown;
          }
          {
            const charAfterHash = codePointUnchecked(pos + 1);
            if (charAfterHash === CharacterCodes.backslash) {
              pos++;
              const extendedCookedChar = peekExtendedUnicodeEscape();
              if (extendedCookedChar >= 0 && isIdentifierStart(extendedCookedChar)) {
                tokenValue = "#" + scanExtendedUnicodeEscape() + scanIdentifierParts();
                return token = SyntaxKind.PrivateIdentifier;
              }
              const cookedChar = peekUnicodeEscape();
              if (cookedChar >= 0 && isIdentifierStart(cookedChar)) {
                pos += 6;
                tokenFlags |= TokenFlags.UnicodeEscape;
                tokenValue = "#" + String.fromCharCode(cookedChar) + scanIdentifierParts();
                return token = SyntaxKind.PrivateIdentifier;
              }
              pos--;
            }
            if (isIdentifierStart(charAfterHash)) {
              pos++;
              scanIdentifier(charAfterHash);
            } else {
              tokenValue = "#";
            }
            return token = SyntaxKind.PrivateIdentifier;
          }
        case CharacterCodes.replacementCharacter:
          pos = end;
          return token = SyntaxKind.NonTextFileMarkerTrivia;
        default: {
          const identifierKind = scanIdentifier(ch);
          if (identifierKind) {
            return token = identifierKind;
          } else if (isWhiteSpaceSingleLine(ch)) {
            pos += charSize(ch);
            continue;
          } else if (isLineBreak(ch)) {
            tokenFlags |= TokenFlags.PrecedingLineBreak;
            pos += charSize(ch);
            continue;
          }
          const size = charSize(ch);
          pos += size;
          return token = SyntaxKind.Unknown;
        }
      }
    }
  }
  function reScanInvalidIdentifier() {
    pos = tokenStart = fullStartPos;
    tokenFlags = 0;
    const ch = codePointUnchecked(pos);
    const identifierKind = scanIdentifier(ch);
    if (identifierKind) {
      return token = identifierKind;
    }
    pos += charSize(ch);
    return token;
  }
  function scanIdentifier(startCharacter) {
    let ch = startCharacter;
    if (isIdentifierStart(ch)) {
      pos += charSize(ch);
      while (pos < end && isIdentifierPart(ch = codePointUnchecked(pos)))
        pos += charSize(ch);
      tokenValue = text.substring(tokenStart, pos);
      if (ch === CharacterCodes.backslash) {
        tokenValue += scanIdentifierParts();
      }
      return getIdentifierToken();
    }
  }
  function reScanGreaterToken() {
    if (token === SyntaxKind.GreaterThanToken) {
      if (charCodeUnchecked(pos) === CharacterCodes.greaterThan) {
        if (charCodeUnchecked(pos + 1) === CharacterCodes.greaterThan) {
          if (charCodeUnchecked(pos + 2) === CharacterCodes.equals) {
            return pos += 3, token = SyntaxKind.GreaterThanGreaterThanGreaterThanEqualsToken;
          }
          return pos += 2, token = SyntaxKind.GreaterThanGreaterThanGreaterThanToken;
        }
        if (charCodeUnchecked(pos + 1) === CharacterCodes.equals) {
          return pos += 2, token = SyntaxKind.GreaterThanGreaterThanEqualsToken;
        }
        pos++;
        return token = SyntaxKind.GreaterThanGreaterThanToken;
      }
      if (charCodeUnchecked(pos) === CharacterCodes.equals) {
        pos++;
        return token = SyntaxKind.GreaterThanEqualsToken;
      }
    }
    return token;
  }
  function reScanAsteriskEqualsToken() {
    pos = tokenStart + 1;
    return token = SyntaxKind.EqualsToken;
  }
  function reScanSlashToken() {
    if (token === SyntaxKind.SlashToken || token === SyntaxKind.SlashEqualsToken) {
      const startOfRegExpBody = tokenStart + 1;
      pos = startOfRegExpBody;
      let inEscape = false;
      let inCharacterClass = false;
      while (true) {
        const ch = charCodeChecked(pos);
        if (ch === CharacterCodes.EOF || isLineBreak(ch)) {
          tokenFlags |= TokenFlags.Unterminated;
          break;
        }
        if (inEscape) {
          inEscape = false;
        } else if (ch === CharacterCodes.slash && !inCharacterClass) {
          break;
        } else if (ch === CharacterCodes.openBracket) {
          inCharacterClass = true;
        } else if (ch === CharacterCodes.backslash) {
          inEscape = true;
        } else if (ch === CharacterCodes.closeBracket) {
          inCharacterClass = false;
        }
        pos++;
      }
      if (tokenFlags & TokenFlags.Unterminated) {
        pos = startOfRegExpBody;
        inEscape = false;
        let characterClassDepth = 0;
        let inDecimalQuantifier = false;
        let groupDepth = 0;
        while (pos < end && !isLineBreak(charCodeUnchecked(pos))) {
          const ch = charCodeUnchecked(pos);
          if (inEscape) {
            inEscape = false;
          } else if (ch === CharacterCodes.backslash) {
            inEscape = true;
          } else if (ch === CharacterCodes.openBracket) {
            characterClassDepth++;
          } else if (ch === CharacterCodes.closeBracket && characterClassDepth) {
            characterClassDepth--;
          } else if (!characterClassDepth) {
            if (ch === CharacterCodes.openBrace) {
              inDecimalQuantifier = true;
            } else if (ch === CharacterCodes.closeBrace && inDecimalQuantifier) {
              inDecimalQuantifier = false;
            } else if (!inDecimalQuantifier) {
              if (ch === CharacterCodes.openParen) {
                groupDepth++;
              } else if (ch === CharacterCodes.closeParen && groupDepth) {
                groupDepth--;
              } else if (ch === CharacterCodes.closeParen || ch === CharacterCodes.closeBracket || ch === CharacterCodes.closeBrace) {
                break;
              }
            }
          }
          pos++;
        }
        while (isWhiteSpaceLike(charCodeChecked(pos - 1)) || charCodeChecked(pos - 1) === CharacterCodes.semicolon)
          pos--;
      } else {
        pos++;
        let regExpFlags = RegularExpressionFlags.None;
        while (true) {
          const ch = codePointChecked(pos);
          if (ch === CharacterCodes.EOF || !isIdentifierPart(ch)) {
            break;
          }
          const size = charSize(ch);
          const flag = characterCodeToRegularExpressionFlag(ch);
          if (flag !== void 0) {
            regExpFlags |= flag;
          }
          pos += size;
        }
      }
      tokenValue = text.substring(tokenStart, pos);
      token = SyntaxKind.RegularExpressionLiteral;
    }
    return token;
  }
  function appendIfCommentDirective(commentDirectives2, text2, commentDirectiveRegEx, lineStart) {
    const type = getDirectiveFromComment(text2.trimStart(), commentDirectiveRegEx);
    if (type === void 0) {
      return commentDirectives2;
    }
    if (!commentDirectives2) {
      commentDirectives2 = [];
    }
    commentDirectives2.push({
      range: { pos: lineStart, end: pos },
      type
    });
    return commentDirectives2;
  }
  function getDirectiveFromComment(text2, commentDirectiveRegEx) {
    const match = commentDirectiveRegEx.exec(text2);
    if (!match) {
      return void 0;
    }
    switch (match[1]) {
      case "ts-expect-error":
        return CommentDirectiveType.ExpectError;
      case "ts-ignore":
        return CommentDirectiveType.Ignore;
    }
    return void 0;
  }
  function reScanTemplateToken(isTaggedTemplate) {
    pos = tokenStart;
    return token = scanTemplateAndSetTokenValue(!isTaggedTemplate);
  }
  function reScanTemplateHeadOrNoSubstitutionTemplate() {
    pos = tokenStart;
    return token = scanTemplateAndSetTokenValue(
      /*shouldEmitInvalidEscapeError*/
      true
    );
  }
  function reScanJsxToken(allowMultilineJsxText = true) {
    pos = tokenStart = fullStartPos;
    return token = scanJsxToken(allowMultilineJsxText);
  }
  function reScanLessThanToken() {
    if (token === SyntaxKind.LessThanLessThanToken) {
      pos = tokenStart + 1;
      return token = SyntaxKind.LessThanToken;
    }
    return token;
  }
  function reScanHashToken() {
    if (token === SyntaxKind.PrivateIdentifier) {
      pos = tokenStart + 1;
      return token = SyntaxKind.HashToken;
    }
    return token;
  }
  function reScanQuestionToken() {
    pos = tokenStart + 1;
    return token = SyntaxKind.QuestionToken;
  }
  function scanJsxToken(allowMultilineJsxText = true) {
    fullStartPos = tokenStart = pos;
    if (pos >= end) {
      return token = SyntaxKind.EndOfFile;
    }
    let char = charCodeUnchecked(pos);
    if (char === CharacterCodes.lessThan) {
      if (charCodeUnchecked(pos + 1) === CharacterCodes.slash) {
        pos += 2;
        return token = SyntaxKind.LessThanSlashToken;
      }
      pos++;
      return token = SyntaxKind.LessThanToken;
    }
    if (char === CharacterCodes.openBrace) {
      pos++;
      return token = SyntaxKind.OpenBraceToken;
    }
    let firstNonWhitespace = 0;
    while (pos < end) {
      char = charCodeUnchecked(pos);
      if (char === CharacterCodes.openBrace) {
        break;
      }
      if (char === CharacterCodes.lessThan) {
        if (isConflictMarkerTrivia(text, pos)) {
          pos = scanConflictMarkerTrivia(text, pos);
          return token = SyntaxKind.ConflictMarkerTrivia;
        }
        break;
      }
      if (isLineBreak(char) && firstNonWhitespace === 0) {
        firstNonWhitespace = -1;
      } else if (!allowMultilineJsxText && isLineBreak(char) && firstNonWhitespace > 0) {
        break;
      } else if (!isWhiteSpaceLike(char)) {
        firstNonWhitespace = pos;
      }
      pos++;
    }
    tokenValue = text.substring(fullStartPos, pos);
    return firstNonWhitespace === -1 ? SyntaxKind.JsxTextAllWhiteSpaces : SyntaxKind.JsxText;
  }
  function scanJsxIdentifier() {
    if (tokenIsIdentifierOrKeyword(token)) {
      while (pos < end) {
        const ch = charCodeUnchecked(pos);
        if (ch === CharacterCodes.minus) {
          tokenValue += "-";
          pos++;
          continue;
        }
        const oldPos = pos;
        tokenValue += scanIdentifierParts();
        if (pos === oldPos) {
          break;
        }
      }
      return getIdentifierToken();
    }
    return token;
  }
  function scanJsxAttributeValue() {
    fullStartPos = pos;
    switch (charCodeUnchecked(pos)) {
      case CharacterCodes.doubleQuote:
      case CharacterCodes.singleQuote:
        tokenValue = scanString(
          /*jsxAttributeString*/
          true
        );
        return token = SyntaxKind.StringLiteral;
      default:
        return scan();
    }
  }
  function reScanJsxAttributeValue() {
    pos = tokenStart = fullStartPos;
    return scanJsxAttributeValue();
  }
  function scanJSDocCommentTextToken(inBackticks) {
    fullStartPos = tokenStart = pos;
    tokenFlags = TokenFlags.None;
    if (pos >= end) {
      return token = SyntaxKind.EndOfFile;
    }
    for (let ch = charCodeUnchecked(pos); pos < end && (!isLineBreak(ch) && ch !== CharacterCodes.backtick); ch = codePointUnchecked(++pos)) {
      if (!inBackticks) {
        if (ch === CharacterCodes.openBrace) {
          break;
        } else if (ch === CharacterCodes.at && pos - 1 >= 0 && isWhiteSpaceSingleLine(charCodeUnchecked(pos - 1)) && pos + 1 < end && isIdentifierStart(charCodeUnchecked(pos + 1))) {
          break;
        }
      }
    }
    if (pos === tokenStart) {
      return scanJsDocToken();
    }
    tokenValue = text.substring(tokenStart, pos);
    return token = SyntaxKind.JSDocCommentTextToken;
  }
  function scanJsDocToken() {
    fullStartPos = tokenStart = pos;
    tokenFlags = TokenFlags.None;
    if (pos >= end) {
      return token = SyntaxKind.EndOfFile;
    }
    const ch = codePointUnchecked(pos);
    pos += charSize(ch);
    switch (ch) {
      case CharacterCodes.tab:
      case CharacterCodes.verticalTab:
      case CharacterCodes.formFeed:
      case CharacterCodes.space:
        while (pos < end && isWhiteSpaceSingleLine(charCodeUnchecked(pos))) {
          pos++;
        }
        return token = SyntaxKind.WhitespaceTrivia;
      case CharacterCodes.at:
        return token = SyntaxKind.AtToken;
      case CharacterCodes.carriageReturn:
        if (charCodeUnchecked(pos) === CharacterCodes.lineFeed) {
          pos++;
        }
      // falls through
      case CharacterCodes.lineFeed:
        tokenFlags |= TokenFlags.PrecedingLineBreak;
        return token = SyntaxKind.NewLineTrivia;
      case CharacterCodes.asterisk:
        return token = SyntaxKind.AsteriskToken;
      case CharacterCodes.openBrace:
        return token = SyntaxKind.OpenBraceToken;
      case CharacterCodes.closeBrace:
        return token = SyntaxKind.CloseBraceToken;
      case CharacterCodes.openBracket:
        return token = SyntaxKind.OpenBracketToken;
      case CharacterCodes.closeBracket:
        return token = SyntaxKind.CloseBracketToken;
      case CharacterCodes.openParen:
        return token = SyntaxKind.OpenParenToken;
      case CharacterCodes.closeParen:
        return token = SyntaxKind.CloseParenToken;
      case CharacterCodes.lessThan:
        return token = SyntaxKind.LessThanToken;
      case CharacterCodes.greaterThan:
        return token = SyntaxKind.GreaterThanToken;
      case CharacterCodes.equals:
        return token = SyntaxKind.EqualsToken;
      case CharacterCodes.comma:
        return token = SyntaxKind.CommaToken;
      case CharacterCodes.dot:
        return token = SyntaxKind.DotToken;
      case CharacterCodes.backtick:
        return token = SyntaxKind.BacktickToken;
      case CharacterCodes.hash:
        return token = SyntaxKind.HashToken;
      case CharacterCodes.backslash:
        pos--;
        {
          const extendedCookedChar = peekExtendedUnicodeEscape();
          if (extendedCookedChar >= 0 && isIdentifierStart(extendedCookedChar)) {
            tokenValue = scanExtendedUnicodeEscape() + scanIdentifierParts();
            return token = getIdentifierToken();
          }
          const cookedChar = peekUnicodeEscape();
          if (cookedChar >= 0 && isIdentifierStart(cookedChar)) {
            pos += 6;
            tokenFlags |= TokenFlags.UnicodeEscape;
            tokenValue = String.fromCharCode(cookedChar) + scanIdentifierParts();
            return token = getIdentifierToken();
          }
        }
        pos++;
        return token = SyntaxKind.Unknown;
    }
    if (isIdentifierStart(ch)) {
      let char = ch;
      while (pos < end && isIdentifierPart(char = codePointUnchecked(pos)) || char === CharacterCodes.minus)
        pos += charSize(char);
      tokenValue = text.substring(tokenStart, pos);
      if (char === CharacterCodes.backslash) {
        tokenValue += scanIdentifierParts();
      }
      return token = getIdentifierToken();
    } else {
      return token = SyntaxKind.Unknown;
    }
  }
  function speculationHelper(callback, isLookahead) {
    const savePos = pos;
    const saveStartPos = fullStartPos;
    const saveTokenPos = tokenStart;
    const saveToken = token;
    const saveTokenValue = tokenValue;
    const saveTokenFlags = tokenFlags;
    const result = callback();
    if (!result || isLookahead) {
      pos = savePos;
      fullStartPos = saveStartPos;
      tokenStart = saveTokenPos;
      token = saveToken;
      tokenValue = saveTokenValue;
      tokenFlags = saveTokenFlags;
    }
    return result;
  }
  function scanRange(start2, length2, callback) {
    const saveEnd = end;
    const savePos = pos;
    const saveStartPos = fullStartPos;
    const saveTokenPos = tokenStart;
    const saveToken = token;
    const saveTokenValue = tokenValue;
    const saveTokenFlags = tokenFlags;
    const saveErrorExpectations = commentDirectives;
    setText(text, start2, length2);
    const result = callback();
    end = saveEnd;
    pos = savePos;
    fullStartPos = saveStartPos;
    tokenStart = saveTokenPos;
    token = saveToken;
    tokenValue = saveTokenValue;
    tokenFlags = saveTokenFlags;
    commentDirectives = saveErrorExpectations;
    return result;
  }
  function lookAhead(callback) {
    return speculationHelper(
      callback,
      /*isLookahead*/
      true
    );
  }
  function tryScan(callback) {
    return speculationHelper(
      callback,
      /*isLookahead*/
      false
    );
  }
  function getText() {
    return text;
  }
  function clearCommentDirectives() {
    commentDirectives = void 0;
  }
  function setText(newText, start2, length2) {
    text = newText || "";
    end = length2 === void 0 ? text.length : start2 + length2;
    resetTokenState(start2 || 0);
  }
  function setLanguageVariant(variant) {
    languageVariant = variant;
  }
  function resetTokenState(position) {
    pos = position;
    fullStartPos = position;
    tokenStart = position;
    token = SyntaxKind.Unknown;
    tokenValue = void 0;
    tokenFlags = TokenFlags.None;
  }
  function setSkipJsDocLeadingAsterisks(skip) {
    skipJsDocLeadingAsterisks += skip ? 1 : -1;
  }
}

// node_modules/typescript/dist/ast/astnav.js
function getTokenAtPosition(sourceFile, position) {
  return getTokenAtPositionImpl(
    sourceFile,
    position,
    /*allowPositionInLeadingTrivia*/
    true,
    /*includePrecedingTokenAtEndPosition*/
    void 0
  );
}
function getTouchingPropertyName(sourceFile, position) {
  return getTokenAtPositionImpl(
    sourceFile,
    position,
    /*allowPositionInLeadingTrivia*/
    false,
    (node) => isPropertyNameLiteral(node) || isKeywordKind(node.kind) || isPrivateIdentifier(node)
  );
}
function getTouchingToken(sourceFile, position) {
  return getTokenAtPositionImpl(
    sourceFile,
    position,
    /*allowPositionInLeadingTrivia*/
    false,
    /*includePrecedingTokenAtEndPosition*/
    void 0
  );
}
function findNextToken(previousToken, parent, sourceFile) {
  return find(parent);
  function find(n) {
    if (isTokenKind(n.kind) && n.pos === previousToken.end) {
      return n;
    }
    let foundNode;
    const visitChild = (node) => {
      if (node.flags & NodeFlags.Reparsed) {
        return void 0;
      }
      if (node.pos <= previousToken.end && node.end > previousToken.end) {
        foundNode = node;
      }
      return void 0;
    };
    if (n.jsDoc) {
      for (const jsdoc of n.jsDoc) {
        visitChild(jsdoc);
      }
    }
    n.forEachChild(visitChild, (nodes) => {
      if (nodes.length > 0 && foundNode === void 0) {
        for (const node of nodes) {
          if (node.flags & NodeFlags.Reparsed)
            continue;
          if (node.pos > previousToken.end)
            break;
          if (node.end > previousToken.end) {
            foundNode = node;
            break;
          }
        }
      }
      return void 0;
    });
    if (foundNode !== void 0) {
      return find(foundNode);
    }
    const startPos = previousToken.end;
    if (startPos >= n.pos && startPos < n.end) {
      const scanner = getScannerForSourceFile(sourceFile, startPos);
      const token = scanner.getToken();
      const tokenFullStart = scanner.getTokenFullStart();
      const tokenEnd = scanner.getTokenEnd();
      const flags = scanner.getTokenFlags();
      return getOrCreateToken(sourceFile, token, tokenFullStart, tokenEnd, n, flags);
    }
    return void 0;
  }
}
function findPrecedingToken(sourceFile, position) {
  return findPrecedingTokenImpl(sourceFile, position, sourceFile);
}
function getTokenAtPositionImpl(sourceFile, position, allowPositionInLeadingTrivia, includePrecedingTokenAtEndPosition) {
  let current = sourceFile;
  let nodeAfterLeft;
  const state = {
    next: void 0,
    prevSubtree: void 0,
    left: 0
  };
  const testNode = (node) => {
    if (node.kind !== SyntaxKind.EndOfFile && node.end === position && includePrecedingTokenAtEndPosition !== void 0) {
      state.prevSubtree = node;
    }
    if (node.end < position || node.end === position && node.kind !== SyntaxKind.EndOfFile && (!isJSDocNodeKind(node.kind) || node.end !== sourceFile.endOfFileToken.end)) {
      return -1;
    }
    const nodePos = getPosition(node, sourceFile, allowPositionInLeadingTrivia);
    if (nodePos > position) {
      return 1;
    }
    return 0;
  };
  while (true) {
    state.next = void 0;
    nodeAfterLeft = void 0;
    let skipSingleCommentChildren = false;
    const visitNode3 = (node) => {
      if (node.flags & NodeFlags.Reparsed) {
        return void 0;
      }
      if (skipSingleCommentChildren && isJSDocCommentChildKind(node.kind)) {
        return void 0;
      }
      if (nodeAfterLeft === void 0) {
        nodeAfterLeft = node;
      }
      if (state.next === void 0) {
        const result = testNode(node);
        switch (result) {
          case -1:
            if (!isJSDocNodeKind(node.kind)) {
              state.left = node.end;
            }
            nodeAfterLeft = void 0;
            break;
          case 0:
            state.next = node;
            break;
        }
      }
      return void 0;
    };
    if (current.jsDoc) {
      for (const jsdoc of current.jsDoc) {
        visitNode3(jsdoc);
      }
    }
    current.forEachChild(visitNode3, (nodes) => {
      skipSingleCommentChildren = isJSDocSingleCommentNodeList(nodes);
      if (nodes.length === 0 || skipSingleCommentChildren) {
        return void 0;
      }
      if (nodeAfterLeft === void 0) {
        for (const node of nodes) {
          if (!(node.flags & NodeFlags.Reparsed)) {
            nodeAfterLeft = node;
            break;
          }
        }
      }
      if (state.next === void 0) {
        if (nodes.end === position && includePrecedingTokenAtEndPosition !== void 0) {
          state.left = nodes.end;
          nodeAfterLeft = void 0;
          state.prevSubtree = nodes[nodes.length - 1];
        } else if (nodes.end <= position) {
          state.left = nodes.end;
          nodeAfterLeft = void 0;
        } else if (nodes.pos <= position) {
          binarySearchNodeList(nodes, testNode, (node, middle, arr) => {
            state.left = node.end;
            nodeAfterLeft = void 0;
            for (let i = middle + 1; i < arr.length; i++) {
              if (!(arr[i].flags & NodeFlags.Reparsed)) {
                nodeAfterLeft = arr[i];
                break;
              }
            }
          }, (found) => {
            state.next = found;
          });
        }
      }
      return void 0;
    });
    if (state.prevSubtree !== void 0) {
      const child = findPrecedingTokenImpl(sourceFile, position, state.prevSubtree);
      if (child !== void 0 && child.end === position && includePrecedingTokenAtEndPosition(child)) {
        return child;
      }
      state.prevSubtree = void 0;
    }
    if (state.next === void 0) {
      if (isTokenKind(current.kind) || shouldSkipChild(current)) {
        return current;
      }
      const scanner = getScannerForSourceFile(sourceFile, state.left);
      let end = current.end;
      const afterLeft = nodeAfterLeft;
      if (afterLeft !== void 0) {
        end = afterLeft.pos;
      }
      while (state.left < end) {
        const token = scanner.getToken();
        const tokenFullStart = scanner.getTokenFullStart();
        const tokenStart = allowPositionInLeadingTrivia ? tokenFullStart : scanner.getTokenStart();
        const tokenEnd = scanner.getTokenEnd();
        const flags = scanner.getTokenFlags();
        if (tokenEnd > end) {
          break;
        }
        if (tokenStart <= position && position < tokenEnd) {
          if (token === SyntaxKind.Identifier || !isTokenKind(token)) {
            if (isJSDocNodeKind(current.kind)) {
              return current;
            }
            throw new Error(`did not expect ${SyntaxKind[current.kind]} to have ${SyntaxKind[token]} in its trivia`);
          }
          return getOrCreateToken(sourceFile, token, tokenFullStart, tokenEnd, current, flags);
        }
        if (includePrecedingTokenAtEndPosition !== void 0 && tokenEnd === position) {
          const prevToken = getOrCreateToken(sourceFile, token, tokenFullStart, tokenEnd, current, flags);
          if (includePrecedingTokenAtEndPosition(prevToken)) {
            return prevToken;
          }
        }
        state.left = tokenEnd;
        scanner.scan();
      }
      return current;
    }
    current = state.next;
    state.left = current.pos;
    nodeAfterLeft = void 0;
  }
}
function getPosition(node, sourceFile, allowPositionInLeadingTrivia) {
  if (allowPositionInLeadingTrivia) {
    return node.pos;
  }
  return getTokenPosOfNode(
    node,
    sourceFile,
    /*includeJSDoc*/
    true
  );
}
function getTokenPosOfNode(node, sourceFile, includeJSDoc) {
  if (nodeIsMissing(node)) {
    return node.pos;
  }
  if (isJSDocNodeKind(node.kind) || node.kind === SyntaxKind.JsxText) {
    return skipTrivia(
      sourceFile.text,
      node.pos,
      /*stopAfterLineBreak*/
      false,
      /*stopAtComments*/
      true
    );
  }
  if (includeJSDoc && node.jsDoc && node.jsDoc.length > 0) {
    return getTokenPosOfNode(
      node.jsDoc[0],
      sourceFile,
      /*includeJSDoc*/
      false
    );
  }
  return skipTrivia(
    sourceFile.text,
    node.pos,
    /*stopAfterLineBreak*/
    false,
    /*stopAtComments*/
    false,
    /*inJSDoc*/
    !!(node.flags & NodeFlags.JSDoc)
  );
}
function nodeIsMissing(node) {
  return node.pos === node.end && node.pos >= 0 && node.kind !== SyntaxKind.EndOfFile;
}
function findPrecedingTokenImpl(sourceFile, position, startNode) {
  const find = (n) => {
    if (isTokenKind(n.kind) && n.kind !== SyntaxKind.EndOfFile) {
      return n;
    }
    let foundChild;
    let prevChild;
    if (n.jsDoc) {
      for (const jsdoc of n.jsDoc) {
        if (jsdoc.flags & NodeFlags.Reparsed)
          continue;
        if (foundChild !== void 0)
          break;
        if (position < jsdoc.end && (prevChild === void 0 || prevChild.end <= position)) {
          foundChild = jsdoc;
        } else {
          prevChild = jsdoc;
        }
      }
    }
    let skipSingleCommentChildrenImpl = false;
    n.forEachChild((node) => {
      if (node.flags & NodeFlags.Reparsed) {
        return void 0;
      }
      if (skipSingleCommentChildrenImpl && isJSDocCommentChildKind(node.kind)) {
        return void 0;
      }
      if (foundChild !== void 0) {
        return void 0;
      }
      if (position < node.end && (prevChild === void 0 || prevChild.end <= position)) {
        foundChild = node;
      } else {
        prevChild = node;
      }
      return void 0;
    }, (nodes) => {
      skipSingleCommentChildrenImpl = isJSDocSingleCommentNodeList(nodes);
      if (foundChild !== void 0) {
        return void 0;
      }
      if (nodes.length > 0 && !skipSingleCommentChildrenImpl) {
        const index = binarySearchForPrecedingToken(nodes, position);
        if (index >= 0 && !(nodes[index].flags & NodeFlags.Reparsed)) {
          foundChild = nodes[index];
        }
        const lookupIndex = index >= 0 ? index - 1 : nodes.length - 1;
        for (let i = lookupIndex; i >= 0; i--) {
          if (!(nodes[i].flags & NodeFlags.Reparsed)) {
            if (prevChild === void 0) {
              prevChild = nodes[i];
            }
            break;
          }
        }
      }
      return void 0;
    });
    if (foundChild !== void 0) {
      const start = getTokenPosOfNode(
        foundChild,
        sourceFile,
        /*includeJSDoc*/
        true
      );
      if (start >= position) {
        if (position >= foundChild.pos) {
          let jsDoc;
          if (n.jsDoc) {
            for (let i = n.jsDoc.length - 1; i >= 0; i--) {
              if (n.jsDoc[i].pos >= foundChild.pos) {
                jsDoc = n.jsDoc[i];
                break;
              }
            }
          }
          if (jsDoc !== void 0) {
            if (position < jsDoc.end) {
              return find(jsDoc);
            }
            return findRightmostValidToken(sourceFile, jsDoc.end, n, position);
          }
          return findRightmostValidToken(sourceFile, foundChild.pos, n, -1);
        }
        return findRightmostValidToken(sourceFile, foundChild.pos, n, position);
      }
      return find(foundChild);
    }
    if (position >= n.end) {
      return findRightmostValidToken(sourceFile, n.end, n, -1);
    }
    return findRightmostValidToken(sourceFile, n.end, n, position);
  };
  return find(startNode);
}
function findRightmostValidToken(sourceFile, endPos, containingNode, position) {
  if (position === -1) {
    position = containingNode.end;
  }
  const find = (n, endPos2) => {
    if (isTokenKind(n.kind) && n.kind !== SyntaxKind.EndOfFile) {
      return n;
    }
    let rightmostValidNode;
    let hasChildren = false;
    if (n.jsDoc) {
      hasChildren = true;
      for (const jsdoc of n.jsDoc) {
        if (jsdoc.flags & NodeFlags.Reparsed)
          continue;
        if (jsdoc.end > endPos2 || getTokenPosOfNode(jsdoc, sourceFile) >= position)
          continue;
        if (isValidPrecedingNode(jsdoc, sourceFile)) {
          rightmostValidNode = jsdoc;
        }
      }
    }
    let skipSingleCommentChildren = false;
    n.forEachChild((node) => {
      if (node.flags & NodeFlags.Reparsed) {
        return void 0;
      }
      if (skipSingleCommentChildren && isJSDocCommentChildKind(node.kind)) {
        return void 0;
      }
      hasChildren = true;
      if (node.end > endPos2 || getTokenPosOfNode(node, sourceFile) >= position) {
        return void 0;
      }
      if (isValidPrecedingNode(node, sourceFile)) {
        rightmostValidNode = node;
      }
      return void 0;
    }, (nodes) => {
      skipSingleCommentChildren = isJSDocSingleCommentNodeList(nodes);
      if (nodes.length > 0 && !skipSingleCommentChildren) {
        hasChildren = true;
        for (let i = nodes.length - 1; i >= 0; i--) {
          const node = nodes[i];
          if (node.flags & NodeFlags.Reparsed)
            continue;
          if (node.end > endPos2 || getTokenPosOfNode(node, sourceFile) >= position)
            continue;
          if (isValidPrecedingNode(node, sourceFile)) {
            rightmostValidNode = node;
            break;
          }
        }
      }
      return void 0;
    });
    if (!shouldSkipChild(n)) {
      const startPos = rightmostValidNode !== void 0 ? rightmostValidNode.end : n.pos;
      const targetEnd = Math.min(endPos2, position);
      if (startPos < targetEnd) {
        const scanner = getScannerForSourceFile(sourceFile, startPos);
        let pos = startPos;
        let lastScannedToken;
        while (pos < targetEnd) {
          const tokenStart = scanner.getTokenStart();
          if (tokenStart >= position)
            break;
          const tokenFullStart = scanner.getTokenFullStart();
          const tokenEnd = scanner.getTokenEnd();
          const token = scanner.getToken();
          const flags = scanner.getTokenFlags();
          lastScannedToken = getOrCreateToken(sourceFile, token, tokenFullStart, tokenEnd, n, flags);
          pos = tokenEnd;
          scanner.scan();
        }
        if (lastScannedToken !== void 0) {
          return lastScannedToken;
        }
      }
    }
    if (!hasChildren) {
      if (n !== containingNode) {
        return n;
      }
      return void 0;
    }
    if (rightmostValidNode !== void 0) {
      return find(rightmostValidNode, rightmostValidNode.end);
    }
    return void 0;
  };
  return find(containingNode, endPos);
}
function isValidPrecedingNode(node, sourceFile) {
  if (node.kind === SyntaxKind.EndOfFile) {
    return false;
  }
  const start = getTokenPosOfNode(node, sourceFile);
  const width = node.end - start;
  return width > 0;
}
function shouldSkipChild(node) {
  return node.kind === SyntaxKind.JSDoc || node.kind === SyntaxKind.JSDocText || node.kind === SyntaxKind.JSDocTypeLiteral || node.kind === SyntaxKind.JSDocSignature || node.kind === SyntaxKind.JSDocLink || node.kind === SyntaxKind.JSDocLinkCode || node.kind === SyntaxKind.JSDocLinkPlain || isJSDocTag(node);
}
function isJSDocTag(node) {
  return node.kind >= SyntaxKind.FirstJSDocTagNode && node.kind <= SyntaxKind.LastJSDocTagNode;
}
function isJSDocCommentChildKind(kind) {
  switch (kind) {
    case SyntaxKind.JSDocText:
    case SyntaxKind.JSDocLink:
    case SyntaxKind.JSDocLinkCode:
    case SyntaxKind.JSDocLinkPlain:
      return true;
    default:
      return false;
  }
}
function isJSDocSingleCommentNodeList(nodes) {
  return nodes.length === 1 && isJSDocCommentChildKind(nodes[0].kind);
}
function getScannerForSourceFile(sourceFile, pos) {
  const scanner = createScanner(
    /*skipTrivia*/
    true,
    sourceFile.languageVariant,
    sourceFile.text
  );
  scanner.resetTokenState(pos);
  scanner.scan();
  return scanner;
}
function getOrCreateToken(sourceFile, kind, pos, end, parent, _flags) {
  const key = `${pos}_${end}`;
  if (!sourceFile.tokenCache) {
    sourceFile.tokenCache = /* @__PURE__ */ new Map();
  }
  const existing = sourceFile.tokenCache.get(key);
  if (existing !== void 0) {
    return existing;
  }
  const token = createToken(kind);
  token.pos = pos;
  token.end = end;
  token.parent = parent;
  sourceFile.tokenCache.set(key, token);
  return token;
}
function binarySearchNodeList(nodes, testNode, onLeft, onMatch) {
  let lo = 0;
  let hi = nodes.length - 1;
  while (lo <= hi) {
    const mid = lo + hi >>> 1;
    const node = nodes[mid];
    if (node.flags & NodeFlags.Reparsed) {
      let found = false;
      for (let i = mid + 1; i <= hi; i++) {
        if (!(nodes[i].flags & NodeFlags.Reparsed)) {
          const cmp2 = testNode(nodes[i]);
          if (cmp2 < 0) {
            onLeft(nodes[i], i, nodes);
            lo = i + 1;
          } else if (cmp2 > 0) {
            hi = i - 1;
          } else {
            onMatch(nodes[i]);
            return;
          }
          found = true;
          break;
        }
      }
      if (!found) {
        hi = mid - 1;
      }
      continue;
    }
    const cmp = testNode(node);
    if (cmp < 0) {
      onLeft(node, mid, nodes);
      lo = mid + 1;
    } else if (cmp > 0) {
      hi = mid - 1;
    } else {
      onMatch(node);
      return;
    }
  }
}
function binarySearchForPrecedingToken(nodes, position) {
  let lo = 0;
  let hi = nodes.length - 1;
  let result = -1;
  while (lo <= hi) {
    const mid = lo + hi >>> 1;
    const node = nodes[mid];
    if (node.flags & NodeFlags.Reparsed) {
      lo = mid + 1;
      continue;
    }
    if (position < node.end) {
      if (mid === 0 || position >= nodes[mid - 1].end) {
        result = mid;
        break;
      }
      hi = mid - 1;
    } else {
      lo = mid + 1;
    }
  }
  return result;
}

// node_modules/typescript/dist/ast/clone.js
function isArray(value) {
  return Array.isArray(value);
}
function forEachChildRecursively(rootNode, cbNode, cbNodes) {
  const queue = gatherPossibleChildren(rootNode);
  const parents = [];
  while (parents.length < queue.length) {
    parents.push(rootNode);
  }
  while (queue.length !== 0) {
    const current = queue.pop();
    const parent = parents.pop();
    if (isArray(current)) {
      if (cbNodes) {
        const res = cbNodes(current, parent);
        if (res) {
          if (res === "skip")
            continue;
          return res;
        }
      }
      for (let i = current.length - 1; i >= 0; --i) {
        queue.push(current[i]);
        parents.push(parent);
      }
    } else {
      const res = cbNode(current, parent);
      if (res) {
        if (res === "skip")
          continue;
        return res;
      }
      if (current.kind >= SyntaxKind.FirstNode) {
        for (const child of gatherPossibleChildren(current)) {
          queue.push(child);
          parents.push(current);
        }
      }
    }
  }
}
function gatherPossibleChildren(node) {
  const children = [];
  node.forEachChild(addWorkItem, addWorkItem);
  return children;
  function addWorkItem(n) {
    children.unshift(n);
  }
}
function setParentRecursive(rootNode) {
  if (rootNode === void 0)
    return rootNode;
  forEachChildRecursively(rootNode, (child, parent) => {
    child.parent = parent;
    return void 0;
  });
  return rootNode;
}
function setTextRange(node, range) {
  if (range) {
    node.pos = range.pos;
    node.end = range.end;
  }
  return node;
}
function getSynthesizedDeepClone(node, includeTrivia = true) {
  const clone = node && getSynthesizedDeepCloneWorker(node);
  if (clone && !includeTrivia) {
    clone.pos = -1;
    clone.end = -1;
  }
  return setParentRecursive(clone);
}
function getSynthesizedDeepClones(nodes, includeTrivia = true) {
  if (nodes) {
    const cloned = createNodeArray(nodes.map((n) => getSynthesizedDeepClone(n, includeTrivia)), nodes.pos, nodes.end);
    return cloned;
  }
  return nodes;
}
function getSynthesizedDeepCloneWorker(node) {
  const visited = visitEachChild(node, (n) => getSynthesizedDeepCloneWorker(n));
  if (visited === node) {
    const clone = node.kind === SyntaxKind.StringLiteral ? createStringLiteral(node.text, node.tokenFlags) : node.kind === SyntaxKind.NumericLiteral ? createNumericLiteral(node.text, node.tokenFlags) : cloneNode(node);
    return setTextRange(clone, node);
  }
  visited.parent = void 0;
  return visited;
}

// node_modules/typescript/dist/internal/utils.js
var hasOwnProperty = Object.prototype.hasOwnProperty;
function hasProperty(map, key) {
  return hasOwnProperty.call(map, key);
}
function assertNever(member, message = "Illegal value:", stackCrawlMark) {
  const detail = typeof member === "object" && hasProperty(member, "kind") && hasProperty(member, "pos") ? "SyntaxKind: " + formatSyntaxKind(member.kind) : JSON.stringify(member);
  return fail(`${message} ${detail}`, stackCrawlMark || assertNever);
}
function fail(message, stackCrawlMark) {
  debugger;
  const e = new Error(message ? `Debug Failure. ${message}` : "Debug Failure.");
  if (Error.captureStackTrace) {
    Error.captureStackTrace(e, stackCrawlMark || fail);
  }
  throw e;
}

// node_modules/typescript/dist/ast/jsdoc.js
function getJSDocTags(node) {
  return getJSDocCommentsAndTags(node);
}
function getAllJSDocTags(node, predicate) {
  return getJSDocTags(node).filter(predicate);
}
function getAllJSDocTagsOfKind(node, kind) {
  return getJSDocTags(node).filter((doc) => doc.kind === kind);
}
function getTextOfJSDocComment(comment) {
  return typeof comment === "string" ? comment : comment?.map((c) => c.kind === SyntaxKind.JSDocText ? c.text : formatJSDocLink(c)).join("");
}
function isVariableLike(node) {
  switch (node.kind) {
    case SyntaxKind.BindingElement:
    case SyntaxKind.EnumMember:
    case SyntaxKind.Parameter:
    case SyntaxKind.PropertyAssignment:
    case SyntaxKind.PropertyDeclaration:
    case SyntaxKind.PropertySignature:
    case SyntaxKind.ShorthandPropertyAssignment:
    case SyntaxKind.VariableDeclaration:
      return true;
  }
  return false;
}
function getJSDocNodes(node) {
  const jsDoc = node.jsDoc;
  if (!jsDoc || jsDoc.length === 0) {
    return [];
  }
  const result = [];
  for (const j of jsDoc) {
    if (isJSDoc(j)) {
      result.push(j);
    }
  }
  return result;
}
function ownsJSDocTag(hostNode, tag) {
  return !(isJSDocTypeTag(tag) || isJSDocSatisfiesTag(tag)) || !tag.parent || !isJSDoc(tag.parent) || !tag.parent.parent || !isParenthesizedExpression(tag.parent.parent) || tag.parent.parent === hostNode;
}
function filterOwnedJSDocTags(hostNode, comments) {
  const result = [];
  const lastJSDoc = comments[comments.length - 1];
  for (const jsDoc of comments) {
    if (!jsDoc.tags) {
      continue;
    }
    if (jsDoc === lastJSDoc) {
      for (const tag of jsDoc.tags) {
        if (ownsJSDocTag(hostNode, tag)) {
          result.push(tag);
        }
      }
    } else {
      for (const tag of jsDoc.tags) {
        if (isJSDocOverloadTag(tag)) {
          result.push(tag);
        }
      }
    }
  }
  return result;
}
function getJSDocParameterTags(param) {
  const result = [];
  const name = param.name;
  const parentTags = getJSDocTags(param.parent);
  if (name && isIdentifier(name)) {
    for (const tag of parentTags) {
      if (isJSDocParameterTag(tag) && isIdentifier(tag.name) && tag.name.text === name.text) {
        result.push(tag);
      }
    }
  } else if (name) {
    const parameters = param.parent.parameters;
    const i = parameters ? [...parameters].indexOf(param) : -1;
    if (i > -1) {
      const paramTags = parentTags.filter(isJSDocParameterTag);
      if (i < paramTags.length) {
        result.push(paramTags[i]);
      }
    }
  }
  return result;
}
function getJSDocTypeParameterTags(typeParam) {
  const result = [];
  const name = typeParam.name.text;
  for (const tag of getJSDocTags(typeParam.parent)) {
    if (isJSDocTemplateTag(tag) && [...tag.typeParameters].some((tp) => tp.name.text === name)) {
      result.push(tag);
    }
  }
  return result;
}
function getNextJSDocCommentLocation(node) {
  const parent = node.parent;
  if (parent) {
    switch (parent.kind) {
      case SyntaxKind.PropertyAssignment:
      case SyntaxKind.ExportAssignment:
      case SyntaxKind.PropertyDeclaration:
      case SyntaxKind.VariableDeclaration:
      case SyntaxKind.SatisfiesExpression:
      case SyntaxKind.ReturnStatement:
      case SyntaxKind.VariableStatement:
      case SyntaxKind.ExpressionStatement:
        return parent;
      case SyntaxKind.VariableDeclarationList:
        if (parent.declarations[0] === node) {
          return parent;
        }
        break;
    }
  }
  return void 0;
}
function getJSDocCommentsAndTags(hostNode) {
  const result = [];
  if (isVariableLike(hostNode)) {
    const initializer = hostNode.initializer;
    if (initializer) {
      const initJSDoc = getJSDocNodes(initializer);
      if (initJSDoc.length) {
        result.push(...filterOwnedJSDocTags(hostNode, initJSDoc));
      }
    }
  }
  let node = hostNode;
  while (node && node.parent) {
    const jsDocNodes = getJSDocNodes(node);
    if (jsDocNodes.length) {
      result.push(...filterOwnedJSDocTags(hostNode, jsDocNodes));
    }
    if (node.kind === SyntaxKind.Parameter) {
      result.push(...getJSDocParameterTags(node));
      break;
    }
    if (node.kind === SyntaxKind.TypeParameter) {
      result.push(...getJSDocTypeParameterTags(node));
      break;
    }
    node = getNextJSDocCommentLocation(node);
  }
  return result;
}
function formatJSDocLink(link) {
  const kind = link.kind === SyntaxKind.JSDocLink ? "link" : link.kind === SyntaxKind.JSDocLinkCode ? "linkcode" : "linkplain";
  const name = link.name ? entityNameToString(link.name) : "";
  const space = link.name && (link.text === "" || link.text.startsWith("://")) ? "" : " ";
  return `{@${kind} ${name}${space}${link.text}}`;
}
function entityNameToString(name) {
  switch (name.kind) {
    case SyntaxKind.ThisKeyword:
      return "this";
    case SyntaxKind.PrivateIdentifier:
    case SyntaxKind.Identifier:
      return name.getFullWidth() === 0 ? name.text : name.getText();
    case SyntaxKind.QualifiedName:
      return entityNameToString(name.left) + "." + entityNameToString(name.right);
    case SyntaxKind.PropertyAccessExpression:
      if (isIdentifier(name.name) || isPrivateIdentifier(name.name)) {
        return entityNameToString(name.expression) + "." + entityNameToString(name.name);
      } else {
        return assertNever(name.name);
      }
    case SyntaxKind.JsxNamespacedName:
      return entityNameToString(name.namespace) + ":" + entityNameToString(name.name);
    default:
      return assertNever(name);
  }
}

// node_modules/typescript/dist/enums/completionItemKind.js
var CompletionItemKind;
(function(CompletionItemKind2) {
  CompletionItemKind2[CompletionItemKind2["Text"] = 1] = "Text";
  CompletionItemKind2[CompletionItemKind2["Method"] = 2] = "Method";
  CompletionItemKind2[CompletionItemKind2["Function"] = 3] = "Function";
  CompletionItemKind2[CompletionItemKind2["Constructor"] = 4] = "Constructor";
  CompletionItemKind2[CompletionItemKind2["Field"] = 5] = "Field";
  CompletionItemKind2[CompletionItemKind2["Variable"] = 6] = "Variable";
  CompletionItemKind2[CompletionItemKind2["Class"] = 7] = "Class";
  CompletionItemKind2[CompletionItemKind2["Interface"] = 8] = "Interface";
  CompletionItemKind2[CompletionItemKind2["Module"] = 9] = "Module";
  CompletionItemKind2[CompletionItemKind2["Property"] = 10] = "Property";
  CompletionItemKind2[CompletionItemKind2["Unit"] = 11] = "Unit";
  CompletionItemKind2[CompletionItemKind2["Value"] = 12] = "Value";
  CompletionItemKind2[CompletionItemKind2["Enum"] = 13] = "Enum";
  CompletionItemKind2[CompletionItemKind2["Keyword"] = 14] = "Keyword";
  CompletionItemKind2[CompletionItemKind2["Snippet"] = 15] = "Snippet";
  CompletionItemKind2[CompletionItemKind2["Color"] = 16] = "Color";
  CompletionItemKind2[CompletionItemKind2["File"] = 17] = "File";
  CompletionItemKind2[CompletionItemKind2["Reference"] = 18] = "Reference";
  CompletionItemKind2[CompletionItemKind2["Folder"] = 19] = "Folder";
  CompletionItemKind2[CompletionItemKind2["EnumMember"] = 20] = "EnumMember";
  CompletionItemKind2[CompletionItemKind2["Constant"] = 21] = "Constant";
  CompletionItemKind2[CompletionItemKind2["Struct"] = 22] = "Struct";
  CompletionItemKind2[CompletionItemKind2["Event"] = 23] = "Event";
  CompletionItemKind2[CompletionItemKind2["Operator"] = 24] = "Operator";
  CompletionItemKind2[CompletionItemKind2["TypeParameter"] = 25] = "TypeParameter";
})(CompletionItemKind || (CompletionItemKind = {}));

// node_modules/typescript/dist/enums/diagnosticCategory.js
var DiagnosticCategory;
(function(DiagnosticCategory2) {
  DiagnosticCategory2[DiagnosticCategory2["Warning"] = 0] = "Warning";
  DiagnosticCategory2[DiagnosticCategory2["Error"] = 1] = "Error";
  DiagnosticCategory2[DiagnosticCategory2["Suggestion"] = 2] = "Suggestion";
  DiagnosticCategory2[DiagnosticCategory2["Message"] = 3] = "Message";
})(DiagnosticCategory || (DiagnosticCategory = {}));

// node_modules/typescript/dist/enums/elementFlags.js
var ElementFlags;
(function(ElementFlags2) {
  ElementFlags2[ElementFlags2["None"] = 0] = "None";
  ElementFlags2[ElementFlags2["Required"] = 1] = "Required";
  ElementFlags2[ElementFlags2["Optional"] = 2] = "Optional";
  ElementFlags2[ElementFlags2["Rest"] = 4] = "Rest";
  ElementFlags2[ElementFlags2["Variadic"] = 8] = "Variadic";
  ElementFlags2[ElementFlags2["Fixed"] = 3] = "Fixed";
  ElementFlags2[ElementFlags2["Variable"] = 12] = "Variable";
  ElementFlags2[ElementFlags2["NonRequired"] = 14] = "NonRequired";
  ElementFlags2[ElementFlags2["NonRest"] = 11] = "NonRest";
})(ElementFlags || (ElementFlags = {}));

// node_modules/typescript/dist/enums/moduleKind.js
var ModuleKind;
(function(ModuleKind2) {
  ModuleKind2[ModuleKind2["None"] = 0] = "None";
  ModuleKind2[ModuleKind2["CommonJS"] = 1] = "CommonJS";
  ModuleKind2[ModuleKind2["AMD"] = 2] = "AMD";
  ModuleKind2[ModuleKind2["UMD"] = 3] = "UMD";
  ModuleKind2[ModuleKind2["System"] = 4] = "System";
  ModuleKind2[ModuleKind2["ES2015"] = 5] = "ES2015";
  ModuleKind2[ModuleKind2["ES2020"] = 6] = "ES2020";
  ModuleKind2[ModuleKind2["ES2022"] = 7] = "ES2022";
  ModuleKind2[ModuleKind2["ESNext"] = 99] = "ESNext";
  ModuleKind2[ModuleKind2["Node16"] = 100] = "Node16";
  ModuleKind2[ModuleKind2["Node18"] = 101] = "Node18";
  ModuleKind2[ModuleKind2["Node20"] = 102] = "Node20";
  ModuleKind2[ModuleKind2["NodeNext"] = 199] = "NodeNext";
  ModuleKind2[ModuleKind2["Preserve"] = 200] = "Preserve";
})(ModuleKind || (ModuleKind = {}));

// node_modules/typescript/dist/enums/nodeBuilderFlags.js
var NodeBuilderFlags;
(function(NodeBuilderFlags2) {
  NodeBuilderFlags2[NodeBuilderFlags2["None"] = 0] = "None";
  NodeBuilderFlags2[NodeBuilderFlags2["NoTruncation"] = 1] = "NoTruncation";
  NodeBuilderFlags2[NodeBuilderFlags2["WriteArrayAsGenericType"] = 2] = "WriteArrayAsGenericType";
  NodeBuilderFlags2[NodeBuilderFlags2["GenerateNamesForShadowedTypeParams"] = 4] = "GenerateNamesForShadowedTypeParams";
  NodeBuilderFlags2[NodeBuilderFlags2["UseStructuralFallback"] = 8] = "UseStructuralFallback";
  NodeBuilderFlags2[NodeBuilderFlags2["ForbidIndexedAccessSymbolReferences"] = 16] = "ForbidIndexedAccessSymbolReferences";
  NodeBuilderFlags2[NodeBuilderFlags2["WriteTypeArgumentsOfSignature"] = 32] = "WriteTypeArgumentsOfSignature";
  NodeBuilderFlags2[NodeBuilderFlags2["UseFullyQualifiedType"] = 64] = "UseFullyQualifiedType";
  NodeBuilderFlags2[NodeBuilderFlags2["UseOnlyExternalAliasing"] = 128] = "UseOnlyExternalAliasing";
  NodeBuilderFlags2[NodeBuilderFlags2["SuppressAnyReturnType"] = 256] = "SuppressAnyReturnType";
  NodeBuilderFlags2[NodeBuilderFlags2["WriteTypeParametersInQualifiedName"] = 512] = "WriteTypeParametersInQualifiedName";
  NodeBuilderFlags2[NodeBuilderFlags2["MultilineObjectLiterals"] = 1024] = "MultilineObjectLiterals";
  NodeBuilderFlags2[NodeBuilderFlags2["WriteClassExpressionAsTypeLiteral"] = 2048] = "WriteClassExpressionAsTypeLiteral";
  NodeBuilderFlags2[NodeBuilderFlags2["UseTypeOfFunction"] = 4096] = "UseTypeOfFunction";
  NodeBuilderFlags2[NodeBuilderFlags2["OmitParameterModifiers"] = 8192] = "OmitParameterModifiers";
  NodeBuilderFlags2[NodeBuilderFlags2["UseAliasDefinedOutsideCurrentScope"] = 16384] = "UseAliasDefinedOutsideCurrentScope";
  NodeBuilderFlags2[NodeBuilderFlags2["UseSingleQuotesForStringLiteralType"] = 268435456] = "UseSingleQuotesForStringLiteralType";
  NodeBuilderFlags2[NodeBuilderFlags2["NoTypeReduction"] = 536870912] = "NoTypeReduction";
  NodeBuilderFlags2[NodeBuilderFlags2["UseInstantiationExpressions"] = 1073741824] = "UseInstantiationExpressions";
  NodeBuilderFlags2[NodeBuilderFlags2["OmitThisParameter"] = 33554432] = "OmitThisParameter";
  NodeBuilderFlags2[NodeBuilderFlags2["WriteCallStyleSignature"] = 134217728] = "WriteCallStyleSignature";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowThisInObjectLiteral"] = 32768] = "AllowThisInObjectLiteral";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowQualifiedNameInPlaceOfIdentifier"] = 65536] = "AllowQualifiedNameInPlaceOfIdentifier";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowAnonymousIdentifier"] = 131072] = "AllowAnonymousIdentifier";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowEmptyUnionOrIntersection"] = 262144] = "AllowEmptyUnionOrIntersection";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowEmptyTuple"] = 524288] = "AllowEmptyTuple";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowUniqueESSymbolType"] = 1048576] = "AllowUniqueESSymbolType";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowEmptyIndexInfoType"] = 2097152] = "AllowEmptyIndexInfoType";
  NodeBuilderFlags2[NodeBuilderFlags2["AllowNodeModulesRelativePaths"] = 67108864] = "AllowNodeModulesRelativePaths";
  NodeBuilderFlags2[NodeBuilderFlags2["IgnoreErrors"] = 70221824] = "IgnoreErrors";
  NodeBuilderFlags2[NodeBuilderFlags2["InObjectTypeLiteral"] = 4194304] = "InObjectTypeLiteral";
  NodeBuilderFlags2[NodeBuilderFlags2["InTypeAlias"] = 8388608] = "InTypeAlias";
  NodeBuilderFlags2[NodeBuilderFlags2["InInitialEntityName"] = 16777216] = "InInitialEntityName";
})(NodeBuilderFlags || (NodeBuilderFlags = {}));

// node_modules/typescript/dist/enums/objectFlags.js
var ObjectFlags;
(function(ObjectFlags2) {
  ObjectFlags2[ObjectFlags2["None"] = 0] = "None";
  ObjectFlags2[ObjectFlags2["Class"] = 1] = "Class";
  ObjectFlags2[ObjectFlags2["Interface"] = 2] = "Interface";
  ObjectFlags2[ObjectFlags2["Reference"] = 4] = "Reference";
  ObjectFlags2[ObjectFlags2["Tuple"] = 8] = "Tuple";
  ObjectFlags2[ObjectFlags2["Anonymous"] = 16] = "Anonymous";
  ObjectFlags2[ObjectFlags2["Mapped"] = 32] = "Mapped";
  ObjectFlags2[ObjectFlags2["Instantiated"] = 64] = "Instantiated";
  ObjectFlags2[ObjectFlags2["ObjectLiteral"] = 128] = "ObjectLiteral";
  ObjectFlags2[ObjectFlags2["EvolvingArray"] = 256] = "EvolvingArray";
  ObjectFlags2[ObjectFlags2["ObjectLiteralPatternWithComputedProperties"] = 512] = "ObjectLiteralPatternWithComputedProperties";
  ObjectFlags2[ObjectFlags2["ReverseMapped"] = 1024] = "ReverseMapped";
  ObjectFlags2[ObjectFlags2["JsxAttributes"] = 2048] = "JsxAttributes";
  ObjectFlags2[ObjectFlags2["JSLiteral"] = 4096] = "JSLiteral";
  ObjectFlags2[ObjectFlags2["FreshLiteral"] = 8192] = "FreshLiteral";
  ObjectFlags2[ObjectFlags2["ArrayLiteral"] = 16384] = "ArrayLiteral";
  ObjectFlags2[ObjectFlags2["PrimitiveUnion"] = 32768] = "PrimitiveUnion";
  ObjectFlags2[ObjectFlags2["ContainsWideningType"] = 65536] = "ContainsWideningType";
  ObjectFlags2[ObjectFlags2["ContainsObjectOrArrayLiteral"] = 131072] = "ContainsObjectOrArrayLiteral";
  ObjectFlags2[ObjectFlags2["NonInferrableType"] = 262144] = "NonInferrableType";
  ObjectFlags2[ObjectFlags2["CouldContainTypeVariablesComputed"] = 524288] = "CouldContainTypeVariablesComputed";
  ObjectFlags2[ObjectFlags2["CouldContainTypeVariables"] = 1048576] = "CouldContainTypeVariables";
  ObjectFlags2[ObjectFlags2["MembersResolved"] = 2097152] = "MembersResolved";
  ObjectFlags2[ObjectFlags2["ClassOrInterface"] = 3] = "ClassOrInterface";
  ObjectFlags2[ObjectFlags2["RequiresWidening"] = 196608] = "RequiresWidening";
  ObjectFlags2[ObjectFlags2["PropagatingFlags"] = 458752] = "PropagatingFlags";
  ObjectFlags2[ObjectFlags2["InstantiatedMapped"] = 96] = "InstantiatedMapped";
  ObjectFlags2[ObjectFlags2["InstantiationExpressionType"] = 16777216] = "InstantiationExpressionType";
  ObjectFlags2[ObjectFlags2["SingleSignatureType"] = 33554432] = "SingleSignatureType";
  ObjectFlags2[ObjectFlags2["ObjectTypeKindMask"] = 50332991] = "ObjectTypeKindMask";
  ObjectFlags2[ObjectFlags2["ContainsSpread"] = 4194304] = "ContainsSpread";
  ObjectFlags2[ObjectFlags2["ObjectRestType"] = 8388608] = "ObjectRestType";
  ObjectFlags2[ObjectFlags2["IsClassInstanceClone"] = 67108864] = "IsClassInstanceClone";
  ObjectFlags2[ObjectFlags2["IdenticalBaseTypeCalculated"] = 134217728] = "IdenticalBaseTypeCalculated";
  ObjectFlags2[ObjectFlags2["IdenticalBaseTypeExists"] = 268435456] = "IdenticalBaseTypeExists";
  ObjectFlags2[ObjectFlags2["UnresolvedMembers"] = 536870912] = "UnresolvedMembers";
  ObjectFlags2[ObjectFlags2["FromTypeNode"] = 1073741824] = "FromTypeNode";
  ObjectFlags2[ObjectFlags2["IsGenericTypeComputed"] = 4194304] = "IsGenericTypeComputed";
  ObjectFlags2[ObjectFlags2["IsGenericObjectType"] = 8388608] = "IsGenericObjectType";
  ObjectFlags2[ObjectFlags2["IsGenericIndexType"] = 16777216] = "IsGenericIndexType";
  ObjectFlags2[ObjectFlags2["IsGenericType"] = 25165824] = "IsGenericType";
  ObjectFlags2[ObjectFlags2["ContainsIntersections"] = 33554432] = "ContainsIntersections";
  ObjectFlags2[ObjectFlags2["IsUnknownLikeUnionComputed"] = 67108864] = "IsUnknownLikeUnionComputed";
  ObjectFlags2[ObjectFlags2["IsUnknownLikeUnion"] = 134217728] = "IsUnknownLikeUnion";
  ObjectFlags2[ObjectFlags2["IsNeverIntersectionComputed"] = 33554432] = "IsNeverIntersectionComputed";
  ObjectFlags2[ObjectFlags2["IsNeverIntersection"] = 67108864] = "IsNeverIntersection";
  ObjectFlags2[ObjectFlags2["IsConstrainedTypeVariable"] = 134217728] = "IsConstrainedTypeVariable";
})(ObjectFlags || (ObjectFlags = {}));

// node_modules/typescript/dist/enums/signatureFlags.js
var SignatureFlags;
(function(SignatureFlags2) {
  SignatureFlags2[SignatureFlags2["None"] = 0] = "None";
  SignatureFlags2[SignatureFlags2["HasRestParameter"] = 1] = "HasRestParameter";
  SignatureFlags2[SignatureFlags2["HasLiteralTypes"] = 2] = "HasLiteralTypes";
  SignatureFlags2[SignatureFlags2["Construct"] = 4] = "Construct";
  SignatureFlags2[SignatureFlags2["Abstract"] = 8] = "Abstract";
  SignatureFlags2[SignatureFlags2["IsInnerCallChain"] = 16] = "IsInnerCallChain";
  SignatureFlags2[SignatureFlags2["IsOuterCallChain"] = 32] = "IsOuterCallChain";
  SignatureFlags2[SignatureFlags2["IsUntypedSignatureInJSFile"] = 64] = "IsUntypedSignatureInJSFile";
  SignatureFlags2[SignatureFlags2["IsNonInferrable"] = 128] = "IsNonInferrable";
  SignatureFlags2[SignatureFlags2["IsSignatureCandidateForOverloadFailure"] = 256] = "IsSignatureCandidateForOverloadFailure";
  SignatureFlags2[SignatureFlags2["PropagatingFlags"] = 335] = "PropagatingFlags";
  SignatureFlags2[SignatureFlags2["CallChainFlags"] = 48] = "CallChainFlags";
})(SignatureFlags || (SignatureFlags = {}));

// node_modules/typescript/dist/enums/signatureKind.js
var SignatureKind;
(function(SignatureKind2) {
  SignatureKind2[SignatureKind2["Call"] = 0] = "Call";
  SignatureKind2[SignatureKind2["Construct"] = 1] = "Construct";
})(SignatureKind || (SignatureKind = {}));

// node_modules/typescript/dist/enums/symbolFlags.js
var SymbolFlags;
(function(SymbolFlags2) {
  SymbolFlags2[SymbolFlags2["None"] = 0] = "None";
  SymbolFlags2[SymbolFlags2["FunctionScopedVariable"] = 1] = "FunctionScopedVariable";
  SymbolFlags2[SymbolFlags2["BlockScopedVariable"] = 2] = "BlockScopedVariable";
  SymbolFlags2[SymbolFlags2["Property"] = 4] = "Property";
  SymbolFlags2[SymbolFlags2["EnumMember"] = 8] = "EnumMember";
  SymbolFlags2[SymbolFlags2["Function"] = 16] = "Function";
  SymbolFlags2[SymbolFlags2["Class"] = 32] = "Class";
  SymbolFlags2[SymbolFlags2["Interface"] = 64] = "Interface";
  SymbolFlags2[SymbolFlags2["ConstEnum"] = 128] = "ConstEnum";
  SymbolFlags2[SymbolFlags2["RegularEnum"] = 256] = "RegularEnum";
  SymbolFlags2[SymbolFlags2["ValueModule"] = 512] = "ValueModule";
  SymbolFlags2[SymbolFlags2["NamespaceModule"] = 1024] = "NamespaceModule";
  SymbolFlags2[SymbolFlags2["TypeLiteral"] = 2048] = "TypeLiteral";
  SymbolFlags2[SymbolFlags2["ObjectLiteral"] = 4096] = "ObjectLiteral";
  SymbolFlags2[SymbolFlags2["Method"] = 8192] = "Method";
  SymbolFlags2[SymbolFlags2["Constructor"] = 16384] = "Constructor";
  SymbolFlags2[SymbolFlags2["GetAccessor"] = 32768] = "GetAccessor";
  SymbolFlags2[SymbolFlags2["SetAccessor"] = 65536] = "SetAccessor";
  SymbolFlags2[SymbolFlags2["Signature"] = 131072] = "Signature";
  SymbolFlags2[SymbolFlags2["TypeParameter"] = 262144] = "TypeParameter";
  SymbolFlags2[SymbolFlags2["TypeAlias"] = 524288] = "TypeAlias";
  SymbolFlags2[SymbolFlags2["ExportValue"] = 1048576] = "ExportValue";
  SymbolFlags2[SymbolFlags2["Alias"] = 2097152] = "Alias";
  SymbolFlags2[SymbolFlags2["Prototype"] = 4194304] = "Prototype";
  SymbolFlags2[SymbolFlags2["ExportStar"] = 8388608] = "ExportStar";
  SymbolFlags2[SymbolFlags2["Optional"] = 16777216] = "Optional";
  SymbolFlags2[SymbolFlags2["Transient"] = 33554432] = "Transient";
  SymbolFlags2[SymbolFlags2["Assignment"] = 67108864] = "Assignment";
  SymbolFlags2[SymbolFlags2["ModuleExports"] = 134217728] = "ModuleExports";
  SymbolFlags2[SymbolFlags2["ConstEnumOnlyModule"] = 268435456] = "ConstEnumOnlyModule";
  SymbolFlags2[SymbolFlags2["ReplaceableByMethod"] = 536870912] = "ReplaceableByMethod";
  SymbolFlags2[SymbolFlags2["GlobalLookup"] = 1073741824] = "GlobalLookup";
  SymbolFlags2[SymbolFlags2["All"] = 536870912] = "All";
  SymbolFlags2[SymbolFlags2["Enum"] = 384] = "Enum";
  SymbolFlags2[SymbolFlags2["Variable"] = 3] = "Variable";
  SymbolFlags2[SymbolFlags2["Value"] = 111551] = "Value";
  SymbolFlags2[SymbolFlags2["Type"] = 788968] = "Type";
  SymbolFlags2[SymbolFlags2["Namespace"] = 1920] = "Namespace";
  SymbolFlags2[SymbolFlags2["Module"] = 1536] = "Module";
  SymbolFlags2[SymbolFlags2["Accessor"] = 98304] = "Accessor";
  SymbolFlags2[SymbolFlags2["FunctionScopedVariableExcludes"] = 111550] = "FunctionScopedVariableExcludes";
  SymbolFlags2[SymbolFlags2["BlockScopedVariableExcludes"] = 111551] = "BlockScopedVariableExcludes";
  SymbolFlags2[SymbolFlags2["ParameterExcludes"] = 111551] = "ParameterExcludes";
  SymbolFlags2[SymbolFlags2["PropertyExcludes"] = 13243] = "PropertyExcludes";
  SymbolFlags2[SymbolFlags2["EnumMemberExcludes"] = 900095] = "EnumMemberExcludes";
  SymbolFlags2[SymbolFlags2["FunctionExcludes"] = 110991] = "FunctionExcludes";
  SymbolFlags2[SymbolFlags2["ClassExcludes"] = 899503] = "ClassExcludes";
  SymbolFlags2[SymbolFlags2["InterfaceExcludes"] = 788872] = "InterfaceExcludes";
  SymbolFlags2[SymbolFlags2["RegularEnumExcludes"] = 899327] = "RegularEnumExcludes";
  SymbolFlags2[SymbolFlags2["ConstEnumExcludes"] = 899967] = "ConstEnumExcludes";
  SymbolFlags2[SymbolFlags2["ValueModuleExcludes"] = 110735] = "ValueModuleExcludes";
  SymbolFlags2[SymbolFlags2["NamespaceModuleExcludes"] = 0] = "NamespaceModuleExcludes";
  SymbolFlags2[SymbolFlags2["MethodExcludes"] = 103359] = "MethodExcludes";
  SymbolFlags2[SymbolFlags2["GetAccessorExcludes"] = 46011] = "GetAccessorExcludes";
  SymbolFlags2[SymbolFlags2["SetAccessorExcludes"] = 78779] = "SetAccessorExcludes";
  SymbolFlags2[SymbolFlags2["AccessorExcludes"] = 111547] = "AccessorExcludes";
  SymbolFlags2[SymbolFlags2["TypeParameterExcludes"] = 526824] = "TypeParameterExcludes";
  SymbolFlags2[SymbolFlags2["TypeAliasExcludes"] = 788968] = "TypeAliasExcludes";
  SymbolFlags2[SymbolFlags2["AliasExcludes"] = 2097152] = "AliasExcludes";
  SymbolFlags2[SymbolFlags2["ModuleMember"] = 2623475] = "ModuleMember";
  SymbolFlags2[SymbolFlags2["ExportHasLocal"] = 944] = "ExportHasLocal";
  SymbolFlags2[SymbolFlags2["BlockScoped"] = 418] = "BlockScoped";
  SymbolFlags2[SymbolFlags2["PropertyOrAccessor"] = 98308] = "PropertyOrAccessor";
  SymbolFlags2[SymbolFlags2["ClassMember"] = 106500] = "ClassMember";
  SymbolFlags2[SymbolFlags2["ExportSupportsDefaultModifier"] = 112] = "ExportSupportsDefaultModifier";
  SymbolFlags2[SymbolFlags2["ExportDoesNotSupportDefaultModifier"] = -113] = "ExportDoesNotSupportDefaultModifier";
  SymbolFlags2[SymbolFlags2["Classifiable"] = 2885600] = "Classifiable";
  SymbolFlags2[SymbolFlags2["LateBindingContainer"] = 6256] = "LateBindingContainer";
})(SymbolFlags || (SymbolFlags = {}));

// node_modules/typescript/dist/enums/typeFlags.js
var TypeFlags;
(function(TypeFlags2) {
  TypeFlags2[TypeFlags2["None"] = 0] = "None";
  TypeFlags2[TypeFlags2["Any"] = 1] = "Any";
  TypeFlags2[TypeFlags2["Unknown"] = 2] = "Unknown";
  TypeFlags2[TypeFlags2["Undefined"] = 4] = "Undefined";
  TypeFlags2[TypeFlags2["Null"] = 8] = "Null";
  TypeFlags2[TypeFlags2["Void"] = 16] = "Void";
  TypeFlags2[TypeFlags2["String"] = 32] = "String";
  TypeFlags2[TypeFlags2["Number"] = 64] = "Number";
  TypeFlags2[TypeFlags2["BigInt"] = 128] = "BigInt";
  TypeFlags2[TypeFlags2["Boolean"] = 256] = "Boolean";
  TypeFlags2[TypeFlags2["ESSymbol"] = 512] = "ESSymbol";
  TypeFlags2[TypeFlags2["StringLiteral"] = 1024] = "StringLiteral";
  TypeFlags2[TypeFlags2["NumberLiteral"] = 2048] = "NumberLiteral";
  TypeFlags2[TypeFlags2["BigIntLiteral"] = 4096] = "BigIntLiteral";
  TypeFlags2[TypeFlags2["BooleanLiteral"] = 8192] = "BooleanLiteral";
  TypeFlags2[TypeFlags2["UniqueESSymbol"] = 16384] = "UniqueESSymbol";
  TypeFlags2[TypeFlags2["EnumLiteral"] = 32768] = "EnumLiteral";
  TypeFlags2[TypeFlags2["Enum"] = 65536] = "Enum";
  TypeFlags2[TypeFlags2["NonPrimitive"] = 131072] = "NonPrimitive";
  TypeFlags2[TypeFlags2["Never"] = 262144] = "Never";
  TypeFlags2[TypeFlags2["TypeParameter"] = 524288] = "TypeParameter";
  TypeFlags2[TypeFlags2["Object"] = 1048576] = "Object";
  TypeFlags2[TypeFlags2["Index"] = 2097152] = "Index";
  TypeFlags2[TypeFlags2["TemplateLiteral"] = 4194304] = "TemplateLiteral";
  TypeFlags2[TypeFlags2["StringMapping"] = 8388608] = "StringMapping";
  TypeFlags2[TypeFlags2["Substitution"] = 16777216] = "Substitution";
  TypeFlags2[TypeFlags2["IndexedAccess"] = 33554432] = "IndexedAccess";
  TypeFlags2[TypeFlags2["Conditional"] = 67108864] = "Conditional";
  TypeFlags2[TypeFlags2["Union"] = 134217728] = "Union";
  TypeFlags2[TypeFlags2["Intersection"] = 268435456] = "Intersection";
  TypeFlags2[TypeFlags2["Reserved1"] = 536870912] = "Reserved1";
  TypeFlags2[TypeFlags2["Reserved2"] = 1073741824] = "Reserved2";
  TypeFlags2[TypeFlags2["Reserved3"] = -2147483648] = "Reserved3";
  TypeFlags2[TypeFlags2["AnyOrUnknown"] = 3] = "AnyOrUnknown";
  TypeFlags2[TypeFlags2["Nullable"] = 12] = "Nullable";
  TypeFlags2[TypeFlags2["Literal"] = 15360] = "Literal";
  TypeFlags2[TypeFlags2["Unit"] = 97292] = "Unit";
  TypeFlags2[TypeFlags2["Freshable"] = 80896] = "Freshable";
  TypeFlags2[TypeFlags2["StringOrNumberLiteral"] = 3072] = "StringOrNumberLiteral";
  TypeFlags2[TypeFlags2["StringOrNumberLiteralOrUnique"] = 19456] = "StringOrNumberLiteralOrUnique";
  TypeFlags2[TypeFlags2["DefinitelyFalsy"] = 15388] = "DefinitelyFalsy";
  TypeFlags2[TypeFlags2["PossiblyFalsy"] = 15868] = "PossiblyFalsy";
  TypeFlags2[TypeFlags2["Intrinsic"] = 393983] = "Intrinsic";
  TypeFlags2[TypeFlags2["StringLike"] = 12583968] = "StringLike";
  TypeFlags2[TypeFlags2["NumberLike"] = 67648] = "NumberLike";
  TypeFlags2[TypeFlags2["BigIntLike"] = 4224] = "BigIntLike";
  TypeFlags2[TypeFlags2["BooleanLike"] = 8448] = "BooleanLike";
  TypeFlags2[TypeFlags2["EnumLike"] = 98304] = "EnumLike";
  TypeFlags2[TypeFlags2["ESSymbolLike"] = 16896] = "ESSymbolLike";
  TypeFlags2[TypeFlags2["VoidLike"] = 20] = "VoidLike";
  TypeFlags2[TypeFlags2["Primitive"] = 12713980] = "Primitive";
  TypeFlags2[TypeFlags2["DefinitelyNonNullable"] = 13893600] = "DefinitelyNonNullable";
  TypeFlags2[TypeFlags2["DisjointDomains"] = 12812284] = "DisjointDomains";
  TypeFlags2[TypeFlags2["UnionOrIntersection"] = 402653184] = "UnionOrIntersection";
  TypeFlags2[TypeFlags2["StructuredType"] = 403701760] = "StructuredType";
  TypeFlags2[TypeFlags2["TypeVariable"] = 34078720] = "TypeVariable";
  TypeFlags2[TypeFlags2["InstantiableNonPrimitive"] = 117964800] = "InstantiableNonPrimitive";
  TypeFlags2[TypeFlags2["InstantiablePrimitive"] = 14680064] = "InstantiablePrimitive";
  TypeFlags2[TypeFlags2["Instantiable"] = 132644864] = "Instantiable";
  TypeFlags2[TypeFlags2["StructuredOrInstantiable"] = 536346624] = "StructuredOrInstantiable";
  TypeFlags2[TypeFlags2["ObjectFlagsType"] = 403963917] = "ObjectFlagsType";
  TypeFlags2[TypeFlags2["Simplifiable"] = 102760448] = "Simplifiable";
  TypeFlags2[TypeFlags2["Singleton"] = 394239] = "Singleton";
  TypeFlags2[TypeFlags2["Narrowable"] = 536575971] = "Narrowable";
  TypeFlags2[TypeFlags2["IncludesMask"] = 416808959] = "IncludesMask";
  TypeFlags2[TypeFlags2["IncludesMissingType"] = 524288] = "IncludesMissingType";
  TypeFlags2[TypeFlags2["IncludesNonWideningType"] = 2097152] = "IncludesNonWideningType";
  TypeFlags2[TypeFlags2["IncludesWildcard"] = 33554432] = "IncludesWildcard";
  TypeFlags2[TypeFlags2["IncludesEmptyObject"] = 67108864] = "IncludesEmptyObject";
  TypeFlags2[TypeFlags2["IncludesInstantiable"] = 16777216] = "IncludesInstantiable";
  TypeFlags2[TypeFlags2["IncludesConstrainedTypeVariable"] = 536870912] = "IncludesConstrainedTypeVariable";
  TypeFlags2[TypeFlags2["IncludesError"] = 1073741824] = "IncludesError";
  TypeFlags2[TypeFlags2["NotPrimitiveUnion"] = 286523411] = "NotPrimitiveUnion";
})(TypeFlags || (TypeFlags = {}));

// node_modules/typescript/dist/enums/typePredicateKind.js
var TypePredicateKind;
(function(TypePredicateKind2) {
  TypePredicateKind2[TypePredicateKind2["This"] = 0] = "This";
  TypePredicateKind2[TypePredicateKind2["Identifier"] = 1] = "Identifier";
  TypePredicateKind2[TypePredicateKind2["AssertsThis"] = 2] = "AssertsThis";
  TypePredicateKind2[TypePredicateKind2["AssertsIdentifier"] = 3] = "AssertsIdentifier";
})(TypePredicateKind || (TypePredicateKind = {}));

// node_modules/typescript/dist/api/node/protocol.generated.js
var childProperties = {
  [SyntaxKind.QualifiedName]: ["left", "right"],
  [SyntaxKind.ComputedPropertyName]: ["expression"],
  [SyntaxKind.Decorator]: ["expression"],
  [SyntaxKind.IfStatement]: ["expression", "thenStatement", "elseStatement"],
  [SyntaxKind.DoStatement]: ["statement", "expression"],
  [SyntaxKind.WhileStatement]: ["expression", "statement"],
  [SyntaxKind.ForStatement]: ["initializer", "condition", "incrementor", "statement"],
  [SyntaxKind.ForInStatement]: ["awaitModifier", "initializer", "expression", "statement"],
  [SyntaxKind.ForOfStatement]: ["awaitModifier", "initializer", "expression", "statement"],
  [SyntaxKind.BreakStatement]: ["label"],
  [SyntaxKind.ContinueStatement]: ["label"],
  [SyntaxKind.ReturnStatement]: ["expression"],
  [SyntaxKind.WithStatement]: ["expression", "statement"],
  [SyntaxKind.SwitchStatement]: ["expression", "caseBlock"],
  [SyntaxKind.CaseBlock]: ["clauses"],
  [SyntaxKind.CaseClause]: ["expression", "statements"],
  [SyntaxKind.DefaultClause]: ["expression", "statements"],
  [SyntaxKind.ThrowStatement]: ["expression"],
  [SyntaxKind.TryStatement]: ["tryBlock", "catchClause", "finallyBlock"],
  [SyntaxKind.CatchClause]: ["variableDeclaration", "block"],
  [SyntaxKind.LabeledStatement]: ["label", "statement"],
  [SyntaxKind.ExpressionStatement]: ["expression"],
  [SyntaxKind.Block]: ["statements"],
  [SyntaxKind.VariableStatement]: ["modifiers", "declarationList"],
  [SyntaxKind.VariableDeclaration]: ["name", "exclamationToken", "type", "initializer"],
  [SyntaxKind.VariableDeclarationList]: ["declarations"],
  [SyntaxKind.ObjectBindingPattern]: ["elements"],
  [SyntaxKind.ArrayBindingPattern]: ["elements"],
  [SyntaxKind.Parameter]: ["modifiers", "dotDotDotToken", "name", "questionToken", "type", "initializer"],
  [SyntaxKind.BindingElement]: ["dotDotDotToken", "propertyName", "name", "initializer"],
  [SyntaxKind.MissingDeclaration]: ["modifiers"],
  [SyntaxKind.FunctionDeclaration]: ["modifiers", "asteriskToken", "name", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.ClassDeclaration]: ["modifiers", "name", "typeParameters", "heritageClauses", "members"],
  [SyntaxKind.ClassExpression]: ["modifiers", "name", "typeParameters", "heritageClauses", "members"],
  [SyntaxKind.HeritageClause]: ["types"],
  [SyntaxKind.InterfaceDeclaration]: ["modifiers", "name", "typeParameters", "heritageClauses", "members"],
  [SyntaxKind.TypeAliasDeclaration]: ["modifiers", "name", "typeParameters", "type"],
  [SyntaxKind.JSTypeAliasDeclaration]: ["modifiers", "name", "typeParameters", "type"],
  [SyntaxKind.EnumMember]: ["name", "initializer"],
  [SyntaxKind.EnumDeclaration]: ["modifiers", "name", "members"],
  [SyntaxKind.ModuleBlock]: ["statements"],
  [SyntaxKind.ImportDeclaration]: ["modifiers", "importClause", "moduleSpecifier", "attributes"],
  [SyntaxKind.JSImportDeclaration]: ["modifiers", "importClause", "moduleSpecifier", "attributes"],
  [SyntaxKind.ExternalModuleReference]: ["expression"],
  [SyntaxKind.NamespaceImport]: ["name"],
  [SyntaxKind.NamedImports]: ["elements"],
  [SyntaxKind.ExportAssignment]: ["modifiers", "type", "expression"],
  [SyntaxKind.NamespaceExportDeclaration]: ["modifiers", "name"],
  [SyntaxKind.NamespaceExport]: ["name"],
  [SyntaxKind.NamedExports]: ["elements"],
  [SyntaxKind.ExportSpecifier]: ["propertyName", "name"],
  [SyntaxKind.CallSignature]: ["typeParameters", "parameters", "type"],
  [SyntaxKind.ConstructSignature]: ["typeParameters", "parameters", "type"],
  [SyntaxKind.Constructor]: ["modifiers", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.GetAccessor]: ["modifiers", "name", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.SetAccessor]: ["modifiers", "name", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.IndexSignature]: ["modifiers", "parameters", "type"],
  [SyntaxKind.MethodSignature]: ["modifiers", "name", "postfixToken", "typeParameters", "parameters", "type"],
  [SyntaxKind.MethodDeclaration]: ["modifiers", "asteriskToken", "name", "postfixToken", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.PropertySignature]: ["modifiers", "name", "postfixToken", "type", "initializer"],
  [SyntaxKind.PropertyDeclaration]: ["modifiers", "name", "postfixToken", "type", "initializer"],
  [SyntaxKind.ClassStaticBlockDeclaration]: ["modifiers", "body"],
  [SyntaxKind.BinaryExpression]: ["modifiers", "left", "type", "operatorToken", "right"],
  [SyntaxKind.PrefixUnaryExpression]: ["operand"],
  [SyntaxKind.PostfixUnaryExpression]: ["operand"],
  [SyntaxKind.YieldExpression]: ["asteriskToken", "expression"],
  [SyntaxKind.ArrowFunction]: ["modifiers", "typeParameters", "parameters", "type", "equalsGreaterThanToken", "body"],
  [SyntaxKind.FunctionExpression]: ["modifiers", "asteriskToken", "name", "typeParameters", "parameters", "type", "body"],
  [SyntaxKind.AsExpression]: ["expression", "type"],
  [SyntaxKind.SatisfiesExpression]: ["expression", "type"],
  [SyntaxKind.ConditionalExpression]: ["condition", "questionToken", "whenTrue", "colonToken", "whenFalse"],
  [SyntaxKind.PropertyAccessExpression]: ["expression", "questionDotToken", "name"],
  [SyntaxKind.ElementAccessExpression]: ["expression", "questionDotToken", "argumentExpression"],
  [SyntaxKind.CallExpression]: ["expression", "questionDotToken", "typeArguments", "arguments"],
  [SyntaxKind.NewExpression]: ["expression", "typeArguments", "arguments"],
  [SyntaxKind.MetaProperty]: ["name"],
  [SyntaxKind.NonNullExpression]: ["expression"],
  [SyntaxKind.SpreadElement]: ["expression"],
  [SyntaxKind.TemplateExpression]: ["head", "templateSpans"],
  [SyntaxKind.TemplateSpan]: ["expression", "literal"],
  [SyntaxKind.TaggedTemplateExpression]: ["tag", "questionDotToken", "typeArguments", "template"],
  [SyntaxKind.ParenthesizedExpression]: ["expression"],
  [SyntaxKind.ArrayLiteralExpression]: ["elements"],
  [SyntaxKind.ObjectLiteralExpression]: ["properties"],
  [SyntaxKind.SpreadAssignment]: ["expression"],
  [SyntaxKind.PropertyAssignment]: ["modifiers", "name", "postfixToken", "type", "initializer"],
  [SyntaxKind.ShorthandPropertyAssignment]: ["modifiers", "name", "postfixToken", "type", "equalsToken", "objectAssignmentInitializer"],
  [SyntaxKind.DeleteExpression]: ["expression"],
  [SyntaxKind.TypeOfExpression]: ["expression"],
  [SyntaxKind.VoidExpression]: ["expression"],
  [SyntaxKind.AwaitExpression]: ["expression"],
  [SyntaxKind.TypeAssertionExpression]: ["type", "expression"],
  [SyntaxKind.UnionType]: ["types"],
  [SyntaxKind.IntersectionType]: ["types"],
  [SyntaxKind.ConditionalType]: ["checkType", "extendsType", "trueType", "falseType"],
  [SyntaxKind.TypeOperator]: ["type"],
  [SyntaxKind.InferType]: ["typeParameter"],
  [SyntaxKind.ArrayType]: ["elementType"],
  [SyntaxKind.IndexedAccessType]: ["objectType", "indexType"],
  [SyntaxKind.TypeReference]: ["typeName", "typeArguments"],
  [SyntaxKind.ExpressionWithTypeArguments]: ["expression", "typeArguments"],
  [SyntaxKind.LiteralType]: ["literal"],
  [SyntaxKind.TypePredicate]: ["assertsModifier", "parameterName", "type"],
  [SyntaxKind.ImportAttribute]: ["name", "value"],
  [SyntaxKind.ImportAttributes]: ["attributes"],
  [SyntaxKind.TypeQuery]: ["exprName", "typeArguments"],
  [SyntaxKind.MappedType]: ["readonlyToken", "typeParameter", "nameType", "questionToken", "type", "members"],
  [SyntaxKind.TypeLiteral]: ["members"],
  [SyntaxKind.TupleType]: ["elements"],
  [SyntaxKind.NamedTupleMember]: ["dotDotDotToken", "name", "questionToken", "type"],
  [SyntaxKind.OptionalType]: ["type"],
  [SyntaxKind.RestType]: ["type"],
  [SyntaxKind.ParenthesizedType]: ["type"],
  [SyntaxKind.FunctionType]: ["typeParameters", "parameters", "type"],
  [SyntaxKind.ConstructorType]: ["modifiers", "typeParameters", "parameters", "type"],
  [SyntaxKind.TemplateLiteralType]: ["head", "templateSpans"],
  [SyntaxKind.TemplateLiteralTypeSpan]: ["type", "literal"],
  [SyntaxKind.SyntheticExpression]: ["tupleNameSource"],
  [SyntaxKind.PartiallyEmittedExpression]: ["expression"],
  [SyntaxKind.JsxElement]: ["openingElement", "children", "closingElement"],
  [SyntaxKind.JsxAttributes]: ["properties"],
  [SyntaxKind.JsxNamespacedName]: ["namespace", "name"],
  [SyntaxKind.JsxOpeningElement]: ["tagName", "typeArguments", "attributes"],
  [SyntaxKind.JsxSelfClosingElement]: ["tagName", "typeArguments", "attributes"],
  [SyntaxKind.JsxFragment]: ["openingFragment", "children", "closingFragment"],
  [SyntaxKind.JsxAttribute]: ["name", "initializer"],
  [SyntaxKind.JsxSpreadAttribute]: ["expression"],
  [SyntaxKind.JsxClosingElement]: ["tagName"],
  [SyntaxKind.JsxExpression]: ["dotDotDotToken", "expression"],
  [SyntaxKind.SyntaxList]: ["children"],
  [SyntaxKind.JSDoc]: ["comment", "tags"],
  [SyntaxKind.JSDocTypeExpression]: ["type"],
  [SyntaxKind.JSDocNonNullableType]: ["type"],
  [SyntaxKind.JSDocNullableType]: ["type"],
  [SyntaxKind.JSDocVariadicType]: ["type"],
  [SyntaxKind.JSDocOptionalType]: ["type"],
  [SyntaxKind.JSDocTypeTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocUnknownTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocTemplateTag]: ["tagName", "constraint", "typeParameters", "comment"],
  [SyntaxKind.JSDocReturnTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocPublicTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocPrivateTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocProtectedTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocReadonlyTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocOverrideTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocDeprecatedTag]: ["tagName", "comment"],
  [SyntaxKind.JSDocSeeTag]: ["tagName", "nameExpression", "comment"],
  [SyntaxKind.JSDocImplementsTag]: ["tagName", "className", "comment"],
  [SyntaxKind.JSDocAugmentsTag]: ["tagName", "className", "comment"],
  [SyntaxKind.JSDocSatisfiesTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocThrowsTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocThisTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocImportTag]: ["tagName", "importClause", "moduleSpecifier", "attributes", "comment"],
  [SyntaxKind.JSDocCallbackTag]: ["tagName", "typeExpression", "name", "comment"],
  [SyntaxKind.JSDocOverloadTag]: ["tagName", "typeExpression", "comment"],
  [SyntaxKind.JSDocTypedefTag]: ["tagName", "typeExpression", "name", "comment"],
  [SyntaxKind.JSDocSignature]: ["typeParameters", "parameters", "type"],
  [SyntaxKind.JSDocNameReference]: ["name"],
  [SyntaxKind.SourceFile]: ["statements", "endOfFileToken"],
  [SyntaxKind.ModuleDeclaration]: ["modifiers", "name", "body"],
  [SyntaxKind.ImportEqualsDeclaration]: ["modifiers", "name", "moduleReference"],
  [SyntaxKind.ExportDeclaration]: ["modifiers", "exportClause", "moduleSpecifier", "attributes"],
  [SyntaxKind.ImportType]: ["argument", "attributes", "qualifier", "typeArguments"],
  [SyntaxKind.ImportClause]: ["name", "namedBindings"],
  [SyntaxKind.ImportSpecifier]: ["propertyName", "name"],
  [SyntaxKind.JSDocLink]: ["name"],
  [SyntaxKind.JSDocLinkPlain]: ["name"],
  [SyntaxKind.JSDocLinkCode]: ["name"],
  [SyntaxKind.TypeParameter]: ["modifiers", "name", "constraint", "expression", "defaultType"],
  [SyntaxKind.SyntheticReferenceExpression]: ["expression", "thisArg"],
  [SyntaxKind.JSDocTypeLiteral]: ["jsdocPropertyTags"],
  [SyntaxKind.JSDocParameterTag]: ["tagName", "name", "typeExpression", "comment"],
  [SyntaxKind.JSDocPropertyTag]: ["tagName", "name", "typeExpression", "comment"]
};
var singleChildNodePropertyNames = {
  [SyntaxKind.ComputedPropertyName]: "expression",
  [SyntaxKind.Decorator]: "expression",
  [SyntaxKind.BreakStatement]: "label",
  [SyntaxKind.ContinueStatement]: "label",
  [SyntaxKind.ReturnStatement]: "expression",
  [SyntaxKind.CaseBlock]: "clauses",
  [SyntaxKind.ThrowStatement]: "expression",
  [SyntaxKind.ExpressionStatement]: "expression",
  [SyntaxKind.Block]: "statements",
  [SyntaxKind.VariableDeclarationList]: "declarations",
  [SyntaxKind.ObjectBindingPattern]: "elements",
  [SyntaxKind.ArrayBindingPattern]: "elements",
  [SyntaxKind.MissingDeclaration]: "modifiers",
  [SyntaxKind.HeritageClause]: "types",
  [SyntaxKind.ModuleBlock]: "statements",
  [SyntaxKind.ExternalModuleReference]: "expression",
  [SyntaxKind.NamespaceImport]: "name",
  [SyntaxKind.NamedImports]: "elements",
  [SyntaxKind.NamespaceExport]: "name",
  [SyntaxKind.NamedExports]: "elements",
  [SyntaxKind.PrefixUnaryExpression]: "operand",
  [SyntaxKind.PostfixUnaryExpression]: "operand",
  [SyntaxKind.MetaProperty]: "name",
  [SyntaxKind.NonNullExpression]: "expression",
  [SyntaxKind.SpreadElement]: "expression",
  [SyntaxKind.ParenthesizedExpression]: "expression",
  [SyntaxKind.ArrayLiteralExpression]: "elements",
  [SyntaxKind.ObjectLiteralExpression]: "properties",
  [SyntaxKind.SpreadAssignment]: "expression",
  [SyntaxKind.DeleteExpression]: "expression",
  [SyntaxKind.TypeOfExpression]: "expression",
  [SyntaxKind.VoidExpression]: "expression",
  [SyntaxKind.AwaitExpression]: "expression",
  [SyntaxKind.UnionType]: "types",
  [SyntaxKind.IntersectionType]: "types",
  [SyntaxKind.TypeOperator]: "type",
  [SyntaxKind.InferType]: "typeParameter",
  [SyntaxKind.ArrayType]: "elementType",
  [SyntaxKind.LiteralType]: "literal",
  [SyntaxKind.ImportAttributes]: "attributes",
  [SyntaxKind.TypeLiteral]: "members",
  [SyntaxKind.TupleType]: "elements",
  [SyntaxKind.OptionalType]: "type",
  [SyntaxKind.RestType]: "type",
  [SyntaxKind.ParenthesizedType]: "type",
  [SyntaxKind.SyntheticExpression]: "tupleNameSource",
  [SyntaxKind.PartiallyEmittedExpression]: "expression",
  [SyntaxKind.JsxAttributes]: "properties",
  [SyntaxKind.JsxSpreadAttribute]: "expression",
  [SyntaxKind.JsxClosingElement]: "tagName",
  [SyntaxKind.SyntaxList]: "children",
  [SyntaxKind.JSDocTypeExpression]: "type",
  [SyntaxKind.JSDocNonNullableType]: "type",
  [SyntaxKind.JSDocNullableType]: "type",
  [SyntaxKind.JSDocVariadicType]: "type",
  [SyntaxKind.JSDocOptionalType]: "type",
  [SyntaxKind.JSDocNameReference]: "name",
  [SyntaxKind.JSDocLink]: "name",
  [SyntaxKind.JSDocLinkPlain]: "name",
  [SyntaxKind.JSDocLinkCode]: "name",
  [SyntaxKind.JSDocTypeLiteral]: "jsdocPropertyTags"
};

// node_modules/typescript/dist/api/node/protocol.js
var PROTOCOL_VERSION = 5;
var HEADER_OFFSET_METADATA = 0;
var HEADER_OFFSET_HASH_LO0 = 4;
var HEADER_OFFSET_HASH_LO1 = 8;
var HEADER_OFFSET_HASH_HI0 = 12;
var HEADER_OFFSET_HASH_HI1 = 16;
var HEADER_OFFSET_PARSE_OPTIONS = 20;
var HEADER_OFFSET_STRING_TABLE_OFFSETS = 24;
var HEADER_OFFSET_STRING_TABLE = 28;
var HEADER_OFFSET_EXTENDED_DATA = 32;
var HEADER_OFFSET_STRUCTURED_DATA = 36;
var HEADER_OFFSET_NODES = 40;
var HEADER_SIZE = 44;
var NODE_LEN = 28;
var NODE_OFFSET_KIND = 0;
var NODE_OFFSET_POS = 4;
var NODE_OFFSET_END = 8;
var NODE_OFFSET_NEXT = 12;
var NODE_OFFSET_PARENT = 16;
var NODE_OFFSET_DATA = 20;
var NODE_OFFSET_FLAGS = 24;
var KIND_NODE_LIST = 4294967295;
var NODE_DATA_TYPE_CHILDREN = 0;
var NODE_DATA_TYPE_STRING = 1073741824;
var NODE_DATA_TYPE_EXTENDED = 2147483648;

// node_modules/typescript/dist/api/node/encoder.generated.js
function getNodeDataType(kind) {
  switch (kind) {
    case SyntaxKind.Identifier:
    case SyntaxKind.PrivateIdentifier:
    case SyntaxKind.JsxText:
    case SyntaxKind.JSDocText:
    case SyntaxKind.JSDocLink:
    case SyntaxKind.JSDocLinkPlain:
    case SyntaxKind.JSDocLinkCode:
      return NODE_DATA_TYPE_STRING;
    case SyntaxKind.StringLiteral:
    case SyntaxKind.NumericLiteral:
    case SyntaxKind.BigIntLiteral:
    case SyntaxKind.RegularExpressionLiteral:
    case SyntaxKind.NoSubstitutionTemplateLiteral:
    case SyntaxKind.TemplateHead:
    case SyntaxKind.TemplateMiddle:
    case SyntaxKind.TemplateTail:
    case SyntaxKind.SourceFile:
      return NODE_DATA_TYPE_EXTENDED;
    default:
      return NODE_DATA_TYPE_CHILDREN;
  }
}
function getNodeCommonData(node) {
  switch (node.kind) {
    case SyntaxKind.Block:
      return (node.multiLine ? 1 : 0) << 24;
    case SyntaxKind.HeritageClause:
      return (node.token === SyntaxKind.ImplementsKeyword ? 1 : 0) << 24;
    case SyntaxKind.ExportAssignment:
      return (node.isExportEquals ? 1 : 0) << 24;
    case SyntaxKind.ExportSpecifier:
      return (node.isTypeOnly ? 1 : 0) << 24;
    case SyntaxKind.PrefixUnaryExpression:
      return (node.operator === SyntaxKind.MinusToken ? 1 : node.operator === SyntaxKind.TildeToken ? 2 : node.operator === SyntaxKind.ExclamationToken ? 3 : node.operator === SyntaxKind.PlusPlusToken ? 4 : node.operator === SyntaxKind.MinusMinusToken ? 5 : 0) << 24;
    case SyntaxKind.PostfixUnaryExpression:
      return (node.operator === SyntaxKind.MinusMinusToken ? 1 : 0) << 24;
    case SyntaxKind.MetaProperty:
      return (node.keywordToken === SyntaxKind.NewKeyword ? 1 : 0) << 24;
    case SyntaxKind.ArrayLiteralExpression:
      return (node.multiLine ? 1 : 0) << 24;
    case SyntaxKind.ObjectLiteralExpression:
      return (node.multiLine ? 1 : 0) << 24;
    case SyntaxKind.TypeOperator:
      return (node.operator === SyntaxKind.ReadonlyKeyword ? 1 : node.operator === SyntaxKind.UniqueKeyword ? 2 : 0) << 24;
    case SyntaxKind.ImportAttributes:
      return (node.multiLine ? 1 : 0) << 24 | (node.token === SyntaxKind.AssertKeyword ? 1 : 0) << 25;
    case SyntaxKind.JsxText:
      return (node.containsOnlyTriviaWhiteSpaces ? 1 : 0) << 24;
    case SyntaxKind.ModuleDeclaration:
      return (node.keyword === SyntaxKind.NamespaceKeyword ? 1 : 0) << 24;
    case SyntaxKind.ImportEqualsDeclaration:
      return (node.isTypeOnly ? 1 : 0) << 24;
    case SyntaxKind.ExportDeclaration:
      return (node.isTypeOnly ? 1 : 0) << 24;
    case SyntaxKind.ImportType:
      return (node.isTypeOf ? 1 : 0) << 24;
    case SyntaxKind.ImportClause:
      return (node.phaseModifier === SyntaxKind.TypeKeyword ? 1 : node.phaseModifier === SyntaxKind.DeferKeyword ? 2 : 0) << 24;
    case SyntaxKind.ImportSpecifier:
      return (node.isTypeOnly ? 1 : 0) << 24;
    case SyntaxKind.JSDocTypeLiteral:
      return (node.isArrayType ? 1 : 0) << 24;
    case SyntaxKind.JSDocParameterTag:
    case SyntaxKind.JSDocPropertyTag:
      return (node.isBracketed ? 1 : 0) << 24 | (node.isNameFirst ? 1 : 0) << 25;
  }
  return 0;
}

// node_modules/typescript/dist/api/node/wtf8.js
import { Buffer as Buffer2 } from "node:buffer";
var surrogateLeadByte = 237;
var surrogateSecondByteMin = 160;
var surrogateSecondByteMax = 191;
var continuationByteMin = 128;
var continuationByteMax = 191;
function isWtf8Surrogate(bytes, index) {
  return index + 2 < bytes.length && bytes[index] === surrogateLeadByte && bytes[index + 1] >= surrogateSecondByteMin && bytes[index + 1] <= surrogateSecondByteMax && bytes[index + 2] >= continuationByteMin && bytes[index + 2] <= continuationByteMax;
}
function getSurrogateCodeUnit(bytes, index) {
  return 53248 | (bytes[index + 1] & 63) << 6 | bytes[index + 2] & 63;
}
function hasSurrogateLeadByte(bytes) {
  return Buffer2.from(bytes.buffer, bytes.byteOffset, bytes.byteLength).indexOf(surrogateLeadByte) >= 0;
}
function toUint8Array(input) {
  if (input instanceof Uint8Array) {
    return input;
  }
  if (ArrayBuffer.isView(input)) {
    return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  }
  return new Uint8Array(input);
}
var Wtf8Decoder = class extends TextDecoder {
  decode(input, options) {
    if (input === void 0) {
      return super.decode(input, options);
    }
    const bytes = toUint8Array(input);
    if (!hasSurrogateLeadByte(bytes)) {
      return super.decode(bytes, options);
    }
    const parts = [];
    let segmentStart = 0;
    for (let i = 0; i < bytes.length; i++) {
      if (!isWtf8Surrogate(bytes, i)) {
        continue;
      }
      if (segmentStart < i) {
        parts.push(super.decode(bytes.subarray(segmentStart, i), options));
      }
      parts.push(String.fromCharCode(getSurrogateCodeUnit(bytes, i)));
      i += 2;
      segmentStart = i + 1;
    }
    if (segmentStart === 0) {
      return super.decode(bytes, options);
    }
    if (segmentStart < bytes.length) {
      parts.push(super.decode(bytes.subarray(segmentStart), options));
    }
    return parts.join("");
  }
};

// node_modules/typescript/dist/api/node/msgpack.js
var MSGPACK_FIXARRAY3 = 147;
var MSGPACK_BIN8 = 196;
var MSGPACK_BIN16 = 197;
var MSGPACK_BIN32 = 198;
var MSGPACK_UINT8 = 204;
function binHeaderSize(len) {
  if (len < 256)
    return 2;
  if (len < 65536)
    return 3;
  return 5;
}
function writeBinHeader(buf, off, len) {
  if (len < 256) {
    buf[off++] = MSGPACK_BIN8;
    buf[off++] = len;
  } else if (len < 65536) {
    buf[off++] = MSGPACK_BIN16;
    buf[off++] = len >>> 8 & 255;
    buf[off++] = len & 255;
  } else {
    buf[off++] = MSGPACK_BIN32;
    buf[off++] = len >>> 24 & 255;
    buf[off++] = len >>> 16 & 255;
    buf[off++] = len >>> 8 & 255;
    buf[off++] = len & 255;
  }
  return off;
}
var encoder = new TextEncoder();
var decoder = new Wtf8Decoder();
var MsgpackWriter = class {
  buf;
  view;
  pos;
  constructor(initialSize = 256) {
    this.buf = new Uint8Array(initialSize);
    this.view = new DataView(this.buf.buffer);
    this.pos = 0;
  }
  ensure(n) {
    if (this.pos + n > this.buf.length) {
      let newSize = this.buf.length * 2;
      while (newSize < this.pos + n)
        newSize *= 2;
      const next = new Uint8Array(newSize);
      next.set(this.buf);
      this.buf = next;
      this.view = new DataView(this.buf.buffer);
    }
  }
  writeArrayHeader(length) {
    if (length <= 15) {
      this.ensure(1);
      this.buf[this.pos++] = 144 | length;
    } else if (length <= 65535) {
      this.ensure(3);
      this.buf[this.pos++] = 220;
      this.view.setUint16(this.pos, length, false);
      this.pos += 2;
    } else {
      this.ensure(5);
      this.buf[this.pos++] = 221;
      this.view.setUint32(this.pos, length, false);
      this.pos += 4;
    }
  }
  writeUint(value) {
    if (value <= 127) {
      this.ensure(1);
      this.buf[this.pos++] = value;
    } else if (value <= 255) {
      this.ensure(2);
      this.buf[this.pos++] = 204;
      this.buf[this.pos++] = value;
    } else if (value <= 65535) {
      this.ensure(3);
      this.buf[this.pos++] = 205;
      this.view.setUint16(this.pos, value, false);
      this.pos += 2;
    } else {
      this.ensure(5);
      this.buf[this.pos++] = 206;
      this.view.setUint32(this.pos, value, false);
      this.pos += 4;
    }
  }
  writeString(str) {
    const encoded = encoder.encode(str);
    const len = encoded.length;
    if (len <= 31) {
      this.ensure(1 + len);
      this.buf[this.pos++] = 160 | len;
    } else if (len <= 255) {
      this.ensure(2 + len);
      this.buf[this.pos++] = 217;
      this.buf[this.pos++] = len;
    } else if (len <= 65535) {
      this.ensure(3 + len);
      this.buf[this.pos++] = 218;
      this.view.setUint16(this.pos, len, false);
      this.pos += 2;
    } else {
      this.ensure(5 + len);
      this.buf[this.pos++] = 219;
      this.view.setUint32(this.pos, len, false);
      this.pos += 4;
    }
    this.buf.set(encoded, this.pos);
    this.pos += len;
  }
  writeBool(value) {
    this.ensure(1);
    this.buf[this.pos++] = value ? 195 : 194;
  }
  finish() {
    return this.buf.subarray(0, this.pos);
  }
};
var MsgpackReader = class {
  buf;
  view;
  pos;
  constructor(data, offset = 0) {
    this.buf = data;
    this.view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    this.pos = offset;
  }
  readArrayHeader() {
    const byte = this.buf[this.pos++];
    if ((byte & 240) === 144)
      return byte & 15;
    if (byte === 220) {
      const len = this.view.getUint16(this.pos, false);
      this.pos += 2;
      return len;
    }
    if (byte === 221) {
      const len = this.view.getUint32(this.pos, false);
      this.pos += 4;
      return len;
    }
    throw new Error(`Expected array header, got 0x${byte.toString(16)}`);
  }
  readUint() {
    const byte = this.buf[this.pos++];
    if (byte <= 127)
      return byte;
    if (byte === 204)
      return this.buf[this.pos++];
    if (byte === 205) {
      const val = this.view.getUint16(this.pos, false);
      this.pos += 2;
      return val;
    }
    if (byte === 206) {
      const val = this.view.getUint32(this.pos, false);
      this.pos += 4;
      return val;
    }
    throw new Error(`Expected uint, got 0x${byte.toString(16)}`);
  }
  readString() {
    const byte = this.buf[this.pos++];
    let len;
    if ((byte & 224) === 160) {
      len = byte & 31;
    } else if (byte === 217) {
      len = this.buf[this.pos++];
    } else if (byte === 218) {
      len = this.view.getUint16(this.pos, false);
      this.pos += 2;
    } else if (byte === 219) {
      len = this.view.getUint32(this.pos, false);
      this.pos += 4;
    } else {
      throw new Error(`Expected string, got 0x${byte.toString(16)}`);
    }
    const str = decoder.decode(this.buf.subarray(this.pos, this.pos + len));
    this.pos += len;
    return str;
  }
  readBool() {
    const byte = this.buf[this.pos++];
    if (byte === 195)
      return true;
    if (byte === 194)
      return false;
    throw new Error(`Expected bool, got 0x${byte.toString(16)}`);
  }
};

// node_modules/typescript/dist/api/node/encoder.js
var NODE_FIELDS = NODE_LEN / 4;
var NODE_FIELD_NEXT = 3;
var NO_STRUCTURED_DATA = 4294967295;
var StringTable = class {
  parts;
  byteLen;
  offsets;
  constructor() {
    this.parts = [];
    this.byteLen = 0;
    this.offsets = [];
  }
  add(text) {
    const index = this.offsets.length;
    const encoder2 = cachedEncoder();
    const encodedLength = encoder2.encode(text).length;
    const offset = this.byteLen;
    this.parts.push(text);
    this.byteLen += encodedLength;
    this.offsets.push(offset, offset + encodedLength);
    return index;
  }
  encode() {
    const encoder2 = cachedEncoder();
    const dataBytes = encoder2.encode(this.parts.join(""));
    const offsetBytes = new Uint8Array(this.offsets.length * 4);
    const view = new DataView(offsetBytes.buffer);
    for (let i = 0; i < this.offsets.length; i++) {
      view.setUint32(i * 4, this.offsets[i], true);
    }
    const result = new Uint8Array(offsetBytes.length + dataBytes.length);
    result.set(offsetBytes, 0);
    result.set(dataBytes, offsetBytes.length);
    return result;
  }
  stringByteLength() {
    return this.byteLen;
  }
  offsetsCount() {
    return this.offsets.length;
  }
};
var _encoder;
function cachedEncoder() {
  return _encoder ??= new TextEncoder();
}
function getChildrenPropertyMask(node) {
  const kind = node.kind;
  const props = childProperties[kind];
  if (!props) {
    return 0;
  }
  const n = node;
  let mask = 0;
  for (let i = 0; i < props.length; i++) {
    const prop = props[i];
    if (prop !== void 0 && isChildPresent(n[prop])) {
      mask |= 1 << i;
    }
  }
  return mask;
}
function isChildPresent(v) {
  if (v === void 0 || v === null)
    return false;
  return true;
}
function recordNodeStrings(node, strs) {
  return strs.add(node.text ?? "");
}
function encodeFileReferences(refs, writer) {
  if (!refs || refs.length === 0)
    return NO_STRUCTURED_DATA;
  const offset = writer.finish().length;
  writer.writeArrayHeader(refs.length);
  for (const ref of refs) {
    writer.writeArrayHeader(5);
    writer.writeUint(ref.pos);
    writer.writeUint(ref.end);
    writer.writeString(ref.fileName);
    writer.writeUint(ref.resolutionMode ?? 0);
    writer.writeBool(ref.preserve ?? false);
  }
  return offset;
}
function recordExtendedData(node, strs, extendedData, structuredWriter) {
  const offset = extendedData.length * 4;
  if (node.kind === SyntaxKind.SourceFile) {
    const sf = node;
    const textIndex = strs.add(sf.text);
    const fileNameIndex = strs.add(sf.fileName);
    const pathIndex = strs.add(sf.path);
    const referencedFilesOffset = encodeFileReferences(sf.referencedFiles, structuredWriter);
    const typeRefDirectivesOffset = encodeFileReferences(sf.typeReferenceDirectives, structuredWriter);
    const libRefDirectivesOffset = encodeFileReferences(sf.libReferenceDirectives, structuredWriter);
    extendedData.push(textIndex, fileNameIndex, pathIndex, sf.languageVariant, sf.scriptKind, referencedFilesOffset, typeRefDirectivesOffset, libRefDirectivesOffset, NO_STRUCTURED_DATA, NO_STRUCTURED_DATA, NO_STRUCTURED_DATA, 0);
  } else if (node.kind === SyntaxKind.TemplateHead || node.kind === SyntaxKind.TemplateMiddle || node.kind === SyntaxKind.TemplateTail) {
    const tmpl = node;
    const text = tmpl.text ?? "";
    const rawText = tmpl.rawText ?? "";
    const templateFlags = tmpl.templateFlags ?? 0;
    const textIndex = strs.add(text);
    const rawTextIndex = strs.add(rawText);
    extendedData.push(textIndex, rawTextIndex, templateFlags);
  } else {
    const n = node;
    const text = n.text ?? "";
    const tokenFlags = n.tokenFlags ?? 0;
    const textIndex = strs.add(text);
    extendedData.push(textIndex, tokenFlags);
  }
  return offset;
}
function getNodeData(node, strs, extendedData, structuredWriter) {
  const t = getNodeDataType(node.kind);
  const common = getNodeCommonData(node);
  switch (t) {
    case NODE_DATA_TYPE_CHILDREN:
      return t | common | getChildrenPropertyMask(node);
    case NODE_DATA_TYPE_STRING:
      return t | common | recordNodeStrings(node, strs);
    case NODE_DATA_TYPE_EXTENDED:
      return t | common | recordExtendedData(node, strs, extendedData, structuredWriter);
    default:
      throw new Error("unreachable");
  }
}
function getChildPropertiesForNode(node) {
  return childProperties[node.kind];
}
function isNodeArray2(value) {
  return Array.isArray(value) && typeof value.pos === "number" && typeof value.end === "number";
}
function encodeNode(node) {
  const strs = new StringTable();
  const extendedDataValues = [];
  const structuredWriter = new MsgpackWriter();
  const nodeValues = [];
  nodeValues.push(0, 0, 0, 0, 0, 0, 0);
  let nodeCount = 0;
  let parentIndex = 0;
  let prevIndex = 0;
  function visitNode3(node2) {
    nodeCount++;
    const currentIndex = nodeCount;
    if (prevIndex !== 0) {
      nodeValues[prevIndex * NODE_FIELDS + NODE_FIELD_NEXT] = currentIndex;
    }
    const data = getNodeData(node2, strs, extendedDataValues, structuredWriter);
    nodeValues.push(
      node2.kind,
      node2.pos >= 0 ? node2.pos : 0,
      node2.end >= 0 ? node2.end : 0,
      0,
      // next (filled in later)
      parentIndex,
      data,
      node2.flags
    );
    const saveParentIndex = parentIndex;
    const savePrevIndex = prevIndex;
    parentIndex = currentIndex;
    prevIndex = 0;
    visitChildren(node2);
    prevIndex = currentIndex;
    parentIndex = saveParentIndex;
  }
  function visitNodeList(list) {
    if (!list) {
      return;
    }
    nodeCount++;
    const currentIndex = nodeCount;
    if (prevIndex !== 0) {
      nodeValues[prevIndex * NODE_FIELDS + NODE_FIELD_NEXT] = currentIndex;
    }
    nodeValues.push(
      KIND_NODE_LIST,
      list.pos >= 0 ? list.pos : 0,
      list.end >= 0 ? list.end : 0,
      0,
      // next
      parentIndex,
      list.length,
      // data for NodeList is its length
      0
    );
    const saveParentIndex = parentIndex;
    parentIndex = currentIndex;
    prevIndex = 0;
    for (const child of list) {
      visitNode3(child);
    }
    prevIndex = currentIndex;
    parentIndex = saveParentIndex;
  }
  function visitChildren(node2) {
    const props = getChildPropertiesForNode(node2);
    const n = node2;
    if (props) {
      for (const propName of props) {
        if (propName === void 0)
          continue;
        const child = n[propName];
        if (child === void 0 || child === null)
          continue;
        if (isNodeArray2(child)) {
          visitNodeList(child);
        } else {
          visitNode3(child);
        }
      }
    }
  }
  nodeCount++;
  parentIndex++;
  const rootData = getNodeData(node, strs, extendedDataValues, structuredWriter);
  nodeValues.push(node.kind, node.pos >= 0 ? node.pos : 0, node.end >= 0 ? node.end : 0, 0, 0, rootData, node.flags);
  const saveParent = parentIndex;
  prevIndex = 0;
  parentIndex = 1;
  visitChildren(node);
  parentIndex = saveParent;
  const extendedDataBytes = new Uint8Array(extendedDataValues.length * 4);
  const extView = new DataView(extendedDataBytes.buffer);
  for (let i = 0; i < extendedDataValues.length; i++) {
    extView.setUint32(i * 4, extendedDataValues[i], true);
  }
  const structuredDataBytes = structuredWriter.finish();
  const strsBytes = strs.encode();
  const nodesBytes = new Uint8Array(nodeValues.length * 4);
  const nodesView = new DataView(nodesBytes.buffer);
  for (let i = 0; i < nodeValues.length; i++) {
    nodesView.setUint32(i * 4, nodeValues[i] >>> 0, true);
  }
  const offsetStringTableOffsets = HEADER_SIZE;
  const offsetStringTableData = HEADER_SIZE + strs.offsetsCount() * 4;
  const offsetExtendedData = offsetStringTableData + strs.stringByteLength();
  const offsetStructuredData = offsetExtendedData + extendedDataBytes.length;
  const offsetNodes = offsetStructuredData + structuredDataBytes.length;
  const header = new Uint8Array(HEADER_SIZE);
  const headerView = new DataView(header.buffer);
  const metadata = PROTOCOL_VERSION << 24;
  headerView.setUint32(HEADER_OFFSET_METADATA, metadata, true);
  headerView.setUint32(HEADER_OFFSET_STRING_TABLE_OFFSETS, offsetStringTableOffsets, true);
  headerView.setUint32(HEADER_OFFSET_STRING_TABLE, offsetStringTableData, true);
  headerView.setUint32(HEADER_OFFSET_EXTENDED_DATA, offsetExtendedData, true);
  headerView.setUint32(HEADER_OFFSET_STRUCTURED_DATA, offsetStructuredData, true);
  headerView.setUint32(HEADER_OFFSET_NODES, offsetNodes, true);
  const result = new Uint8Array(header.length + strsBytes.length + extendedDataBytes.length + structuredDataBytes.length + nodesBytes.length);
  result.set(header, 0);
  result.set(strsBytes, HEADER_SIZE);
  result.set(extendedDataBytes, offsetExtendedData);
  result.set(structuredDataBytes, offsetStructuredData);
  result.set(nodesBytes, offsetNodes);
  return result;
}
function uint8ArrayToBase64(data) {
  return Buffer.from(data).toString("base64");
}

// node_modules/typescript/dist/api/node/node.infrastructure.js
var popcount8 = [0, 1, 1, 2, 1, 2, 2, 3, 1, 2, 2, 3, 2, 3, 3, 4, 1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7, 1, 2, 2, 3, 2, 3, 3, 4, 2, 3, 3, 4, 3, 4, 4, 5, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7, 2, 3, 3, 4, 3, 4, 4, 5, 3, 4, 4, 5, 4, 5, 5, 6, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7, 3, 4, 4, 5, 4, 5, 5, 6, 4, 5, 5, 6, 5, 6, 6, 7, 4, 5, 5, 6, 5, 6, 6, 7, 5, 6, 6, 7, 6, 7, 7, 8];
var NODE_DATA_TYPE_MASK = 3221225472;
var NODE_CHILD_MASK = 255;
var NODE_STRING_INDEX_MASK = 16777215;
var NODE_EXTENDED_DATA_MASK = 16777215;
function readSourceFileHash(data) {
  const lo0 = data.getUint32(HEADER_OFFSET_HASH_LO0, true);
  const lo1 = data.getUint32(HEADER_OFFSET_HASH_LO1, true);
  const hi0 = data.getUint32(HEADER_OFFSET_HASH_HI0, true);
  const hi1 = data.getUint32(HEADER_OFFSET_HASH_HI1, true);
  return hex8(hi1) + hex8(hi0) + hex8(lo1) + hex8(lo0);
}
function readParseOptionsKey(data) {
  return data.getUint32(HEADER_OFFSET_PARSE_OPTIONS, true).toString();
}
function hex8(n) {
  return (n >>> 0).toString(16).padStart(8, "0");
}
function modifierToFlag(kind) {
  switch (kind) {
    case SyntaxKind.StaticKeyword:
      return ModifierFlags.Static;
    case SyntaxKind.PublicKeyword:
      return ModifierFlags.Public;
    case SyntaxKind.ProtectedKeyword:
      return ModifierFlags.Protected;
    case SyntaxKind.PrivateKeyword:
      return ModifierFlags.Private;
    case SyntaxKind.AbstractKeyword:
      return ModifierFlags.Abstract;
    case SyntaxKind.AccessorKeyword:
      return ModifierFlags.Accessor;
    case SyntaxKind.ExportKeyword:
      return ModifierFlags.Export;
    case SyntaxKind.DeclareKeyword:
      return ModifierFlags.Ambient;
    case SyntaxKind.ConstKeyword:
      return ModifierFlags.Const;
    case SyntaxKind.DefaultKeyword:
      return ModifierFlags.Default;
    case SyntaxKind.AsyncKeyword:
      return ModifierFlags.Async;
    case SyntaxKind.ReadonlyKeyword:
      return ModifierFlags.Readonly;
    case SyntaxKind.OverrideKeyword:
      return ModifierFlags.Override;
    case SyntaxKind.InKeyword:
      return ModifierFlags.In;
    case SyntaxKind.OutKeyword:
      return ModifierFlags.Out;
    case SyntaxKind.Decorator:
      return ModifierFlags.Decorator;
    default:
      return ModifierFlags.None;
  }
}
var RemoteNodeBase = class {
  parent;
  // RemoteNode at runtime
  view;
  index;
  _byteIndex;
  constructor(view, index, parent, byteIndex) {
    this.view = view;
    this.index = index;
    this.parent = parent;
    this._byteIndex = byteIndex;
  }
  get kind() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_KIND, true);
  }
  get pos() {
    return this.view.getInt32(this._byteIndex + NODE_OFFSET_POS, true);
  }
  get end() {
    return this.view.getInt32(this._byteIndex + NODE_OFFSET_END, true);
  }
  get next() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_NEXT, true);
  }
  get parentIndex() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_PARENT, true);
  }
  get data() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_DATA, true);
  }
  get dataType() {
    return this.data & NODE_DATA_TYPE_MASK;
  }
  get childMask() {
    if (this.dataType !== NODE_DATA_TYPE_CHILDREN) {
      return -1;
    }
    return this.data & NODE_CHILD_MASK;
  }
  getFileText(start, end) {
    return this.sourceFile._decoder.decode(new Uint8Array(this.view.buffer, this.view.byteOffset + this.sourceFile._offsetStringTable + start, end - start));
  }
  get sourceFile() {
    throw new Error("sourceFile not available on base");
  }
};

// node_modules/typescript/dist/api/node/node.generated.js
var RemoteNodeList = class extends Array {
  // Inherited Array methods like filter/map/slice use ArraySpeciesCreate, which would
  // otherwise call `new RemoteNodeList(length)` and fail. Produce a plain Array instead.
  static get [Symbol.species]() {
    return Array;
  }
  parent;
  hasTrailingComma;
  transformFlags = 0;
  view;
  index;
  _byteIndex;
  // Cursor memoizing the last resolved (logical index -> node index) so that
  // sequential forward access (index loops and list[i], plus forEach/map/
  // reduce/filter) resumes instead of re-walking from the head, turning an
  // O(n) pass over the whole list from O(n^2) into O(n).
  _cursorIndex = 0;
  _cursorNodeIndex = 0;
  get pos() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_POS, true);
  }
  get end() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_END, true);
  }
  get next() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_NEXT, true);
  }
  get data() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_DATA, true);
  }
  sourceFile;
  constructor(view, index, parent, sourceFile, offsetNodes) {
    super();
    this.view = view;
    this.index = index;
    this.parent = parent;
    this.sourceFile = sourceFile;
    this._byteIndex = offsetNodes + index * NODE_LEN;
    this.length = this.data;
    this._cursorNodeIndex = index + 1;
    const length = this.length;
    for (let i = 16; i < length; i++) {
      Object.defineProperty(this, i, {
        get() {
          return this.at(i);
        }
      });
    }
  }
  get 0() {
    return this.at(0);
  }
  get 1() {
    return this.at(1);
  }
  get 2() {
    return this.at(2);
  }
  get 3() {
    return this.at(3);
  }
  get 4() {
    return this.at(4);
  }
  get 5() {
    return this.at(5);
  }
  get 6() {
    return this.at(6);
  }
  get 7() {
    return this.at(7);
  }
  get 8() {
    return this.at(8);
  }
  get 9() {
    return this.at(9);
  }
  get 10() {
    return this.at(10);
  }
  get 11() {
    return this.at(11);
  }
  get 12() {
    return this.at(12);
  }
  get 13() {
    return this.at(13);
  }
  get 14() {
    return this.at(14);
  }
  get 15() {
    return this.at(15);
  }
  *[Symbol.iterator]() {
    if (!this.length)
      return;
    let next = this.index + 1;
    while (next) {
      const child = this.getOrCreateChildAtNodeIndex(next);
      next = child.next;
      yield child;
    }
  }
  forEachNode(visitNode3) {
    if (!this.length)
      return;
    let next = this.index + 1;
    while (next) {
      const child = this.getOrCreateChildAtNodeIndex(next);
      next = child.next;
      const result = visitNode3(child);
      if (result)
        return result;
    }
  }
  at(index) {
    if (!Number.isInteger(index)) {
      return void 0;
    }
    if (index >= this.data || index < 0 && -index > this.data) {
      return void 0;
    }
    if (index < 0) {
      index = this.length + index;
    }
    const offsetNodes = this.sourceFile._offsetNodes;
    let i;
    let next;
    if (index >= this._cursorIndex) {
      i = this._cursorIndex;
      next = this._cursorNodeIndex;
    } else {
      i = 0;
      next = this.index + 1;
    }
    for (; i < index; i++) {
      next = this.view.getUint32(offsetNodes + next * NODE_LEN + NODE_OFFSET_NEXT, true);
    }
    this._cursorIndex = index;
    this._cursorNodeIndex = next;
    return this.getOrCreateChildAtNodeIndex(next);
  }
  getOrCreateChildAtNodeIndex(index) {
    let child = this.sourceFile.nodes[index];
    if (!child) {
      const kind = this.view.getUint32(this.sourceFile._offsetNodes + index * NODE_LEN + NODE_OFFSET_KIND, true);
      if (kind === KIND_NODE_LIST) {
        throw new Error("NodeList cannot directly contain another NodeList");
      }
      const sf = this.sourceFile;
      child = new RemoteNode(this.view, index, this.parent, sf, sf._offsetNodes);
      sf.nodes[index] = child;
      sf._timing?.recordMaterialization();
    }
    return child;
  }
  __print() {
    const result = [];
    result.push(`kind: NodeList`);
    result.push(`index: ${this.index}`);
    result.push(`byteIndex: ${this._byteIndex}`);
    result.push(`length: ${this.length}`);
    return result.join("\n");
  }
};
var RemoteNode = class _RemoteNode extends RemoteNodeBase {
  static NODE_LEN = NODE_LEN;
  get sourceFile() {
    return this._sourceFile;
  }
  _sourceFile;
  get id() {
    return `${this.index}.${this.kind}.${this.sourceFile.path}`;
  }
  constructor(view, index, parent, sourceFile, offsetNodes) {
    super(view, index, parent, offsetNodes + index * NODE_LEN);
    this._sourceFile = sourceFile;
  }
  forEachChild(visitNode3, visitList) {
    if (this.hasChildren()) {
      let next = this.index + 1;
      do {
        const child = this.getOrCreateChildAtNodeIndex(next);
        if (child instanceof RemoteNodeList) {
          if (visitList) {
            const result = visitList(child);
            if (result) {
              return result;
            }
          } else {
            const result = child.forEachNode(visitNode3);
            if (result) {
              return result;
            }
          }
        } else if (child.kind !== SyntaxKind.JSDoc) {
          const result = visitNode3(child);
          if (result) {
            return result;
          }
        }
        next = child.next;
      } while (next);
    }
  }
  get jsDoc() {
    if (!this.hasChildren()) {
      return void 0;
    }
    let result;
    let next = this.index + 1;
    do {
      const child = this.getOrCreateChildAtNodeIndex(next);
      if (!(child instanceof RemoteNodeList) && child.kind === SyntaxKind.JSDoc) {
        (result ??= []).push(child);
      }
      next = child.next;
    } while (next);
    return result;
  }
  getSourceFile() {
    return this.sourceFile;
  }
  getStart(sourceFile, includeJsDocComment) {
    return getTokenPosOfNode(this, sourceFile ?? this.getSourceFile(), includeJsDocComment);
  }
  getFullStart() {
    return this.pos;
  }
  getEnd() {
    return this.end;
  }
  getWidth(sourceFile) {
    return this.getEnd() - this.getStart(sourceFile);
  }
  getFullWidth() {
    return this.end - this.pos;
  }
  getLeadingTriviaWidth(sourceFile) {
    return this.getStart(sourceFile) - this.pos;
  }
  getFullText(sourceFile) {
    return (sourceFile ?? this.getSourceFile()).text.substring(this.pos, this.end);
  }
  getText(sourceFile) {
    sourceFile ??= this.getSourceFile();
    return sourceFile.text.substring(this.getStart(sourceFile), this.end);
  }
  getString(index) {
    const offsetStringTableOffsets = this.sourceFile._offsetStringTableOffsets;
    const start = this.view.getUint32(offsetStringTableOffsets + index * 4, true);
    const end = this.view.getUint32(offsetStringTableOffsets + (index + 1) * 4, true);
    const offsetStringTable = this.sourceFile._offsetStringTable;
    const text = new Uint8Array(this.view.buffer, this.view.byteOffset + offsetStringTable + start, end - start);
    return this.sourceFile._decoder.decode(text);
  }
  getOrCreateChildAtNodeIndex(index) {
    let child = this.sourceFile.nodes[index];
    if (!child) {
      const sf = this.sourceFile;
      const offsetNodes = sf._offsetNodes;
      const kind = this.view.getUint32(offsetNodes + index * NODE_LEN + NODE_OFFSET_KIND, true);
      child = kind === KIND_NODE_LIST ? new RemoteNodeList(this.view, index, this, sf, offsetNodes) : new _RemoteNode(this.view, index, this, sf, offsetNodes);
      sf.nodes[index] = child;
      sf._timing?.recordMaterialization();
    }
    return child;
  }
  hasChildren() {
    if (this._byteIndex >= this.view.byteLength - NODE_LEN) {
      return false;
    }
    const nextNodeParent = this.view.getUint32(this.sourceFile._offsetNodes + (this.index + 1) * NODE_LEN + NODE_OFFSET_PARENT, true);
    return nextNodeParent === this.index;
  }
  getNamedChild(propertyName) {
    const kind = this.kind;
    const propertyNames = childProperties[kind];
    if (!propertyNames) {
      return void 0;
    }
    const order = propertyNames.indexOf(propertyName);
    if (order === -1) {
      return void 0;
    }
    return this.getChildAtOrder(order);
  }
  getChildAtOrder(order) {
    const mask = this.childMask;
    if (!(mask & 1 << order)) {
      return void 0;
    }
    const propertyIndex = order - popcount8[~(mask | 255 << order & 255) & 255];
    let childIndex = this.index + 1;
    for (let i = 0; i < propertyIndex; i++) {
      childIndex = this.view.getUint32(this.sourceFile._offsetNodes + childIndex * NODE_LEN + NODE_OFFSET_NEXT, true);
    }
    return this.getOrCreateChildAtNodeIndex(childIndex);
  }
  __print() {
    const result = [];
    result.push(`index: ${this.index}`);
    result.push(`byteIndex: ${this._byteIndex}`);
    result.push(`kind: ${SyntaxKind[this.kind]}`);
    result.push(`pos: ${this.pos}`);
    result.push(`end: ${this.end}`);
    result.push(`next: ${this.next}`);
    result.push(`parent: ${this.parentIndex}`);
    result.push(`data: ${this.data.toString(2).padStart(32, "0")}`);
    const dataType = this.dataType === NODE_DATA_TYPE_CHILDREN ? "children" : this.dataType === NODE_DATA_TYPE_STRING ? "string" : "extended";
    result.push(`dataType: ${dataType}`);
    if (this.dataType === NODE_DATA_TYPE_CHILDREN) {
      result.push(`childMask: ${this.childMask.toString(2).padStart(8, "0")}`);
      result.push(`childProperties: ${childProperties[this.kind]?.join(", ")}`);
    }
    return result.join("\n");
  }
  __printChildren() {
    const result = [];
    let next = this.index + 1;
    while (next) {
      const child = this.getOrCreateChildAtNodeIndex(next);
      next = child.next;
      result.push(child.__print());
    }
    return result.join("\n\n");
  }
  __printSubtree() {
    const result = [this.__print()];
    this.forEachChild(function visitNode3(node) {
      result.push(node.__print());
      node.forEachChild(visitNode3);
    }, (visitList) => {
      result.push(visitList.__print());
    });
    return result.join("\n\n");
  }
  // ═══ Generated boolean property getters ═══
  get containsOnlyTriviaWhiteSpaces() {
    return (this.data & 1 << 24) !== 0;
  }
  get isArrayType() {
    return (this.data & 1 << 24) !== 0;
  }
  get isBracketed() {
    return (this.data & 1 << 24) !== 0;
  }
  get isExportEquals() {
    return (this.data & 1 << 24) !== 0;
  }
  get isNameFirst() {
    return (this.data & 1 << 25) !== 0;
  }
  get isTypeOf() {
    return (this.data & 1 << 24) !== 0;
  }
  get isTypeOnly() {
    return (this.data & 1 << 24) !== 0;
  }
  get multiLine() {
    return (this.data & 1 << 24) !== 0;
  }
  // ═══ Generated SyntaxKind union property getters ═══
  get keyword() {
    switch (this.kind) {
      case SyntaxKind.ModuleDeclaration:
        return this.data >> 24 & 1 ? SyntaxKind.NamespaceKeyword : SyntaxKind.ModuleKeyword;
    }
  }
  get keywordToken() {
    switch (this.kind) {
      case SyntaxKind.MetaProperty:
        return this.data >> 24 & 1 ? SyntaxKind.NewKeyword : SyntaxKind.ImportKeyword;
    }
  }
  get operator() {
    switch (this.kind) {
      case SyntaxKind.PrefixUnaryExpression: {
        const idx = this.data >> 24 & 7;
        if (idx === 1)
          return SyntaxKind.MinusToken;
        if (idx === 2)
          return SyntaxKind.TildeToken;
        if (idx === 3)
          return SyntaxKind.ExclamationToken;
        if (idx === 4)
          return SyntaxKind.PlusPlusToken;
        if (idx === 5)
          return SyntaxKind.MinusMinusToken;
        return SyntaxKind.PlusToken;
      }
      case SyntaxKind.PostfixUnaryExpression:
        return this.data >> 24 & 1 ? SyntaxKind.MinusMinusToken : SyntaxKind.PlusPlusToken;
      case SyntaxKind.TypeOperator: {
        const idx = this.data >> 24 & 3;
        if (idx === 1)
          return SyntaxKind.ReadonlyKeyword;
        if (idx === 2)
          return SyntaxKind.UniqueKeyword;
        return SyntaxKind.KeyOfKeyword;
      }
    }
  }
  get phaseModifier() {
    switch (this.kind) {
      case SyntaxKind.ImportClause: {
        const idx = this.data >> 24 & 3;
        if (idx === 0)
          return void 0;
        return idx === 1 ? SyntaxKind.TypeKeyword : idx === 2 ? SyntaxKind.DeferKeyword : void 0;
      }
    }
  }
  get token() {
    switch (this.kind) {
      case SyntaxKind.HeritageClause:
        return this.data >> 24 & 1 ? SyntaxKind.ImplementsKeyword : SyntaxKind.ExtendsKeyword;
      case SyntaxKind.ImportAttributes:
        return this.data >> 25 & 1 ? SyntaxKind.AssertKeyword : SyntaxKind.WithKeyword;
    }
  }
  get templateFlags() {
    switch (this.kind) {
      case SyntaxKind.TemplateHead:
      case SyntaxKind.TemplateMiddle:
      case SyntaxKind.TemplateTail:
        const extendedDataOffset = this.sourceFile._offsetExtendedData + (this.data & NODE_EXTENDED_DATA_MASK);
        return this.view.getUint32(extendedDataOffset + 8, true);
    }
  }
  get tokenFlags() {
    switch (this.kind) {
      case SyntaxKind.StringLiteral:
      case SyntaxKind.NumericLiteral:
      case SyntaxKind.BigIntLiteral:
      case SyntaxKind.RegularExpressionLiteral:
        const extendedDataOffset = this.sourceFile._offsetExtendedData + (this.data & NODE_EXTENDED_DATA_MASK);
        return this.view.getUint32(extendedDataOffset + 4, true);
      default:
        return 0;
    }
  }
  // ═══ Generated child property getters ═══
  get argument() {
    return this.getNamedChild("argument");
  }
  get argumentExpression() {
    return this.getNamedChild("argumentExpression");
  }
  get arguments() {
    return this.getNamedChild("arguments");
  }
  get assertsModifier() {
    return this.getNamedChild("assertsModifier");
  }
  get asteriskToken() {
    return this.getNamedChild("asteriskToken");
  }
  get attributes() {
    return this.getNamedChild("attributes");
  }
  get awaitModifier() {
    return this.getNamedChild("awaitModifier");
  }
  get block() {
    return this.getNamedChild("block");
  }
  get body() {
    return this.getNamedChild("body");
  }
  get caseBlock() {
    return this.getNamedChild("caseBlock");
  }
  get catchClause() {
    return this.getNamedChild("catchClause");
  }
  get checkType() {
    return this.getNamedChild("checkType");
  }
  get children() {
    return this.getNamedChild("children");
  }
  get className() {
    return this.getNamedChild("className");
  }
  get clauses() {
    return this.getNamedChild("clauses");
  }
  get closingElement() {
    return this.getNamedChild("closingElement");
  }
  get closingFragment() {
    return this.getNamedChild("closingFragment");
  }
  get colonToken() {
    return this.getNamedChild("colonToken");
  }
  get comment() {
    return this.getNamedChild("comment");
  }
  get condition() {
    return this.getNamedChild("condition");
  }
  get constraint() {
    return this.getNamedChild("constraint");
  }
  get declarationList() {
    return this.getNamedChild("declarationList");
  }
  get declarations() {
    return this.getNamedChild("declarations");
  }
  get defaultType() {
    return this.getNamedChild("defaultType");
  }
  get dotDotDotToken() {
    return this.getNamedChild("dotDotDotToken");
  }
  get elements() {
    return this.getNamedChild("elements");
  }
  get elementType() {
    return this.getNamedChild("elementType");
  }
  get elseStatement() {
    return this.getNamedChild("elseStatement");
  }
  get endOfFileToken() {
    return this.getNamedChild("endOfFileToken");
  }
  get equalsGreaterThanToken() {
    return this.getNamedChild("equalsGreaterThanToken");
  }
  get equalsToken() {
    return this.getNamedChild("equalsToken");
  }
  get exclamationToken() {
    return this.getNamedChild("exclamationToken");
  }
  get exportClause() {
    return this.getNamedChild("exportClause");
  }
  get expression() {
    return this.getNamedChild("expression");
  }
  get exprName() {
    return this.getNamedChild("exprName");
  }
  get extendsType() {
    return this.getNamedChild("extendsType");
  }
  get falseType() {
    return this.getNamedChild("falseType");
  }
  get finallyBlock() {
    return this.getNamedChild("finallyBlock");
  }
  get head() {
    return this.getNamedChild("head");
  }
  get heritageClauses() {
    return this.getNamedChild("heritageClauses");
  }
  get importClause() {
    return this.getNamedChild("importClause");
  }
  get incrementor() {
    return this.getNamedChild("incrementor");
  }
  get indexType() {
    return this.getNamedChild("indexType");
  }
  get initializer() {
    return this.getNamedChild("initializer");
  }
  get jsdocPropertyTags() {
    return this.getNamedChild("jsdocPropertyTags");
  }
  get label() {
    return this.getNamedChild("label");
  }
  get left() {
    return this.getNamedChild("left");
  }
  get literal() {
    return this.getNamedChild("literal");
  }
  get members() {
    return this.getNamedChild("members");
  }
  get modifiers() {
    return this.getNamedChild("modifiers");
  }
  get moduleReference() {
    return this.getNamedChild("moduleReference");
  }
  get moduleSpecifier() {
    return this.getNamedChild("moduleSpecifier");
  }
  get name() {
    return this.getNamedChild("name");
  }
  get namedBindings() {
    return this.getNamedChild("namedBindings");
  }
  get nameExpression() {
    return this.getNamedChild("nameExpression");
  }
  get namespace() {
    return this.getNamedChild("namespace");
  }
  get nameType() {
    return this.getNamedChild("nameType");
  }
  get objectAssignmentInitializer() {
    return this.getNamedChild("objectAssignmentInitializer");
  }
  get objectType() {
    return this.getNamedChild("objectType");
  }
  get openingElement() {
    return this.getNamedChild("openingElement");
  }
  get openingFragment() {
    return this.getNamedChild("openingFragment");
  }
  get operand() {
    return this.getNamedChild("operand");
  }
  get operatorToken() {
    return this.getNamedChild("operatorToken");
  }
  get parameterName() {
    return this.getNamedChild("parameterName");
  }
  get parameters() {
    return this.getNamedChild("parameters");
  }
  get postfixToken() {
    return this.getNamedChild("postfixToken");
  }
  get properties() {
    return this.getNamedChild("properties");
  }
  get propertyName() {
    return this.getNamedChild("propertyName");
  }
  get qualifier() {
    return this.getNamedChild("qualifier");
  }
  get questionDotToken() {
    return this.getNamedChild("questionDotToken");
  }
  get questionToken() {
    return this.getNamedChild("questionToken");
  }
  get readonlyToken() {
    return this.getNamedChild("readonlyToken");
  }
  get right() {
    return this.getNamedChild("right");
  }
  get statement() {
    return this.getNamedChild("statement");
  }
  get statements() {
    return this.getNamedChild("statements");
  }
  get tag() {
    return this.getNamedChild("tag");
  }
  get tagName() {
    return this.getNamedChild("tagName");
  }
  get tags() {
    return this.getNamedChild("tags");
  }
  get template() {
    return this.getNamedChild("template");
  }
  get templateSpans() {
    return this.getNamedChild("templateSpans");
  }
  get thenStatement() {
    return this.getNamedChild("thenStatement");
  }
  get thisArg() {
    return this.getNamedChild("thisArg");
  }
  get trueType() {
    return this.getNamedChild("trueType");
  }
  get tryBlock() {
    return this.getNamedChild("tryBlock");
  }
  get tupleNameSource() {
    return this.getNamedChild("tupleNameSource");
  }
  get type() {
    return this.getNamedChild("type");
  }
  get typeArguments() {
    return this.getNamedChild("typeArguments");
  }
  get typeExpression() {
    return this.getNamedChild("typeExpression");
  }
  get typeName() {
    return this.getNamedChild("typeName");
  }
  get typeParameter() {
    return this.getNamedChild("typeParameter");
  }
  get typeParameters() {
    return this.getNamedChild("typeParameters");
  }
  get types() {
    return this.getNamedChild("types");
  }
  get value() {
    return this.getNamedChild("value");
  }
  get variableDeclaration() {
    return this.getNamedChild("variableDeclaration");
  }
  get whenFalse() {
    return this.getNamedChild("whenFalse");
  }
  get whenTrue() {
    return this.getNamedChild("whenTrue");
  }
  // ═══ Generated string property getters ═══
  get text() {
    switch (this.kind) {
      case SyntaxKind.Identifier:
      case SyntaxKind.PrivateIdentifier:
      case SyntaxKind.JsxText:
      case SyntaxKind.JSDocText:
      case SyntaxKind.JSDocLink:
      case SyntaxKind.JSDocLinkPlain:
      case SyntaxKind.JSDocLinkCode: {
        const stringIndex = this.data & NODE_STRING_INDEX_MASK;
        return this.getString(stringIndex);
      }
      case SyntaxKind.StringLiteral:
      case SyntaxKind.NumericLiteral:
      case SyntaxKind.BigIntLiteral:
      case SyntaxKind.RegularExpressionLiteral:
      case SyntaxKind.NoSubstitutionTemplateLiteral:
      case SyntaxKind.TemplateHead:
      case SyntaxKind.TemplateMiddle:
      case SyntaxKind.TemplateTail:
      case SyntaxKind.SourceFile: {
        const extendedDataOffset = this.sourceFile._offsetExtendedData + (this.data & NODE_EXTENDED_DATA_MASK);
        const stringIndex = this.view.getUint32(extendedDataOffset, true);
        return this.getString(stringIndex);
      }
    }
  }
  get rawText() {
    switch (this.kind) {
      case SyntaxKind.TemplateHead:
      case SyntaxKind.TemplateMiddle:
      case SyntaxKind.TemplateTail:
        const extendedDataOffset = this.sourceFile._offsetExtendedData + (this.data & NODE_EXTENDED_DATA_MASK);
        const stringIndex = this.view.getUint32(extendedDataOffset + 4, true);
        return this.getString(stringIndex);
    }
  }
  // ═══ Generated extended data property getters ═══
  // ═══ Other property getters ═══
  get flags() {
    return this.view.getUint32(this._byteIndex + NODE_OFFSET_FLAGS, true);
  }
  get modifierFlags() {
    const mods = this.modifiers;
    if (!mods)
      return ModifierFlags.None;
    let flags = ModifierFlags.None;
    for (const mod of mods) {
      flags |= modifierToFlag(mod.kind);
    }
    return flags;
  }
};

// node_modules/typescript/dist/api/node/node.js
var NO_STRUCTURED_DATA2 = 4294967295;
var RemoteSourceFile = class extends RemoteNode {
  nodes;
  _offsetNodes;
  _offsetStringTableOffsets;
  _offsetStringTable;
  _offsetExtendedData;
  _offsetStructuredData;
  _decoder;
  _timing;
  _lineStarts;
  _cachedText;
  _cachedReferencedFiles;
  _cachedTypeReferenceDirectives;
  _cachedLibReferenceDirectives;
  _cachedImports;
  _cachedModuleAugmentations;
  _cachedAmbientModuleNames;
  constructor(data, decoder2, timing) {
    const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
    const offsetNodes = view.getUint32(HEADER_OFFSET_NODES, true);
    super(view, 1, void 0, void 0, offsetNodes);
    this._sourceFile = this;
    this._offsetNodes = offsetNodes;
    this._offsetStringTableOffsets = view.getUint32(HEADER_OFFSET_STRING_TABLE_OFFSETS, true);
    this._offsetStringTable = view.getUint32(HEADER_OFFSET_STRING_TABLE, true);
    this._offsetExtendedData = view.getUint32(HEADER_OFFSET_EXTENDED_DATA, true);
    this._offsetStructuredData = view.getUint32(HEADER_OFFSET_STRUCTURED_DATA, true);
    this._decoder = decoder2;
    this._timing = timing;
    this.nodes = Array((view.byteLength - offsetNodes) / NODE_LEN);
    this.nodes[1] = this;
    timing?.recordSourceFileFetched(Math.max(0, this.nodes.length - 2));
  }
  readFileReferences(structuredDataOffset) {
    if (structuredDataOffset === NO_STRUCTURED_DATA2) {
      return [];
    }
    const buf = new Uint8Array(this.view.buffer, this.view.byteOffset, this.view.byteLength);
    const reader = new MsgpackReader(buf, this._offsetStructuredData + structuredDataOffset);
    const count = reader.readArrayHeader();
    const result = [];
    for (let i = 0; i < count; i++) {
      reader.readArrayHeader();
      const pos = reader.readUint();
      const end = reader.readUint();
      const fileName = reader.readString();
      const resolutionMode = reader.readUint();
      const preserve = reader.readBool();
      result.push({ pos, end, fileName, resolutionMode, preserve });
    }
    return result;
  }
  readNodeIndexArray(structuredDataOffset) {
    if (structuredDataOffset === NO_STRUCTURED_DATA2) {
      return [];
    }
    const buf = new Uint8Array(this.view.buffer, this.view.byteOffset, this.view.byteLength);
    const reader = new MsgpackReader(buf, this._offsetStructuredData + structuredDataOffset);
    const count = reader.readArrayHeader();
    const result = [];
    for (let i = 0; i < count; i++) {
      const nodeIndex = reader.readUint();
      result.push(this.getOrCreateNodeAtIndex(nodeIndex));
    }
    return result;
  }
  readStringArray(structuredDataOffset) {
    if (structuredDataOffset === NO_STRUCTURED_DATA2) {
      return [];
    }
    const buf = new Uint8Array(this.view.buffer, this.view.byteOffset, this.view.byteLength);
    const reader = new MsgpackReader(buf, this._offsetStructuredData + structuredDataOffset);
    const count = reader.readArrayHeader();
    const result = [];
    for (let i = 0; i < count; i++) {
      result.push(reader.readString());
    }
    return result;
  }
  /** @internal */
  getOrCreateNodeAtIndex(index) {
    let node = this.nodes[index];
    if (!node) {
      let parentIndex = this.view.getUint32(this._offsetNodes + index * NODE_LEN + NODE_OFFSET_PARENT, true);
      while (parentIndex !== index && this.view.getUint32(this._offsetNodes + parentIndex * NODE_LEN + NODE_OFFSET_KIND, true) === KIND_NODE_LIST) {
        parentIndex = this.view.getUint32(this._offsetNodes + parentIndex * NODE_LEN + NODE_OFFSET_PARENT, true);
      }
      const parent = parentIndex === index ? this : this.getOrCreateNodeAtIndex(parentIndex);
      node = new RemoteNode(this.view, index, parent, this, this._offsetNodes);
      this.nodes[index] = node;
      this._timing?.recordMaterialization();
    }
    return node;
  }
  // ═══ SourceFile-specific extended data getters ═══
  get extendedDataOffset() {
    return this._offsetExtendedData + (this.data & NODE_EXTENDED_DATA_MASK);
  }
  get fileName() {
    const stringIndex = this.view.getUint32(this.extendedDataOffset + 4, true);
    return this.getString(stringIndex);
  }
  get path() {
    const stringIndex = this.view.getUint32(this.extendedDataOffset + 8, true);
    return this.getString(stringIndex);
  }
  get languageVariant() {
    return this.view.getUint32(this.extendedDataOffset + 12, true);
  }
  get scriptKind() {
    return this.view.getUint32(this.extendedDataOffset + 16, true);
  }
  get referencedFiles() {
    if (this._cachedReferencedFiles !== void 0)
      return this._cachedReferencedFiles;
    const offset = this.view.getUint32(this.extendedDataOffset + 20, true);
    const files = this.readFileReferences(offset);
    this._cachedReferencedFiles = files;
    return files;
  }
  get typeReferenceDirectives() {
    if (this._cachedTypeReferenceDirectives !== void 0)
      return this._cachedTypeReferenceDirectives;
    const offset = this.view.getUint32(this.extendedDataOffset + 24, true);
    const directives = this.readFileReferences(offset);
    this._cachedTypeReferenceDirectives = directives;
    return directives;
  }
  get libReferenceDirectives() {
    if (this._cachedLibReferenceDirectives !== void 0)
      return this._cachedLibReferenceDirectives;
    const offset = this.view.getUint32(this.extendedDataOffset + 28, true);
    const directives = this.readFileReferences(offset);
    this._cachedLibReferenceDirectives = directives;
    return directives;
  }
  get imports() {
    if (this._cachedImports !== void 0)
      return this._cachedImports;
    const offset = this.view.getUint32(this.extendedDataOffset + 32, true);
    const imports = this.readNodeIndexArray(offset);
    this._cachedImports = imports;
    return imports;
  }
  get moduleAugmentations() {
    if (this._cachedModuleAugmentations !== void 0)
      return this._cachedModuleAugmentations;
    const offset = this.view.getUint32(this.extendedDataOffset + 36, true);
    const moduleAugmentations = this.readNodeIndexArray(offset);
    this._cachedModuleAugmentations = moduleAugmentations;
    return moduleAugmentations;
  }
  get ambientModuleNames() {
    if (this._cachedAmbientModuleNames !== void 0)
      return this._cachedAmbientModuleNames;
    const offset = this.view.getUint32(this.extendedDataOffset + 40, true);
    const names = this.readStringArray(offset);
    this._cachedAmbientModuleNames = names;
    return names;
  }
  get externalModuleIndicator() {
    const nodeIndex = this.view.getUint32(this.extendedDataOffset + 44, true);
    if (nodeIndex === 0)
      return void 0;
    if (nodeIndex === this.index)
      return true;
    return this.getOrCreateNodeAtIndex(nodeIndex);
  }
  get isDeclarationFile() {
    return (this.flags & NodeFlags.Ambient) !== 0;
  }
  get text() {
    if (this._cachedText !== void 0)
      return this._cachedText;
    const text = super.text;
    this._cachedText = text;
    return text;
  }
  // ═══ Line/character position mapping ═══
  getLineStarts() {
    return this._lineStarts ??= computeLineStarts(this.text ?? "");
  }
  getLineAndCharacterOfPosition(position) {
    const lineStarts = this.getLineStarts();
    const line = computeLineOfPosition(lineStarts, position);
    return { line, character: position - lineStarts[line] };
  }
  getPositionOfLineAndCharacter(line, character) {
    const lineStarts = this.getLineStarts();
    if (line < 0 || line >= lineStarts.length) {
      throw new Error(`Bad line number. Line: ${line}, lineStarts.length: ${lineStarts.length}`);
    }
    return lineStarts[line] + character;
  }
};
function computeLineOfPosition(lineStarts, position) {
  let low = 0;
  let high = lineStarts.length - 1;
  while (low <= high) {
    const middle = low + (high - low >> 1);
    const value = lineStarts[middle];
    if (value < position) {
      low = middle + 1;
    } else if (value > position) {
      high = middle - 1;
    } else {
      return middle;
    }
  }
  return low - 1;
}
function parseNodeHandle(handle) {
  const firstDot = handle.indexOf(".");
  if (firstDot === -1) {
    throw new Error(`Invalid node handle: ${handle}`);
  }
  const secondDot = handle.indexOf(".", firstDot + 1);
  if (secondDot === -1) {
    throw new Error(`Invalid node handle: ${handle}`);
  }
  return {
    index: parseInt(handle.slice(0, firstDot), 10),
    kind: parseInt(handle.slice(firstDot + 1, secondDot), 10),
    path: handle.slice(secondDot + 1)
  };
}
function decodeNode(data) {
  const sf = new RemoteSourceFile(data, new Wtf8Decoder());
  return sf;
}
function getNodeId(node) {
  if (!(node instanceof RemoteNode)) {
    throw new Error("getNodeId requires a RemoteNode");
  }
  return node.id;
}

// node_modules/typescript/dist/api/path.js
var CharacterCodesSlash = "/".charCodeAt(0);
var CharacterCodesBackslash = "\\".charCodeAt(0);
var CharacterCodesColon = ":".charCodeAt(0);
var CharacterCodesPercent = "%".charCodeAt(0);
var CharacterCodes3 = "3".charCodeAt(0);
var CharacterCodesa = "a".charCodeAt(0);
var CharacterCodesz = "z".charCodeAt(0);
var CharacterCodesA = "A".charCodeAt(0);
var CharacterCodesZ = "Z".charCodeAt(0);
var CharacterCodesDot = ".".charCodeAt(0);
var directorySeparator = "/";
var altDirectorySeparator = "\\";
var urlSchemeSeparator = "://";
var backslashRegExp = /\\/g;
var relativePathSegmentRegExp = /\/\/|(?:^|\/)\.\.?(?:$|\/)/;
function isAnyDirectorySeparator(charCode) {
  return charCode === CharacterCodesSlash || charCode === CharacterCodesBackslash;
}
function isVolumeCharacter(charCode) {
  return charCode >= CharacterCodesa && charCode <= CharacterCodesz || charCode >= CharacterCodesA && charCode <= CharacterCodesZ;
}
function getFileUrlVolumeSeparatorEnd(url, start) {
  const ch0 = url.charCodeAt(start);
  if (ch0 === CharacterCodesColon)
    return start + 1;
  if (ch0 === CharacterCodesPercent && url.charCodeAt(start + 1) === CharacterCodes3) {
    const ch2 = url.charCodeAt(start + 2);
    if (ch2 === CharacterCodesa || ch2 === CharacterCodesA)
      return start + 3;
  }
  return -1;
}
function getRootLength(path) {
  const rootLength = getEncodedRootLength(path);
  return rootLength < 0 ? ~rootLength : rootLength;
}
function getEncodedRootLength(path) {
  if (!path)
    return 0;
  const ch0 = path.charCodeAt(0);
  if (ch0 === CharacterCodesSlash || ch0 === CharacterCodesBackslash) {
    if (path.charCodeAt(1) !== ch0)
      return 1;
    const p1 = path.indexOf(ch0 === CharacterCodesSlash ? directorySeparator : altDirectorySeparator, 2);
    if (p1 < 0)
      return path.length;
    return p1 + 1;
  }
  if (isVolumeCharacter(ch0) && path.charCodeAt(1) === CharacterCodesColon) {
    const ch2 = path.charCodeAt(2);
    if (ch2 === CharacterCodesSlash || ch2 === CharacterCodesBackslash)
      return 3;
    if (path.length === 2)
      return 2;
  }
  const schemeEnd = path.indexOf(urlSchemeSeparator);
  if (schemeEnd !== -1) {
    const authorityStart = schemeEnd + urlSchemeSeparator.length;
    const authorityEnd = path.indexOf(directorySeparator, authorityStart);
    if (authorityEnd !== -1) {
      const scheme = path.slice(0, schemeEnd);
      const authority = path.slice(authorityStart, authorityEnd);
      if (scheme === "file" && (authority === "" || authority === "localhost") && isVolumeCharacter(path.charCodeAt(authorityEnd + 1))) {
        const volumeSeparatorEnd = getFileUrlVolumeSeparatorEnd(path, authorityEnd + 2);
        if (volumeSeparatorEnd !== -1) {
          if (path.charCodeAt(volumeSeparatorEnd) === CharacterCodesSlash) {
            return ~(volumeSeparatorEnd + 1);
          }
          if (volumeSeparatorEnd === path.length) {
            return ~volumeSeparatorEnd;
          }
        }
      }
      return ~(authorityEnd + 1);
    }
    return ~path.length;
  }
  return 0;
}
function hasTrailingDirectorySeparator(path) {
  return path.length > 0 && isAnyDirectorySeparator(path.charCodeAt(path.length - 1));
}
function removeTrailingDirectorySeparator(path) {
  if (hasTrailingDirectorySeparator(path)) {
    return path.substr(0, path.length - 1);
  }
  return path;
}
function ensureTrailingDirectorySeparator(path) {
  if (!hasTrailingDirectorySeparator(path)) {
    return path + directorySeparator;
  }
  return path;
}
function normalizeSlashes(path) {
  return path.includes("\\") ? path.replace(backslashRegExp, directorySeparator) : path;
}
function combinePaths(path, ...paths) {
  if (path)
    path = normalizeSlashes(path);
  for (let relativePath of paths) {
    if (!relativePath)
      continue;
    relativePath = normalizeSlashes(relativePath);
    if (!path || getRootLength(relativePath) !== 0) {
      path = relativePath;
    } else {
      path = ensureTrailingDirectorySeparator(path) + relativePath;
    }
  }
  return path;
}
function simpleNormalizePath(path) {
  if (!relativePathSegmentRegExp.test(path)) {
    return path;
  }
  let simplified = path.replace(/\/\.\//g, "/");
  if (simplified.startsWith("./")) {
    simplified = simplified.slice(2);
  }
  if (simplified !== path) {
    path = simplified;
    if (!relativePathSegmentRegExp.test(path)) {
      return path;
    }
  }
  return void 0;
}
function getNormalizedAbsolutePath(path, currentDirectory) {
  let rootLength = getRootLength(path);
  if (rootLength === 0 && currentDirectory) {
    path = combinePaths(currentDirectory, path);
    rootLength = getRootLength(path);
  } else {
    path = normalizeSlashes(path);
  }
  const simpleNormalized = simpleNormalizePath(path);
  if (simpleNormalized !== void 0) {
    return simpleNormalized.length > rootLength ? removeTrailingDirectorySeparator(simpleNormalized) : simpleNormalized;
  }
  const length = path.length;
  const root = path.substring(0, rootLength);
  let normalized;
  let index = rootLength;
  let segmentStart = index;
  let normalizedUpTo = index;
  let seenNonDotDotSegment = rootLength !== 0;
  while (index < length) {
    segmentStart = index;
    let ch = path.charCodeAt(index);
    while (ch === CharacterCodesSlash && index + 1 < length) {
      index++;
      ch = path.charCodeAt(index);
    }
    if (index > segmentStart) {
      normalized ??= path.substring(0, segmentStart - 1);
      segmentStart = index;
    }
    let segmentEnd = path.indexOf(directorySeparator, index + 1);
    if (segmentEnd === -1) {
      segmentEnd = length;
    }
    const segmentLength = segmentEnd - segmentStart;
    if (segmentLength === 1 && path.charCodeAt(index) === CharacterCodesDot) {
      normalized ??= path.substring(0, normalizedUpTo);
    } else if (segmentLength === 2 && path.charCodeAt(index) === CharacterCodesDot && path.charCodeAt(index + 1) === CharacterCodesDot) {
      if (!seenNonDotDotSegment) {
        if (normalized !== void 0) {
          normalized += normalized.length === rootLength ? ".." : "/..";
        } else {
          normalizedUpTo = index + 2;
        }
      } else if (normalized === void 0) {
        if (normalizedUpTo - 2 >= 0) {
          normalized = path.substring(0, Math.max(rootLength, path.lastIndexOf(directorySeparator, normalizedUpTo - 2)));
        } else {
          normalized = path.substring(0, normalizedUpTo);
        }
      } else {
        const lastSlash = normalized.lastIndexOf(directorySeparator);
        if (lastSlash !== -1) {
          normalized = normalized.substring(0, Math.max(rootLength, lastSlash));
        } else {
          normalized = root;
        }
        if (normalized.length === rootLength) {
          seenNonDotDotSegment = rootLength !== 0;
        }
      }
    } else if (normalized !== void 0) {
      if (normalized.length !== rootLength) {
        normalized += directorySeparator;
      }
      seenNonDotDotSegment = true;
      normalized += path.substring(segmentStart, segmentEnd);
    } else {
      seenNonDotDotSegment = true;
      normalizedUpTo = segmentEnd;
    }
    index = segmentEnd + 1;
  }
  return normalized ?? (length > rootLength ? removeTrailingDirectorySeparator(path) : path);
}
function normalizePath(path) {
  path = normalizeSlashes(path);
  let normalized = simpleNormalizePath(path);
  if (normalized !== void 0) {
    return normalized;
  }
  normalized = getNormalizedAbsolutePath(path, "");
  return normalized && hasTrailingDirectorySeparator(path) ? ensureTrailingDirectorySeparator(normalized) : normalized;
}
function isRootedDiskPath(path) {
  return getEncodedRootLength(path) > 0;
}
function toPath(fileName, basePath, getCanonicalFileName) {
  const nonCanonicalizedPath = isRootedDiskPath(fileName) ? normalizePath(fileName) : getNormalizedAbsolutePath(fileName, basePath);
  return getCanonicalFileName(nonCanonicalizedPath);
}
function createGetCanonicalFileName(useCaseSensitiveFileNames) {
  return useCaseSensitiveFileNames ? identity : toLowerCase;
}
function identity(x) {
  return x;
}
function toLowerCase(s) {
  return s.toLowerCase();
}
var bundledScheme = "bundled:///";
function isBundled(path) {
  return path.startsWith(bundledScheme);
}
function isDynamicFileName(fileName) {
  return fileName.startsWith("^/");
}
function splitVolumePath(path) {
  if (path.length >= 2 && isVolumeCharacter(path.charCodeAt(0)) && path.charCodeAt(1) === CharacterCodesColon) {
    return [path.substring(0, 2).toLowerCase(), path.substring(2), true];
  }
  return ["", path, false];
}
var extraEscapeChars = {
  ":": "%3A",
  "/": "%2F",
  "?": "%3F",
  "#": "%23",
  "[": "%5B",
  "]": "%5D",
  "@": "%40",
  "!": "%21",
  "$": "%24",
  "&": "%26",
  "'": "%27",
  "(": "%28",
  ")": "%29",
  "*": "%2A",
  "+": "%2B",
  ",": "%2C",
  ";": "%3B",
  "=": "%3D",
  " ": "%20"
};
function extraEscape(s) {
  let result = s;
  for (const [char, escape] of Object.entries(extraEscapeChars)) {
    result = result.replaceAll(char, escape);
  }
  return result;
}
function fileNameToDocumentURI(fileName) {
  if (isBundled(fileName)) {
    return fileName;
  }
  if (isDynamicFileName(fileName)) {
    const withoutPrefix = fileName.substring(2);
    const firstSlash = withoutPrefix.indexOf("/");
    if (firstSlash === -1) {
      throw new Error("invalid file name: " + fileName);
    }
    const scheme = withoutPrefix.substring(0, firstSlash);
    const rest2 = withoutPrefix.substring(firstSlash + 1);
    const secondSlash = rest2.indexOf("/");
    if (secondSlash === -1) {
      throw new Error("invalid file name: " + fileName);
    }
    const authority = rest2.substring(0, secondSlash);
    const path = rest2.substring(secondSlash + 1);
    if (authority === "ts-nul-authority") {
      return scheme + ":" + path;
    }
    return scheme + "://" + authority + "/" + path;
  }
  let [volume, rest] = splitVolumePath(fileName);
  if (volume !== "") {
    volume = "/" + extraEscape(volume);
  }
  if (rest.startsWith("//")) {
    rest = rest.substring(2);
  }
  const parts = rest.split("/");
  const encodedParts = parts.map((part) => extraEscape(encodeURIComponent(part)));
  return "file://" + volume + encodedParts.join("/");
}
function documentURIToFileName(uri) {
  if (isBundled(uri)) {
    return uri;
  }
  if (uri.startsWith("file://")) {
    let parsed;
    try {
      parsed = new URL(uri);
    } catch {
      throw new Error("invalid file URI: " + uri);
    }
    if (parsed.host !== "") {
      return "//" + parsed.host + parsed.pathname;
    }
    const path2 = decodeURIComponent(parsed.pathname);
    if (path2.length >= 3 && path2.charCodeAt(0) === CharacterCodesSlash) {
      const [volume, rest, ok] = splitVolumePath(path2.substring(1));
      if (ok) {
        return volume + rest;
      }
    }
    return path2;
  }
  const colonIndex = uri.indexOf(":");
  if (colonIndex === -1) {
    throw new Error("invalid URI: " + uri);
  }
  const scheme = uri.substring(0, colonIndex);
  let path = uri.substring(colonIndex + 1);
  let authority = "ts-nul-authority";
  if (path.startsWith("//")) {
    const rest = path.substring(2);
    const slashIndex = rest.indexOf("/");
    if (slashIndex === -1) {
      throw new Error("invalid URI: " + uri);
    }
    authority = rest.substring(0, slashIndex);
    path = rest.substring(slashIndex + 1);
  }
  return "^/" + scheme + "/" + authority + "/" + path;
}

// node_modules/typescript/dist/api/proto.js
function resolveFileName(identifier) {
  if (typeof identifier === "string") {
    return identifier;
  }
  return documentURIToFileName(identifier.uri);
}
function toUpdateSnapshotRequest(params) {
  const { openProject, openProjects, ...rest } = params ?? {};
  const mergedOpenProjects = openProject !== void 0 ? [resolveFileName(openProject), ...openProjects ?? []] : openProjects;
  return {
    ...rest,
    ...mergedOpenProjects !== void 0 ? { openProjects: mergedOpenProjects } : {}
  };
}

// node_modules/typescript/dist/api/sourceFileCache.js
function refKey(snapshotId, projectId) {
  return `${snapshotId}:${projectId}`;
}
var SourceFileCache = class {
  /** Map from path to all cached versions of that file */
  cache = /* @__PURE__ */ new Map();
  /** Map from snapshotId to (projectId → Set of paths fetched through that project) */
  snapshotProjectPaths = /* @__PURE__ */ new Map();
  /**
   * Get a cached source file already retained for the given (snapshot, project) pair.
   * This does not require a content hash or parse options key — it returns the entry
   * if one exists with a matching ref. Used to skip the server request entirely when
   * retainForSnapshot has already carried over the ref.
   *
   * A given (snapshot, project) pair always parses a file the same way, so there is
   * at most one matching entry per ref.
   */
  getRetained(path, snapshotId, projectId) {
    const entries = this.cache.get(path);
    if (!entries)
      return void 0;
    const key = refKey(snapshotId, projectId);
    const entry = entries.find((e) => e.refs.has(key));
    return entry?.file;
  }
  /**
   * Store a source file in the cache and retain it for the given (snapshot, project) pair.
   * Returns the cached file — which may be an existing entry if the hash matches.
   */
  set(path, file, parseOptionsKey, contentHash, snapshotId, projectId) {
    let entries = this.cache.get(path);
    if (!entries) {
      entries = [];
      this.cache.set(path, entries);
    }
    const ref = refKey(snapshotId, projectId);
    const existing = entries.find((e) => e.parseOptionsKey === parseOptionsKey && e.contentHash === contentHash);
    if (existing) {
      existing.refs.add(ref);
      this.trackPath(snapshotId, projectId, path);
      return existing.file;
    }
    entries.push({ file, contentHash, parseOptionsKey, refs: /* @__PURE__ */ new Set([ref]) });
    this.trackPath(snapshotId, projectId, path);
    return file;
  }
  /**
   * Retain cache entries from a previous snapshot for a new snapshot.
   * For each project in the previous snapshot:
   *   - Removed projects: skip (don't retain any refs).
   *   - Changed projects: retain refs for files not listed in changedFiles/deletedFiles.
   *   - Unchanged projects: retain all refs.
   */
  retainForSnapshot(newSnapshotId, previousSnapshotId, changes) {
    const prevProjectMap = this.snapshotProjectPaths.get(previousSnapshotId);
    if (!prevProjectMap)
      return;
    const removedProjects = new Set(changes?.removedProjects ?? []);
    const changedProjects = changes?.changedProjects ?? {};
    for (const [projectId, paths] of prevProjectMap) {
      if (removedProjects.has(projectId))
        continue;
      const projectChanges = changedProjects[projectId];
      let invalidPaths;
      if (projectChanges) {
        invalidPaths = /* @__PURE__ */ new Set();
        for (const p of projectChanges.changedFiles ?? [])
          invalidPaths.add(p);
        for (const p of projectChanges.deletedFiles ?? [])
          invalidPaths.add(p);
      }
      const prevRef = refKey(previousSnapshotId, projectId);
      const newRef = refKey(newSnapshotId, projectId);
      for (const path of paths) {
        if (invalidPaths?.has(path))
          continue;
        const entries = this.cache.get(path);
        if (!entries)
          continue;
        for (const entry of entries) {
          if (entry.refs.has(prevRef)) {
            entry.refs.add(newRef);
            this.trackPath(newSnapshotId, projectId, path);
          }
        }
      }
    }
  }
  /**
   * Release all entries retained by the given snapshot across all projects.
   * Only visits paths that the snapshot actually referenced.
   * Entries with no remaining refs are evicted.
   */
  releaseSnapshot(snapshotId) {
    const projectMap = this.snapshotProjectPaths.get(snapshotId);
    if (!projectMap)
      return;
    for (const [projectId, paths] of projectMap) {
      const key = refKey(snapshotId, projectId);
      for (const path of paths) {
        const entries = this.cache.get(path);
        if (!entries)
          continue;
        for (let i = entries.length - 1; i >= 0; i--) {
          entries[i].refs.delete(key);
          if (entries[i].refs.size === 0) {
            entries.splice(i, 1);
          }
        }
        if (entries.length === 0) {
          this.cache.delete(path);
        }
      }
    }
    this.snapshotProjectPaths.delete(snapshotId);
  }
  trackPath(snapshotId, projectId, path) {
    let projectMap = this.snapshotProjectPaths.get(snapshotId);
    if (!projectMap) {
      projectMap = /* @__PURE__ */ new Map();
      this.snapshotProjectPaths.set(snapshotId, projectMap);
    }
    let paths = projectMap.get(projectId);
    if (!paths) {
      paths = /* @__PURE__ */ new Set();
      projectMap.set(projectId, paths);
    }
    paths.add(path);
  }
  /**
   * Clear all entries from the cache.
   */
  clear() {
    this.cache.clear();
    this.snapshotProjectPaths.clear();
  }
  /**
   * Get the number of unique paths in the cache.
   */
  get size() {
    return this.cache.size;
  }
  /**
   * Check if a path is in the cache.
   */
  has(path) {
    return this.cache.has(path);
  }
};

// node_modules/typescript/dist/api/fs.js
var fsCallbackNames = ["readFile", "fileExists", "directoryExists", "getAccessibleEntries", "realpath"];

// lekalo-no-ambient:no-ambient-compiler
function no_ambient_compiler_default() {
  throw new Error("an explicit verified compiler resource is required");
}

// node_modules/typescript/dist/api/options.js
function isSpawnOptions(options) {
  return !("pipe" in options);
}
function resolveExePath(options) {
  return options.tsserverPath ?? no_ambient_compiler_default();
}

// node_modules/typescript/dist/api/syncChannel.js
import { closeSync, openSync, readSync, writeSync } from "node:fs";
var MSG_REQUEST = 1;
var MSG_CALL_RESPONSE = 2;
var MSG_CALL_ERROR = 3;
var MSG_RESPONSE = 4;
var MSG_ERROR = 5;
var MSG_CALL = 6;
var sleepBuf = new Int32Array(new SharedArrayBuffer(4));
var EMPTY_BUF = Buffer.alloc(0);
var liveChildren = /* @__PURE__ */ new Set();
process.on("exit", () => {
  for (const child of liveChildren) {
    try {
      child.kill();
    } catch {
    }
  }
  liveChildren.clear();
});
var SyncRpcChannel = class {
  child;
  readFd;
  writeFd;
  pipeFd;
  callbacks = /* @__PURE__ */ new Map();
  methodBufCache = /* @__PURE__ */ new Map();
  // When true, the payload byte lengths of each request/response are recorded
  // and exposed via the `last*` fields below. These count only the JSON/binary
  // payload bytes, not the MessagePack tuple framing (message type, method
  // name, and length headers).
  collectTiming;
  // Per-request payload byte measurements for the most recently completed
  // request. Only meaningful when `collectTiming` is true. Callers read these
  // immediately after a request returns (the channel is strictly serial).
  lastBytesSent = 0;
  lastBytesReceived = 0;
  _msgType = 0;
  _msgName = EMPTY_BUF;
  _msgPayload = EMPTY_BUF;
  headerBuf = Buffer.allocUnsafe(4);
  // Read-ahead buffer – reduces readSync syscalls by buffering data from the pipe.
  readBuf = Buffer.allocUnsafe(65536);
  readBufPos = 0;
  readBufLen = 0;
  // Write buffer – assembles entire tuples for a single writeSync.
  writeBuf = Buffer.allocUnsafe(65536);
  constructor(exe, args, collectTiming = false) {
    this.collectTiming = collectTiming;
    this.readFd = 0;
    this.writeFd = 1;
    this.lekaloPeer = true;
    this.child = { exitCode: null, kill() {
    }, unref() {
    } };
  }
  // ── Public API ──────────────────────────────────────────────────
  /**
   * Send a request and synchronously wait for the response (string).
   * Handles Call (callback) messages from the child inline.
   */
  requestSync(method, payload) {
    this.ensureOpen();
    const result = this.requestBytesSync(method, payload);
    return result.toString("utf-8");
  }
  /**
   * Send a request and synchronously wait for the response (binary).
   * Handles Call (callback) messages from the child inline.
   */
  requestBinarySync(method, payload) {
    this.ensureOpen();
    return this.requestBytesSync(method, payload);
  }
  /** Register a string→string callback that the child may invoke. */
  registerCallback(name, callback) {
    this.callbacks.set(name, callback);
  }
  /** Release the client session; core owns compiler termination. */
  close() {
    if (this.lekaloPeer) {
      this.closed = true;
      return;
    }
    try {
      liveChildren.delete(this.child);
      if (this.pipeFd !== void 0) {
        closeSync(this.pipeFd);
        this.pipeFd = void 0;
      }
      this.child.stdout?.destroy();
      this.child.stdin?.destroy();
      this.child.kill();
      this.readFd = -1;
      this.writeFd = -1;
    } catch {
    }
  }
  // ── Core request loop ───────────────────────────────────────────
  ensureOpen() {
    if (this.readFd < 0) {
      throw new Error("SyncRpcChannel is closed");
    }
  }
  getMethodBuf(method) {
    let buf = this.methodBufCache.get(method);
    if (buf === void 0) {
      buf = Buffer.from(method, "utf-8");
      this.methodBufCache.set(method, buf);
    }
    return buf;
  }
  requestBytesSync(method, payload) {
    const methodBuf = this.getMethodBuf(method);
    if (this.collectTiming) {
      this.lastBytesSent = typeof payload === "string" ? Buffer.byteLength(payload, "utf-8") : payload.length;
      this.lastBytesReceived = 0;
    }
    this.writeTuple(MSG_REQUEST, methodBuf, payload);
    for (; ; ) {
      this.readTuple();
      switch (this._msgType) {
        case MSG_RESPONSE: {
          if (!methodBuf.equals(this._msgName)) {
            throw new Error(`name mismatch for response: expected \`${method}\`, got \`${this._msgName.toString("utf-8")}\``);
          }
          if (this.collectTiming) {
            this.lastBytesReceived = this._msgPayload.length;
          }
          return this._msgPayload;
        }
        case MSG_ERROR: {
          if (methodBuf.equals(this._msgName)) {
            throw new Error(this._msgPayload.toString("utf-8"));
          }
          throw new Error(`name mismatch for response: expected \`${method}\`, got \`${this._msgName.toString("utf-8")}\``);
        }
        case MSG_CALL: {
          this.handleCall(this._msgName.toString("utf-8"), this._msgPayload);
          break;
        }
        default:
          throw new Error(`Invalid message type from child: ${this._msgType}`);
      }
    }
  }
  // ── Callback handling ───────────────────────────────────────────
  /**
   * Handle an incoming MSG_CALL from the child process.
   *
   * After sending the error response back to the child, this method
   * intentionally re-throws to abort the caller's request loop.
   * A failed callback is treated as unrecoverable to match the
   * behavior of the native libsyncrpc addon.
   */
  handleCall(name, payload) {
    const cb = this.callbacks.get(name);
    if (!cb) {
      const errMsg = `unknown callback: \`${name}\`. Please make sure to register it on the JavaScript side before invoking it.`;
      this.writeTuple(MSG_CALL_ERROR, Buffer.from(name, "utf-8"), Buffer.from(errMsg, "utf-8"));
      throw new Error(`no callback named \`${name}\` found`);
    }
    try {
      const result = cb(name, payload.toString("utf-8"));
      this.writeTuple(MSG_CALL_RESPONSE, Buffer.from(name, "utf-8"), Buffer.from(result, "utf-8"));
    } catch (e) {
      const errMsg = String(e instanceof Error ? e.message : e).trim();
      this.writeTuple(MSG_CALL_ERROR, Buffer.from(name, "utf-8"), Buffer.from(errMsg, "utf-8"));
      throw new Error(`Error calling callback \`${name}\`: ${errMsg}`);
    }
  }
  // ── MessagePack tuple write ─────────────────────────────────────
  /**
   * Write a complete [type, name, payload] tuple in as few writeSync
   * calls as possible.  For messages that fit in the pre-allocated
   * write buffer (64 KB), everything is assembled and sent in a single
   * syscall.  Larger messages use two syscalls: one for the header
   * portion and one for the payload data.
   */
  writeTuple(type, name, payload) {
    const nameLen = name.length;
    const payloadIsString = typeof payload === "string";
    const payloadLen = payloadIsString ? Buffer.byteLength(payload, "utf-8") : payload.length;
    const nameHdrSize = binHeaderSize(nameLen);
    const payloadHdrSize = binHeaderSize(payloadLen);
    const headerSize = 2 + nameHdrSize + nameLen + payloadHdrSize;
    const totalSize = headerSize + payloadLen;
    if (totalSize > 67108864) throw new Error("compiler IPC frame bound");
    if (totalSize <= this.writeBuf.length) {
      let off = 0;
      this.writeBuf[off++] = MSGPACK_FIXARRAY3;
      this.writeBuf[off++] = type;
      off = writeBinHeader(this.writeBuf, off, nameLen);
      name.copy(this.writeBuf, off);
      off += nameLen;
      off = writeBinHeader(this.writeBuf, off, payloadLen);
      if (payloadLen > 0) {
        if (payloadIsString) {
          this.writeBuf.write(payload, off, payloadLen, "utf-8");
        } else if (payload instanceof Buffer) {
          payload.copy(this.writeBuf, off);
        } else {
          this.writeBuf.set(payload, off);
        }
      }
      this.writeAllBuf(this.writeBuf, totalSize);
    } else {
      let off = 0;
      this.writeBuf[off++] = MSGPACK_FIXARRAY3;
      this.writeBuf[off++] = type;
      off = writeBinHeader(this.writeBuf, off, nameLen);
      name.copy(this.writeBuf, off);
      off += nameLen;
      off = writeBinHeader(this.writeBuf, off, payloadLen);
      this.writeAllBuf(this.writeBuf, off);
      if (payloadLen > 0) {
        if (payloadIsString) {
          this.writeAllBuf(Buffer.from(payload, "utf-8"));
        } else {
          this.writeAllBuf(payload);
        }
      }
    }
  }
  // ── MessagePack tuple read ──────────────────────────────────────
  /**
   * Read a [type, name, payload] tuple into instance fields
   * (_msgType, _msgName, _msgPayload) to avoid allocating a
   * short-lived 3-element array on every call.
   */
  readTuple() {
    const marker = this.readByte();
    if (marker !== MSGPACK_FIXARRAY3) {
      throw new Error(`Expected fixed 3-element array (0x93), received: 0x${marker.toString(16)}`);
    }
    const tb = this.readByte();
    if (tb <= 127) {
      this._msgType = tb;
    } else if (tb === MSGPACK_UINT8) {
      this._msgType = this.readByte();
    } else {
      throw new Error(`Expected positive fixint or uint8 marker, received: 0x${tb.toString(16)}`);
    }
    this._msgName = this.readBin();
    this._msgPayload = this.readBin();
  }
  /**
   * Read a MessagePack bin field.
   */
  readBin() {
    const marker = this.readByte();
    let size;
    switch (marker) {
      case MSGPACK_BIN8:
        size = this.readByte();
        break;
      case MSGPACK_BIN16:
        this.readExactInto(this.headerBuf, 2);
        size = this.headerBuf[0] << 8 | this.headerBuf[1];
        break;
      case MSGPACK_BIN32:
        this.readExactInto(this.headerBuf, 4);
        size = this.headerBuf.readUInt32BE(0);
        break;
      default:
        throw new Error(`Expected binary data (0xc4-0xc6), received: 0x${marker.toString(16)}`);
    }
    if (size === 0)
      return EMPTY_BUF;
    return this.readExact(size);
  }
  // ── Low-level synchronous I/O ───────────────────────────────────
  /** Build an EOF error with the child's exit code/signal if available. */
  eofError() {
    const code = this.child.exitCode;
    const signal = this.child.signalCode;
    const detail = signal ? `killed by signal ${signal}` : code !== null ? `exited with code ${code}` : "unknown reason";
    return new Error(`Unexpected EOF while reading from child process (${detail})`);
  }
  /** Read a single byte from the buffered read-ahead. */
  readByte() {
    if (this.readBufPos >= this.readBufLen) {
      this.fillReadBuffer();
    }
    return this.readBuf[this.readBufPos++];
  }
  readExact(length) {
    if (!Number.isSafeInteger(length) || length < 0 || length > 67108864) throw new Error("compiler IPC frame bound");
    const buf = Buffer.allocUnsafeSlow(length);
    this.readExactInto(buf, length);
    return buf;
  }
  /**
   * Fill the internal read-ahead buffer from the pipe fd.
   * Retries on EAGAIN for non-blocking mode compatibility.
   */
  fillReadBuffer() {
    this.readBufPos = 0;
    this.readBufLen = 0;
    for (; ; ) {
      try {
        const n = readSync(this.readFd, this.readBuf, 0, this.readBuf.length, null);
        if (n === 0) {
          throw this.eofError();
        }
        this.readBufLen = n;
        return;
      } catch (e) {
        if (e instanceof Error && "code" in e && (e.code === "EAGAIN" || e.code === "EWOULDBLOCK")) {
          Atomics.wait(sleepBuf, 0, 0, 1);
          continue;
        }
        throw e;
      }
    }
  }
  /**
   * Synchronously read exactly `length` bytes into `buffer`.
   * Serves from the internal read-ahead buffer first; for large reads
   * that exceed the buffer size, reads directly from the fd to avoid
   * an extra copy.
   */
  readExactInto(buffer, length) {
    let pos = 0;
    while (pos < length) {
      const avail = this.readBufLen - this.readBufPos;
      if (avail > 0) {
        const toCopy = Math.min(avail, length - pos);
        this.readBuf.copy(buffer, pos, this.readBufPos, this.readBufPos + toCopy);
        this.readBufPos += toCopy;
        pos += toCopy;
      } else if (length - pos >= this.readBuf.length) {
        try {
          const n = readSync(this.readFd, buffer, pos, length - pos, null);
          if (n === 0) {
            throw this.eofError();
          }
          pos += n;
        } catch (e) {
          if (e instanceof Error && "code" in e && (e.code === "EAGAIN" || e.code === "EWOULDBLOCK")) {
            Atomics.wait(sleepBuf, 0, 0, 1);
            continue;
          }
          throw e;
        }
      } else {
        this.fillReadBuffer();
      }
    }
  }
  /**
   * Synchronously write all bytes from `data` (up to `length`).
   * Retries on EAGAIN.
   */
  writeAllBuf(data, length) {
    const total = length ?? data.length;
    let pos = 0;
    while (pos < total) {
      try {
        const n = writeSync(this.writeFd, data, pos, total - pos);
        pos += n;
      } catch (e) {
        if (e instanceof Error && "code" in e && (e.code === "EAGAIN" || e.code === "EWOULDBLOCK")) {
          Atomics.wait(sleepBuf, 0, 0, 1);
          continue;
        }
        throw e;
      }
    }
  }
};

// node_modules/typescript/dist/api/timing.js
var RECENT_REQUEST_CAPACITY = 5;
function emptyAccumulators() {
  return {
    requestCount: 0,
    roundTripMs: 0,
    bytesSent: 0,
    bytesReceived: 0,
    serverTimeMs: 0,
    transportOverheadMs: 0,
    nodesMaterialized: 0,
    sourceFilesFetched: 0,
    nodesFetched: 0
  };
}
function disabledTimingInfo() {
  return {
    enabled: false,
    totals: emptyAccumulators(),
    recentRequests: []
  };
}
function combineTimingInfo(client, server) {
  if (!client.enabled) {
    return client;
  }
  const serverTimeMs = server.totals.totalProcessingTimeMs;
  const totals = {
    ...client.totals,
    serverTimeMs,
    transportOverheadMs: Math.max(0, client.totals.roundTripMs - serverTimeMs)
  };
  const recentRequests = client.recentRequests.map((r) => ({ ...r }));
  const serverRecent = server.recentRequests;
  const pairs = Math.min(recentRequests.length, serverRecent.length);
  for (let i = 1; i <= pairs; i++) {
    const c = recentRequests[recentRequests.length - i];
    const s = serverRecent[serverRecent.length - i];
    if (c.method === s.method) {
      c.serverTimeMs = s.processingTimeMs;
      c.transportOverheadMs = Math.max(0, c.roundTripMs - s.processingTimeMs);
    }
  }
  return {
    enabled: true,
    totals,
    recentRequests
  };
}
var TimingCollector = class {
  totals = emptyAccumulators();
  // Ring buffer of the most recent requests. `ring` grows to at most
  // RECENT_REQUEST_CAPACITY; once full, `head` marks the oldest entry.
  ring = [];
  head = 0;
  /** Records a single request's measurements. */
  record(sample) {
    this.totals.requestCount++;
    this.totals.roundTripMs += sample.roundTripMs;
    this.totals.bytesSent += sample.bytesSent;
    this.totals.bytesReceived += sample.bytesReceived;
    const entry = {
      method: sample.method,
      roundTripMs: sample.roundTripMs,
      bytesSent: sample.bytesSent,
      bytesReceived: sample.bytesReceived,
      timestamp: Date.now()
    };
    if (this.ring.length < RECENT_REQUEST_CAPACITY) {
      this.ring.push(entry);
    } else {
      this.ring[this.head] = entry;
      this.head = (this.head + 1) % RECENT_REQUEST_CAPACITY;
    }
  }
  /**
   * Records a single AST node materialization. Called on demand as the consumer
   * walks a binary source-file response's tree, so it is not tied to any one
   * request.
   */
  recordMaterialization() {
    this.totals.nodesMaterialized++;
  }
  /**
   * Records a fetched source file: increments the fetched-file counter and adds
   * the file's materializable node count to the fetched-node total, which serves
   * as the denominator for the share of fetched nodes that end up materialized.
   */
  recordSourceFileFetched(materializableNodeCount) {
    this.totals.sourceFilesFetched++;
    this.totals.nodesFetched += materializableNodeCount;
  }
  /** Returns a snapshot of the collected timing information. */
  getInfo() {
    const recentRequests = [];
    for (let i = 0; i < this.ring.length; i++) {
      recentRequests.push(this.ring[(this.head + i) % this.ring.length]);
    }
    return {
      enabled: true,
      totals: { ...this.totals },
      recentRequests
    };
  }
  /** Clears all accumulated totals and recent-request history. */
  reset() {
    this.totals = emptyAccumulators();
    this.ring = [];
    this.head = 0;
  }
};

// node_modules/typescript/dist/api/sync/client.js
var Client = class {
  channel;
  encoder = new TextEncoder();
  timing;
  constructor(options) {
    if (!isSpawnOptions(options)) {
      throw new Error("Socket connections are not yet supported in the sync client");
    }
    const cwd = options.cwd ?? process.cwd();
    const args = [
      "--api",
      "--cwd",
      cwd
    ];
    const enabledCallbacks = [];
    if (options.fs) {
      for (const name of fsCallbackNames) {
        if (options.fs[name]) {
          enabledCallbacks.push(name);
        }
      }
    }
    if (enabledCallbacks.length > 0) {
      args.push(`--callbacks=${enabledCallbacks.join(",")}`);
    }
    const collectTiming = options.collectTiming ?? false;
    if (collectTiming) {
      args.push("--timing");
      this.timing = new TimingCollector();
    }
    const channel = new SyncRpcChannel(resolveExePath(options), args, collectTiming);
    this.channel = channel;
    if (options.fs) {
      for (const name of enabledCallbacks) {
        const callback = options.fs[name];
        channel.registerCallback(name, (_, arg) => {
          const result = callback(JSON.parse(arg));
          if (name === "readFile") {
            if (result === void 0)
              return "";
            return JSON.stringify({ content: result });
          }
          return JSON.stringify(result) ?? "";
        });
      }
    }
  }
  apiRequest(method, params) {
    const encodedPayload = JSON.stringify(params);
    const start = performance.now();
    const result = this.channel.requestSync(method, encodedPayload);
    this.recordTiming(method, start);
    if (result.length) {
      return JSON.parse(result);
    }
    return void 0;
  }
  apiRequestBinary(method, params) {
    const start = performance.now();
    const result = this.channel.requestBinarySync(method, this.encoder.encode(JSON.stringify(params)));
    this.recordTiming(method, start);
    if (result.length === 0)
      return void 0;
    return result;
  }
  echo(payload) {
    return this.channel.requestSync("echo", payload);
  }
  echoBinary(payload) {
    return this.channel.requestBinarySync("echo", payload);
  }
  /**
   * Returns a combined timing snapshot: client-measured round-trip and byte
   * counts folded together with the server's own per-request processing time
   * (fetched via a getServerTiming request) and estimated transport overhead.
   */
  getTimingInfo() {
    if (!this.timing) {
      return disabledTimingInfo();
    }
    const local = this.timing.getInfo();
    const result = this.channel.requestSync("getServerTiming", "");
    return combineTimingInfo(local, JSON.parse(result));
  }
  resetTimingInfo() {
    if (!this.timing)
      return;
    this.timing.reset();
    this.channel.requestSync("resetServerTiming", "");
  }
  /**
   * Returns the timing collector that per-node materialization is reported
   * into, or undefined when timing collection is disabled. The returned
   * collector is the same one folded into {@link getTimingInfo}, so
   * materialization totals surface alongside request timings.
   */
  getTimingCollector() {
    return this.timing;
  }
  recordTiming(method, start) {
    if (!this.timing)
      return;
    this.timing.record({
      method,
      roundTripMs: performance.now() - start,
      bytesSent: this.channel.lastBytesSent,
      bytesReceived: this.channel.lastBytesReceived
    });
  }
  close() {
    this.channel.close();
  }
};

// node_modules/typescript/dist/api/sync/api.js
var API = class _API {
  client;
  sourceFileCache;
  toPath;
  initialized = false;
  activeSnapshots = /* @__PURE__ */ new Set();
  latestSnapshot;
  internal;
  constructor(options = {}) {
    this.client = new Client(options);
    this.sourceFileCache = new SourceFileCache();
    this.internal = new InternalAPI(this.client, () => this.ensureInitialized());
  }
  /**
   * Create an API instance from an existing LSP connection's API session.
   * Use this when connecting to an API pipe provided by an LSP server via custom/initializeAPISession.
   */
  static fromLSPConnection(options) {
    const api = new _API(options);
    api.ensureInitialized();
    return api;
  }
  ensureInitialized() {
    if (!this.initialized) {
      const response = this.client.apiRequest("initialize", null);
      const getCanonicalFileName = createGetCanonicalFileName(response.useCaseSensitiveFileNames);
      const currentDirectory = response.currentDirectory;
      this.toPath = (fileName) => toPath(fileName, currentDirectory, getCanonicalFileName);
      this.initialized = true;
    }
  }
  parseConfigFile(file) {
    this.ensureInitialized();
    return this.client.apiRequest("parseConfigFile", { file });
  }
  updateSnapshot(params) {
    this.ensureInitialized();
    const requestParams = toUpdateSnapshotRequest(params);
    const data = this.client.apiRequest("updateSnapshot", requestParams);
    if (this.latestSnapshot) {
      this.sourceFileCache.retainForSnapshot(data.snapshot, this.latestSnapshot.id, data.changes);
      if (this.latestSnapshot.isDisposed()) {
        this.sourceFileCache.releaseSnapshot(this.latestSnapshot.id);
      }
    }
    const snapshot = new Snapshot(data, this.client, this.sourceFileCache, this.toPath, () => {
      this.activeSnapshots.delete(snapshot);
      if (snapshot !== this.latestSnapshot) {
        this.sourceFileCache.releaseSnapshot(snapshot.id);
      }
    });
    this.latestSnapshot = snapshot;
    this.activeSnapshots.add(snapshot);
    return snapshot;
  }
  close() {
    for (const snapshot of [...this.activeSnapshots]) {
      snapshot.dispose();
    }
    if (this.latestSnapshot) {
      this.sourceFileCache.releaseSnapshot(this.latestSnapshot.id);
      this.latestSnapshot = void 0;
    }
    this.client.close();
    this.sourceFileCache.clear();
  }
  clearSourceFileCache() {
    this.sourceFileCache.clear();
  }
  /**
   * Returns a snapshot of collected timing information for requests made
   * through this API instance: client-measured round-trip latency and bytes
   * transferred, folded together with the server's own per-request processing
   * time and an estimated transport overhead (round-trip minus server time).
   *
   * Fetching the snapshot issues a lightweight request to the server to
   * retrieve its timing collection. Collection must be enabled via the
   * `collectTiming` option; when it is not, the returned snapshot has
   * `enabled: false` and zeroed totals.
   */
  getTimingInfo() {
    return this.client.getTimingInfo();
  }
  /** Clears all accumulated timing totals and recent-request history, on both the client and the server. */
  resetTimingInfo() {
    return this.client.resetTimingInfo();
  }
};
var InternalAPI = class {
  client;
  ensureInitialized;
  /** @internal */
  constructor(client, ensureInitialized) {
    this.client = client;
    this.ensureInitialized = ensureInitialized;
  }
  startCPUProfile(dir) {
    this.ensureInitialized();
    this.client.apiRequest("startCPUProfile", { dir });
  }
  stopCPUProfile() {
    this.ensureInitialized();
    const result = this.client.apiRequest("stopCPUProfile", null);
    return result.file;
  }
  saveHeapProfile(dir) {
    this.ensureInitialized();
    const result = this.client.apiRequest("saveHeapProfile", { dir });
    return result.file;
  }
};
var Snapshot = class {
  id;
  projectMap;
  toPath;
  client;
  disposed = false;
  onDispose;
  snapshotRegistry;
  constructor(data, client, sourceFileCache, toPath2, onDispose) {
    this.id = data.snapshot;
    this.client = client;
    this.toPath = toPath2;
    this.onDispose = onDispose;
    this.projectMap = /* @__PURE__ */ new Map();
    this.snapshotRegistry = new SnapshotObjectRegistry(client, this.id, (projectId) => this.projectMap.get(projectId));
    for (const projData of data.projects) {
      const project = new Project(projData, this.id, client, sourceFileCache, toPath2, this.snapshotRegistry);
      this.projectMap.set(toPath2(projData.configFileName), project);
    }
  }
  getProjects() {
    this.ensureNotDisposed();
    return [...this.projectMap.values()];
  }
  getProject(configFileName) {
    this.ensureNotDisposed();
    return this.projectMap.get(this.toPath(configFileName));
  }
  getDefaultProjectForFile(file) {
    this.ensureNotDisposed();
    const data = this.client.apiRequest("getDefaultProjectForFile", {
      snapshot: this.id,
      file
    });
    if (!data)
      return void 0;
    return this.projectMap.get(this.toPath(data.configFileName));
  }
  [globalThis.Symbol.dispose]() {
    this.dispose();
  }
  dispose() {
    if (this.disposed)
      return;
    this.disposed = true;
    for (const project of this.projectMap.values()) {
      project.dispose();
    }
    this.projectMap.clear();
    this.snapshotRegistry.clear();
    this.onDispose();
    this.client.apiRequest("release", { snapshot: this.id });
  }
  isDisposed() {
    return this.disposed;
  }
  ensureNotDisposed() {
    if (this.disposed) {
      throw new Error("Snapshot is disposed");
    }
  }
};
var SnapshotObjectRegistry = class {
  symbols = /* @__PURE__ */ new Map();
  client;
  snapshotId;
  resolveProject;
  constructor(client, snapshotId, resolveProject) {
    this.client = client;
    this.snapshotId = snapshotId;
    this.resolveProject = resolveProject;
  }
  /** Resolve a project id (a config file path) to its Project within this snapshot. */
  getProject(projectId) {
    return this.resolveProject(projectId);
  }
  getOrCreateSymbol(data) {
    let symbol = this.symbols.get(data.id);
    if (!symbol) {
      symbol = new Symbol2(data, this);
      this.symbols.set(data.id, symbol);
    }
    return symbol;
  }
  getSymbol(id) {
    return this.symbols.get(id);
  }
  clear() {
    this.symbols.clear();
  }
  fetchSymbol(source, method, handle, projectId) {
    if (!handle)
      return void 0;
    const cached = this.getSymbol(handle);
    if (cached)
      return cached;
    const data = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: projectId,
      objectId: source.id
    });
    if (!data)
      throw new Error(`${method} returned null symbol for ${source.constructor.name} ${source.id}`);
    return this.getOrCreateSymbol(data);
  }
  fetchSymbols(source, method, handles, projectId) {
    if (handles) {
      const result = new Array(handles.length);
      let allCached = true;
      for (let i = 0; i < handles.length; i++) {
        const cached = this.getSymbol(handles[i]);
        if (!cached) {
          allCached = false;
          break;
        }
        result[i] = cached;
      }
      if (allCached)
        return result;
    }
    const symbolData = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: projectId,
      objectId: source.id
    });
    if (symbolData == null)
      return [];
    else
      return symbolData.map((data) => this.getOrCreateSymbol(data));
  }
};
var ProjectObjectRegistry = class {
  client;
  snapshotId;
  project;
  snapshotRegistry;
  types = /* @__PURE__ */ new Map();
  signatures = /* @__PURE__ */ new Map();
  constructor(client, snapshotId, project, snapshotRegistry) {
    this.client = client;
    this.snapshotId = snapshotId;
    this.project = project;
    this.snapshotRegistry = snapshotRegistry;
  }
  getOrCreateSymbol(data) {
    return this.snapshotRegistry.getOrCreateSymbol(data);
  }
  getSymbol(id) {
    return this.snapshotRegistry.getSymbol(id);
  }
  getOrCreateType(data) {
    let type = this.types.get(data.id);
    if (!type) {
      type = new TypeObject(data, this);
      this.types.set(data.id, type);
    }
    return type;
  }
  getType(id) {
    return this.types.get(id);
  }
  getOrCreateSignature(data) {
    let sig = this.signatures.get(data.id);
    if (!sig) {
      sig = new Signature(data, this.project, this);
      this.signatures.set(data.id, sig);
    }
    return sig;
  }
  getSignature(id) {
    return this.signatures.get(id);
  }
  clear() {
    this.types.clear();
    this.signatures.clear();
  }
  fetchType(source, method, handle) {
    if (handle !== false) {
      if (!handle)
        return void 0;
      const cached = this.getType(handle);
      if (cached)
        return cached;
    }
    const data = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: this.project.id,
      objectId: source.id
    });
    if (!data)
      throw new Error(`${method} returned null type for ${source.constructor.name} ${source.id}`);
    return this.getOrCreateType(data);
  }
  fetchSymbol(source, method, handle) {
    return this.snapshotRegistry.fetchSymbol(source, method, handle, this.project.id);
  }
  fetchSignature(source, method, handle) {
    if (!handle)
      return void 0;
    const cached = this.getSignature(handle);
    if (cached)
      return cached;
    const data = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: this.project.id,
      objectId: source.id
    });
    if (!data)
      throw new Error(`${method} returned null signature for ${source.constructor.name} ${source.id}`);
    return this.getOrCreateSignature(data);
  }
  fetchTypes(source, method, handles) {
    if (handles) {
      const result = new Array(handles.length);
      let allCached = true;
      for (let i = 0; i < handles.length; i++) {
        const cached = this.getType(handles[i]);
        if (!cached) {
          allCached = false;
          break;
        }
        result[i] = cached;
      }
      if (allCached)
        return result;
    }
    const typesData = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: this.project.id,
      objectId: source.id
    });
    if (typesData == null)
      return [];
    else
      return typesData.map((data) => this.getOrCreateType(data));
  }
  fetchSymbols(source, method, handles) {
    return this.snapshotRegistry.fetchSymbols(source, method, handles, this.project.id);
  }
  // getBaseTypes is a checker-level endpoint keyed by `type` (not `objectId`),
  // so it cannot go through fetchTypes. This helper reuses that server method.
  fetchBaseTypes(source) {
    const typesData = this.client.apiRequest("getBaseTypes", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: source.id
    });
    if (typesData == null)
      return [];
    return typesData.map((data) => this.getOrCreateType(data));
  }
};
var Project = class {
  id;
  configFileName;
  compilerOptions;
  rootFiles;
  program;
  checker;
  emitter;
  client;
  constructor(data, snapshotId, client, sourceFileCache, toPath2, snapshotRegistry) {
    this.id = data.id;
    this.configFileName = data.configFileName;
    this.compilerOptions = data.compilerOptions;
    this.rootFiles = data.rootFiles;
    this.client = client;
    this.program = new Program(snapshotId, this, client, sourceFileCache, toPath2);
    const objectRegistry = new ProjectObjectRegistry(client, snapshotId, this, snapshotRegistry);
    this.checker = new Checker(snapshotId, this, client, objectRegistry);
    this.emitter = new Emitter(client);
  }
  dispose() {
    this.checker.dispose();
  }
};
var Program = class {
  snapshotId;
  project;
  client;
  sourceFileCache;
  toPath;
  decoder = new Wtf8Decoder();
  sourceFileMetadataCache = /* @__PURE__ */ new Map();
  constructor(snapshotId, project, client, sourceFileCache, toPath2) {
    this.snapshotId = snapshotId;
    this.project = project;
    this.client = client;
    this.sourceFileCache = sourceFileCache;
    this.toPath = toPath2;
  }
  getCompilerOptions() {
    return this.project.compilerOptions;
  }
  getSourceFile(file) {
    const fileName = resolveFileName(file);
    const path = this.toPath(fileName);
    const retained = this.sourceFileCache.getRetained(path, this.snapshotId, this.project.id);
    if (retained) {
      return retained;
    }
    const binaryData = this.client.apiRequestBinary("getSourceFile", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file
    });
    if (!binaryData) {
      return void 0;
    }
    const view = new DataView(binaryData.buffer, binaryData.byteOffset, binaryData.byteLength);
    const contentHash = readSourceFileHash(view);
    const parseOptionsKey = readParseOptionsKey(view);
    const sourceFile = new RemoteSourceFile(binaryData, this.decoder, this.client.getTimingCollector());
    return this.sourceFileCache.set(path, sourceFile, parseOptionsKey, contentHash, this.snapshotId, this.project.id);
  }
  getSourceFileNames() {
    const data = this.client.apiRequest("getSourceFileNames", {
      snapshot: this.snapshotId,
      project: this.project.id
    });
    return data ?? [];
  }
  /**
   * Returns program-stored metadata for the given source file, or `undefined` if the file
   * is not part of the program. Metadata is fetched lazily per file and cached on this
   * `Program` instance.
   */
  getSourceFileMetadata(fileName) {
    return this.getSourceFileMetadataByPath(this.toPath(fileName));
  }
  /**
   * Returns program-stored metadata for the source file at the given path, or `undefined`
   * if the file is not part of the program. Like {@link getSourceFileMetadata}, but skips
   * the file name to path conversion. Metadata is fetched lazily per file and cached on
   * this `Program` instance.
   */
  getSourceFileMetadataByPath(path) {
    let metadata = this.sourceFileMetadataCache.get(path);
    if (metadata === void 0) {
      metadata = this.fetchSourceFileMetadata(path);
      this.sourceFileMetadataCache.set(path, metadata);
    }
    return metadata;
  }
  fetchSourceFileMetadata(path) {
    const data = this.client.apiRequest("getSourceFileMetadata", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file: path
    });
    return data ?? void 0;
  }
  /**
   * Returns whether the given source file was loaded as part of an external library
   * (e.g. a dependency resolved from `node_modules`). The underlying program metadata is
   * fetched lazily per file and cached on this `Program` instance.
   */
  isSourceFileFromExternalLibrary(file) {
    const metadata = this.getSourceFileMetadataByPath(file.path);
    return metadata?.isFromExternalLibrary ?? false;
  }
  /**
   * Returns whether the given source file is a default library file (e.g. `lib.d.ts`).
   * The underlying program metadata is fetched lazily per file and cached on this
   * `Program` instance.
   */
  isSourceFileDefaultLibrary(file) {
    const metadata = this.getSourceFileMetadataByPath(file.path);
    return metadata?.isDefaultLibrary ?? false;
  }
  /**
   * Get syntactic (parse) diagnostics for a specific file or all files.
   * @param file - Optional file to get diagnostics for. If omitted, returns diagnostics for all files.
   */
  getSyntacticDiagnostics(file) {
    const data = this.client.apiRequest("getSyntacticDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id,
      ...file !== void 0 ? { file } : {}
    });
    return data ?? [];
  }
  /**
   * Get binder diagnostics for a specific file or all files.
   * @param file - Optional file to get diagnostics for. If omitted, returns diagnostics for all files.
   */
  getBindDiagnostics(file) {
    const data = this.client.apiRequest("getBindDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id,
      ...file !== void 0 ? { file } : {}
    });
    return data ?? [];
  }
  /**
   * Get semantic (type-check) diagnostics for a specific file or all files.
   * @param file - Optional file to get diagnostics for. If omitted, returns diagnostics for all files.
   */
  getSemanticDiagnostics(file) {
    const data = this.client.apiRequest("getSemanticDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id,
      ...file !== void 0 ? { file } : {}
    });
    return data ?? [];
  }
  /**
   * Get suggestion diagnostics for a specific file or all files.
   * @param file - Optional file to get diagnostics for. If omitted, returns diagnostics for all files.
   */
  getSuggestionDiagnostics(file) {
    const data = this.client.apiRequest("getSuggestionDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id,
      ...file !== void 0 ? { file } : {}
    });
    return data ?? [];
  }
  /**
   * Get declaration emit diagnostics for a specific file or all files.
   * @param file - Optional file to get diagnostics for. If omitted, returns diagnostics for all files.
   */
  getDeclarationDiagnostics(file) {
    const data = this.client.apiRequest("getDeclarationDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id,
      ...file !== void 0 ? { file } : {}
    });
    return data ?? [];
  }
  /**
   * Get program-wide diagnostics for the project, including compiler options diagnostics.
   */
  getProgramDiagnostics() {
    const data = this.client.apiRequest("getProgramDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id
    });
    return data ?? [];
  }
  /**
   * Get global (non-file-specific) semantic diagnostics for the project.
   */
  getGlobalDiagnostics() {
    const data = this.client.apiRequest("getGlobalDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id
    });
    return data ?? [];
  }
  /**
   * Get config file parsing diagnostics for the project.
   */
  getConfigFileParsingDiagnostics() {
    const data = this.client.apiRequest("getConfigFileParsingDiagnostics", {
      snapshot: this.snapshotId,
      project: this.project.id
    });
    return data ?? [];
  }
};
var Checker = class {
  snapshotId;
  project;
  client;
  objectRegistry;
  wellKnownSymbols;
  constructor(snapshotId, project, client, objectRegistry) {
    this.snapshotId = snapshotId;
    this.project = project;
    this.client = client;
    this.objectRegistry = objectRegistry;
  }
  dispose() {
    this.objectRegistry.clear();
  }
  getSymbolAtLocation(nodeOrNodes) {
    if (Array.isArray(nodeOrNodes)) {
      const data2 = this.client.apiRequest("getSymbolsAtLocations", {
        snapshot: this.snapshotId,
        project: this.project.id,
        locations: nodeOrNodes.map((node) => getNodeId(node))
      });
      return data2.map((d) => d ? this.objectRegistry.getOrCreateSymbol(d) : void 0);
    }
    const data = this.client.apiRequest("getSymbolAtLocation", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(nodeOrNodes)
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  getSymbolAtPosition(file, positionOrPositions) {
    if (typeof positionOrPositions === "number") {
      const data2 = this.client.apiRequest("getSymbolAtPosition", {
        snapshot: this.snapshotId,
        project: this.project.id,
        file,
        position: positionOrPositions
      });
      return data2 ? this.objectRegistry.getOrCreateSymbol(data2) : void 0;
    }
    const data = this.client.apiRequest("getSymbolsAtPositions", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file,
      positions: positionOrPositions
    });
    return data.map((d) => d ? this.objectRegistry.getOrCreateSymbol(d) : void 0);
  }
  getTypeOfSymbol(symbolOrSymbols) {
    if (Array.isArray(symbolOrSymbols)) {
      const data2 = this.client.apiRequest("getTypesOfSymbols", {
        snapshot: this.snapshotId,
        project: this.project.id,
        symbols: symbolOrSymbols.map((s) => s.id)
      });
      return data2.map((d) => d ? this.objectRegistry.getOrCreateType(d) : void 0);
    }
    const data = this.client.apiRequest("getTypeOfSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbolOrSymbols.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  /**
   * Get the declared type of a symbol. Always returns a type; for symbols whose
   * declared type cannot be determined the checker yields the error type (use
   * {@link Type.isErrorType} to detect it).
   */
  getDeclaredTypeOfSymbol(symbol) {
    const data = this.client.apiRequest("getDeclaredTypeOfSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
    if (!data)
      throw new Error(`getDeclaredTypeOfSymbol returned no type for symbol ${symbol.id}`);
    return this.objectRegistry.getOrCreateType(data);
  }
  getReferencesToSymbolInFile(file, symbol) {
    const data = this.client.apiRequest("getReferencesToSymbolInFile", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file,
      symbol: symbol.id
    });
    return (data ?? []).map((h) => new NodeHandle(h, this.project));
  }
  getReferencedSymbolsForNode(node, position) {
    const data = this.client.apiRequest("getReferencedSymbolsForNode", {
      snapshot: this.snapshotId,
      project: this.project.id,
      node: getNodeId(node),
      position
    });
    return (data ?? []).map((entry) => ({
      definition: new NodeHandle(entry.definition, this.project),
      symbol: entry.symbol ? this.objectRegistry.getOrCreateSymbol(entry.symbol) : void 0,
      references: (entry.references ?? []).map((h) => new NodeHandle(h, this.project))
    }));
  }
  getSignatureUsage(signatureDecl) {
    const data = this.client.apiRequest("getSignatureUsages", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signatureDecl: getNodeId(signatureDecl)
    });
    return (data ?? []).map((entry) => ({
      name: new NodeHandle(entry.name, this.project),
      call: entry.call ? new NodeHandle(entry.call, this.project) : void 0
    }));
  }
  getCompletionsAtPosition(document, position, options) {
    const data = this.client.apiRequest("getCompletionsAtPosition", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file: document,
      position,
      triggerCharacter: options?.triggerCharacter,
      includeSymbol: options?.includeSymbol
    });
    if (!data)
      return void 0;
    return {
      isIncomplete: data.isIncomplete,
      entries: data.entries.map((e) => ({
        ...e,
        symbol: e.symbol ? this.objectRegistry.getOrCreateSymbol(e.symbol) : void 0
      }))
    };
  }
  getTypeAtLocation(nodeOrNodes) {
    if (Array.isArray(nodeOrNodes)) {
      const data2 = this.client.apiRequest("getTypeAtLocations", {
        snapshot: this.snapshotId,
        project: this.project.id,
        locations: nodeOrNodes.map((node) => getNodeId(node))
      });
      return data2.map((d) => d ? this.objectRegistry.getOrCreateType(d) : void 0);
    }
    const data = this.client.apiRequest("getTypeAtLocation", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(nodeOrNodes)
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getSignaturesOfType(type, kind) {
    const data = this.client.apiRequest("getSignaturesOfType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id,
      kind
    });
    return data.map((d) => this.objectRegistry.getOrCreateSignature(d));
  }
  getResolvedSignature(node) {
    const data = this.client.apiRequest("getResolvedSignature", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateSignature(data) : void 0;
  }
  getTypeAtPosition(file, positionOrPositions) {
    if (typeof positionOrPositions === "number") {
      const data2 = this.client.apiRequest("getTypeAtPosition", {
        snapshot: this.snapshotId,
        project: this.project.id,
        file,
        position: positionOrPositions
      });
      return data2 ? this.objectRegistry.getOrCreateType(data2) : void 0;
    }
    const data = this.client.apiRequest("getTypesAtPositions", {
      snapshot: this.snapshotId,
      project: this.project.id,
      file,
      positions: positionOrPositions
    });
    return data.map((d) => d ? this.objectRegistry.getOrCreateType(d) : void 0);
  }
  resolveName(name, meaning, location, excludeGlobals) {
    const isNode = location && "kind" in location;
    const data = this.client.apiRequest("resolveName", {
      snapshot: this.snapshotId,
      project: this.project.id,
      name,
      meaning,
      location: isNode ? getNodeId(location) : void 0,
      file: !isNode && location ? location.document : void 0,
      position: !isNode && location ? location.position : void 0,
      excludeGlobals
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  getResolvedSymbol(node) {
    const text = node.text;
    if (!text)
      return void 0;
    return this.resolveName(text, SymbolFlags.Value | SymbolFlags.ExportValue, node);
  }
  getContextualType(node) {
    const data = this.client.apiRequest("getContextualType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getBaseTypeOfLiteralType(type) {
    const data = this.client.apiRequest("getBaseTypeOfLiteralType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getNonNullableType(type) {
    const data = this.client.apiRequest("getNonNullableType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getTypeFromTypeNode(node) {
    const data = this.client.apiRequest("getTypeFromTypeNode", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getWidenedType(type) {
    const data = this.client.apiRequest("getWidenedType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getParameterType(signature, index) {
    const data = this.client.apiRequest("getParameterType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signature: signature.id,
      index
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  isArrayLikeType(type) {
    return this.client.apiRequest("isArrayLikeType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
  }
  isTypeAssignableTo(source, target) {
    return this.client.apiRequest("isTypeAssignableTo", {
      snapshot: this.snapshotId,
      project: this.project.id,
      source: source.id,
      target: target.id
    });
  }
  getShorthandAssignmentValueSymbol(node) {
    const data = this.client.apiRequest("getShorthandAssignmentValueSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  /**
   * Get the type of a symbol as narrowed at a specific location. Always returns
   * a type; for symbols whose type cannot be determined the checker yields the
   * error type (use {@link Type.isErrorType} to detect it).
   */
  getTypeOfSymbolAtLocation(symbol, location) {
    const data = this.client.apiRequest("getTypeOfSymbolAtLocation", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id,
      location: getNodeId(location)
    });
    if (!data)
      throw new Error(`getTypeOfSymbolAtLocation returned no type for symbol ${symbol.id}`);
    return this.objectRegistry.getOrCreateType(data);
  }
  getIntrinsicType(method) {
    const data = this.client.apiRequest(method, {
      snapshot: this.snapshotId,
      project: this.project.id
    });
    return this.objectRegistry.getOrCreateType(data);
  }
  getAnyType() {
    return this.getIntrinsicType("getAnyType");
  }
  getStringType() {
    return this.getIntrinsicType("getStringType");
  }
  getNumberType() {
    return this.getIntrinsicType("getNumberType");
  }
  getBooleanType() {
    return this.getIntrinsicType("getBooleanType");
  }
  getVoidType() {
    return this.getIntrinsicType("getVoidType");
  }
  getUndefinedType() {
    return this.getIntrinsicType("getUndefinedType");
  }
  getNullType() {
    return this.getIntrinsicType("getNullType");
  }
  getNeverType() {
    return this.getIntrinsicType("getNeverType");
  }
  getUnknownType() {
    return this.getIntrinsicType("getUnknownType");
  }
  getBigIntType() {
    return this.getIntrinsicType("getBigIntType");
  }
  getESSymbolType() {
    return this.getIntrinsicType("getESSymbolType");
  }
  typeToTypeNode(type, enclosingDeclaration, flags) {
    const binaryData = this.client.apiRequestBinary("typeToTypeNode", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id,
      location: enclosingDeclaration ? getNodeId(enclosingDeclaration) : void 0,
      flags
    });
    if (!binaryData)
      return void 0;
    return decodeNode(binaryData);
  }
  signatureToSignatureDeclaration(signature, kind, enclosingDeclaration, flags) {
    const binaryData = this.client.apiRequestBinary("signatureToSignatureDeclaration", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signature: signature.id,
      kind,
      location: enclosingDeclaration ? getNodeId(enclosingDeclaration) : void 0,
      flags
    });
    if (!binaryData)
      return void 0;
    return decodeNode(binaryData);
  }
  typeToString(type, enclosingDeclaration, flags) {
    return this.client.apiRequest("typeToString", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id,
      location: enclosingDeclaration ? getNodeId(enclosingDeclaration) : void 0,
      flags
    });
  }
  isContextSensitive(node) {
    return this.client.apiRequest("isContextSensitive", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
  }
  isArrayType(type) {
    return this.client.apiRequest("isArrayType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
  }
  isTupleType(type) {
    return this.client.apiRequest("isTupleType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
  }
  getReturnTypeOfSignature(signature) {
    const data = this.client.apiRequest("getReturnTypeOfSignature", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signature: signature.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getRestTypeOfSignature(signature) {
    const data = this.client.apiRequest("getRestTypeOfSignature", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signature: signature.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getTypePredicateOfSignature(signature) {
    const data = this.client.apiRequest("getTypePredicateOfSignature", {
      snapshot: this.snapshotId,
      project: this.project.id,
      signature: signature.id
    });
    if (!data)
      return void 0;
    return {
      kind: data.kind,
      parameterIndex: data.parameterIndex,
      parameterName: data.parameterName,
      type: data.type ? this.objectRegistry.getOrCreateType(data.type) : void 0
    };
  }
  /**
   * Get the base types of a class or interface type. A type with no base types
   * yields an empty array.
   */
  getBaseTypes(type) {
    const data = this.client.apiRequest("getBaseTypes", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? data.map((d) => this.objectRegistry.getOrCreateType(d)) : [];
  }
  getApparentType(type) {
    const data = this.client.apiRequest("getApparentType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getPropertiesOfType(type) {
    const data = this.client.apiRequest("getPropertiesOfType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? data.map((d) => this.objectRegistry.getOrCreateSymbol(d)) : [];
  }
  getIndexInfosOfType(type) {
    const data = this.client.apiRequest("getIndexInfosOfType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    if (!data)
      return [];
    return data.map((d) => ({
      keyType: this.objectRegistry.getOrCreateType(d.keyType),
      valueType: this.objectRegistry.getOrCreateType(d.valueType),
      isReadonly: d.isReadonly ?? false,
      declaration: d.declaration ? new NodeHandle(d.declaration, this.project) : void 0
    }));
  }
  /**
   * Get the constraint of a type parameter (the `T` in `<U extends T>`), or
   * undefined if it has none.
   */
  getConstraintOfTypeParameter(type) {
    const data = this.client.apiRequest("getConstraintOfTypeParameter", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getBaseConstraintOfType(type) {
    const data = this.client.apiRequest("getBaseConstraintOfType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? this.objectRegistry.getOrCreateType(data) : void 0;
  }
  getPropertyOfType(type, name) {
    const data = this.client.apiRequest("getPropertyOfType", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id,
      name
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  getConstantValue(node) {
    const data = this.client.apiRequest("getConstantValue", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ?? void 0;
  }
  getSignatureFromDeclaration(node) {
    const data = this.client.apiRequest("getSignatureFromDeclaration", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateSignature(data) : void 0;
  }
  getExportSpecifierLocalTargetSymbol(node) {
    const data = this.client.apiRequest("getExportSpecifierLocalTargetSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      location: getNodeId(node)
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  /**
   * Follow all aliases to get the original symbol. Always returns a symbol; for
   * an unresolved alias the checker yields the unknown symbol (use
   * {@link Checker.isUnknownSymbol} to detect it).
   */
  getAliasedSymbol(symbol) {
    const data = this.client.apiRequest("getAliasedSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
    if (!data)
      throw new Error(`getAliasedSymbol returned no symbol for symbol ${symbol.id}`);
    return this.objectRegistry.getOrCreateSymbol(data);
  }
  getImmediateAliasedSymbol(symbol) {
    const data = this.client.apiRequest("getImmediateAliasedSymbol", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  /**
   * Fetch (once, then cache) the handle ids of the per-checker singleton
   * symbols (unknown, undefined, arguments). These ids are stable for the life
   * of the project's checker, so identity checks against them are local after
   * the first call.
   */
  getWellKnownSymbols() {
    return this.wellKnownSymbols ??= this.client.apiRequest("getWellKnownSymbols", {
      snapshot: this.snapshotId,
      project: this.project.id
    });
  }
  /**
   * Returns `true` if the symbol is the checker's "unknown" symbol (e.g. the
   * result of {@link Checker.getAliasedSymbol} on an unresolved alias).
   */
  isUnknownSymbol(symbol) {
    return symbol.id === this.getWellKnownSymbols().unknown;
  }
  /**
   * Returns `true` if the symbol is the checker's "undefined" symbol.
   */
  isUndefinedSymbol(symbol) {
    return symbol.id === this.getWellKnownSymbols().undefined;
  }
  /**
   * Returns `true` if the symbol is the checker's "arguments" symbol.
   */
  isArgumentsSymbol(symbol) {
    return symbol.id === this.getWellKnownSymbols().arguments;
  }
  getExportsOfModule(symbol) {
    const data = this.client.apiRequest("getExportsOfModule", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
    return data ? data.map((d) => this.objectRegistry.getOrCreateSymbol(d)) : [];
  }
  getMemberInModuleExports(symbol, name) {
    const data = this.client.apiRequest("getMemberInModuleExports", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id,
      name
    });
    return data ? this.objectRegistry.getOrCreateSymbol(data) : void 0;
  }
  getJsDocTagsOfSymbol(symbol) {
    const data = this.client.apiRequest("getJsDocTags", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
    return data ?? [];
  }
  getDocumentationCommentOfSymbol(symbol) {
    return this.client.apiRequest("getDocumentationComment", {
      snapshot: this.snapshotId,
      project: this.project.id,
      symbol: symbol.id
    });
  }
  /**
   * Get the type arguments of a type reference (e.g. the `string` in `Array<string>`).
   */
  getTypeArguments(type) {
    const data = this.client.apiRequest("getTypeArguments", {
      snapshot: this.snapshotId,
      project: this.project.id,
      type: type.id
    });
    return data ? data.map((d) => this.objectRegistry.getOrCreateType(d)) : [];
  }
};
var Emitter = class {
  client;
  constructor(client) {
    this.client = client;
  }
  printNode(node, options = {}) {
    const encoded = encodeNode(node);
    const base64 = uint8ArrayToBase64(encoded);
    return this.client.apiRequest("printNode", {
      data: base64,
      ...options
    });
  }
};
var NodeHandle = class {
  /**
   * The project this handle was produced in, used as the default for {@link resolve}.
   * Node handles are only meaningful within a project's program, so the producing project
   * is remembered so callers don't have to pass it explicitly.
   */
  canonicalProject;
  index;
  kind;
  path;
  constructor(handle, canonicalProject) {
    const parsed = parseNodeHandle(handle);
    this.index = parsed.index;
    this.kind = parsed.kind;
    this.path = parsed.path;
    this.canonicalProject = canonicalProject;
  }
  /**
   * Resolve this handle to the actual AST node by fetching the source file from a project
   * and looking up the node by index. If no project is passed, the project that produced
   * the handle is used.
   */
  resolve(project = this.canonicalProject) {
    const sourceFile = project.program.getSourceFile(this.path);
    if (!sourceFile) {
      return void 0;
    }
    return sourceFile.getOrCreateNodeAtIndex(this.index);
  }
};
var Symbol2 = class {
  objectRegistry;
  /**
   * The project this symbol was first observed in, used as the default project for
   * lookups that need a project context (members/exports/parent). Symbols are shared
   * snapshot-wide, so these lookups can otherwise be ambiguous about which project to use.
   */
  canonicalProject;
  id;
  /** The escaped (`__String`) name, used as the key in member/export tables. */
  escapedName;
  /** The display name (escaped underscores removed). */
  name;
  flags;
  checkFlags;
  declarations;
  valueDeclaration;
  parent;
  exportSymbol;
  membersCache;
  exportsCache;
  constructor(data, objectRegistry) {
    this.objectRegistry = objectRegistry;
    this.id = data.id;
    this.escapedName = data.name;
    this.name = unescapeLeadingUnderscores(data.name);
    this.flags = data.flags;
    this.checkFlags = data.checkFlags;
    const canonicalProject = objectRegistry.getProject(data.project);
    if (!canonicalProject) {
      throw new Error(`Symbol ${data.id} references unknown canonical project '${data.project}'`);
    }
    this.canonicalProject = canonicalProject;
    this.declarations = (data.declarations ?? []).map((d) => new NodeHandle(d, canonicalProject));
    this.valueDeclaration = data.valueDeclaration ? new NodeHandle(data.valueDeclaration, canonicalProject) : void 0;
    if (data.parent !== void 0)
      this.parent = data.parent;
    if (data.exportSymbol !== void 0)
      this.exportSymbol = data.exportSymbol;
  }
  getParent() {
    return this.objectRegistry.fetchSymbol(this, "getParentOfSymbol", this.parent, this.canonicalProject.id);
  }
  /**
   * Get this symbol's members keyed by escaped name. The result is cached on
   * the symbol, so repeated calls do not round-trip to the server.
   */
  getMembers() {
    return this.membersCache ??= this.fetchSymbolTable("getMembersOfSymbol");
  }
  /**
   * Get this symbol's exports keyed by escaped name. The result is cached on
   * the symbol, so repeated calls do not round-trip to the server.
   */
  getExports() {
    return this.exportsCache ??= this.fetchSymbolTable("getExportsOfSymbol");
  }
  fetchSymbolTable(method) {
    const symbols = this.objectRegistry.fetchSymbols(this, method, void 0, this.canonicalProject.id);
    const table = /* @__PURE__ */ new Map();
    for (const symbol of symbols) {
      table.set(symbol.escapedName, symbol);
    }
    return table;
  }
  getExportSymbol() {
    if (!this.exportSymbol)
      return this;
    return this.objectRegistry.fetchSymbol(this, "getExportSymbolOfSymbol", this.exportSymbol, this.canonicalProject.id);
  }
  getJsDocTags(checker) {
    return checker.getJsDocTagsOfSymbol(this);
  }
  getDocumentationComment(checker) {
    return checker.getDocumentationCommentOfSymbol(this);
  }
};
var TypeObject = class {
  objectRegistry;
  id;
  flags;
  objectFlags;
  symbol;
  value;
  intrinsicName;
  isThisType;
  freshType;
  regularType;
  target;
  typeParameters;
  outerTypeParameters;
  localTypeParameters;
  aliasTypeArguments;
  aliasSymbol;
  elementFlags;
  fixedLength;
  readonly;
  texts;
  objectType;
  indexType;
  checkType;
  extendsType;
  baseType;
  substConstraint;
  trueType;
  // false if not yet loaded
  falseType;
  // false if not yet loaded
  constructor(data, objectRegistry) {
    this.objectRegistry = objectRegistry;
    this.id = data.id;
    this.flags = data.flags;
    if (data.objectFlags !== void 0)
      this.objectFlags = data.objectFlags;
    if (data.symbol !== void 0)
      this.symbol = data.symbol;
    if (data.value != null) {
      this.value = data.flags & TypeFlags.BigIntLiteral ? BigInt(data.value) : data.value;
    }
    if (data.intrinsicName !== void 0)
      this.intrinsicName = data.intrinsicName;
    if (data.isThisType !== void 0)
      this.isThisType = data.isThisType;
    if (data.freshType !== void 0)
      this.freshType = data.freshType;
    if (data.regularType !== void 0)
      this.regularType = data.regularType;
    if (data.target !== void 0)
      this.target = data.target;
    this.typeParameters = data.typeParameters ?? [];
    this.outerTypeParameters = data.outerTypeParameters ?? [];
    this.localTypeParameters = data.localTypeParameters ?? [];
    this.aliasTypeArguments = data.aliasTypeArguments ?? [];
    if (data.aliasSymbol !== void 0)
      this.aliasSymbol = data.aliasSymbol;
    if (data.elementFlags !== void 0)
      this.elementFlags = data.elementFlags;
    if (data.fixedLength !== void 0)
      this.fixedLength = data.fixedLength;
    if (data.readonly !== void 0)
      this.readonly = data.readonly;
    if (data.texts !== void 0)
      this.texts = data.texts;
    if (data.objectType !== void 0)
      this.objectType = data.objectType;
    if (data.indexType !== void 0)
      this.indexType = data.indexType;
    if (data.checkType !== void 0)
      this.checkType = data.checkType;
    if (data.extendsType !== void 0)
      this.extendsType = data.extendsType;
    if (data.baseType !== void 0)
      this.baseType = data.baseType;
    if (data.substConstraint !== void 0)
      this.substConstraint = data.substConstraint;
    this.trueType = false;
    this.falseType = false;
  }
  getSymbol() {
    return this.objectRegistry.fetchSymbol(this, "getSymbolOfType", this.symbol);
  }
  getAliasSymbol() {
    return this.objectRegistry.fetchSymbol(this, "getAliasSymbolOfType", this.aliasSymbol);
  }
  getTarget() {
    return this.objectRegistry.fetchType(this, "getTargetOfType", this.target);
  }
  getFreshType() {
    return this.objectRegistry.fetchType(this, "getFreshTypeOfType", this.freshType);
  }
  getRegularType() {
    return this.objectRegistry.fetchType(this, "getRegularTypeOfType", this.regularType);
  }
  getTypes() {
    if (!(this.flags & (TypeFlags.UnionOrIntersection | TypeFlags.TemplateLiteral))) {
      return void 0;
    }
    return this.objectRegistry.fetchTypes(this, "getTypesOfType");
  }
  getTypeParameters() {
    return this.objectRegistry.fetchTypes(this, "getTypeParametersOfType", this.typeParameters);
  }
  getOuterTypeParameters() {
    return this.objectRegistry.fetchTypes(this, "getOuterTypeParametersOfType", this.outerTypeParameters);
  }
  getLocalTypeParameters() {
    return this.objectRegistry.fetchTypes(this, "getLocalTypeParametersOfType", this.localTypeParameters);
  }
  getAliasTypeArguments() {
    return this.objectRegistry.fetchTypes(this, "getAliasTypeArgumentsOfType", this.aliasTypeArguments);
  }
  getObjectType() {
    return this.objectRegistry.fetchType(this, "getObjectTypeOfType", this.objectType);
  }
  getIndexType() {
    return this.objectRegistry.fetchType(this, "getIndexTypeOfType", this.indexType);
  }
  getCheckType() {
    return this.objectRegistry.fetchType(this, "getCheckTypeOfType", this.checkType);
  }
  getExtendsType() {
    return this.objectRegistry.fetchType(this, "getExtendsTypeOfType", this.extendsType);
  }
  getBaseType() {
    return this.objectRegistry.fetchType(this, "getBaseTypeOfType", this.baseType);
  }
  getConstraint() {
    return this.objectRegistry.fetchType(this, "getConstraintOfType", this.substConstraint);
  }
  getTrueType() {
    const result = this.objectRegistry.fetchType(this, "getTrueTypeOfConditionalType", this.trueType);
    this.trueType = result.id;
    return result;
  }
  getFalseType() {
    const result = this.objectRegistry.fetchType(this, "getFalseTypeOfConditionalType", this.falseType);
    this.falseType = result.id;
    return result;
  }
  /**
   * Get the base types of this type. Returns `undefined` for any type that is
   * not a class or interface.
   */
  getBaseTypes() {
    if (!this.isClassOrInterface()) {
      return void 0;
    }
    return this.objectRegistry.fetchBaseTypes(this);
  }
  isClassOrInterface() {
    return isClassOrInterfaceType(this);
  }
  isUnionType() {
    return isUnionType(this);
  }
  isIntersectionType() {
    return isIntersectionType(this);
  }
  isObjectType() {
    return isObjectType(this);
  }
  isIntrinsicType() {
    return isIntrinsicType(this);
  }
  isErrorType() {
    return isErrorType(this);
  }
  isLiteralType() {
    return isLiteralType(this);
  }
  isStringLiteralType() {
    return isStringLiteralType(this);
  }
  isNumberLiteralType() {
    return isNumberLiteralType(this);
  }
  isBigIntLiteralType() {
    return isBigIntLiteralType(this);
  }
  isBooleanLiteralType() {
    return isBooleanLiteralType(this);
  }
  isTypeReference() {
    return isTypeReference(this);
  }
  isTupleType() {
    return isTupleType(this);
  }
  isIndexType() {
    return isIndexType(this);
  }
  isIndexedAccessType() {
    return isIndexedAccessType(this);
  }
  isConditionalType() {
    return isConditionalType(this);
  }
  isSubstitutionType() {
    return isSubstitutionType(this);
  }
  isTemplateLiteralType() {
    return isTemplateLiteralType(this);
  }
  isStringMappingType() {
    return isStringMappingType(this);
  }
  isTypeParameter() {
    return isTypeParameter(this);
  }
};
function isUnionType(type) {
  return (type.flags & TypeFlags.Union) !== 0;
}
function isIntersectionType(type) {
  return (type.flags & TypeFlags.Intersection) !== 0;
}
function isObjectType(type) {
  return (type.flags & TypeFlags.Object) !== 0;
}
function isClassOrInterfaceType(type) {
  return isObjectType(type) && (type.objectFlags & ObjectFlags.ClassOrInterface) !== 0;
}
function isIntrinsicType(type) {
  return (type.flags & TypeFlags.Intrinsic) !== 0;
}
function isErrorType(type) {
  return isIntrinsicType(type) && type.intrinsicName === "error";
}
function isLiteralType(type) {
  return (type.flags & TypeFlags.Literal) !== 0;
}
function isStringLiteralType(type) {
  return (type.flags & TypeFlags.StringLiteral) !== 0;
}
function isNumberLiteralType(type) {
  return (type.flags & TypeFlags.NumberLiteral) !== 0;
}
function isBigIntLiteralType(type) {
  return (type.flags & TypeFlags.BigIntLiteral) !== 0;
}
function isBooleanLiteralType(type) {
  return (type.flags & TypeFlags.BooleanLiteral) !== 0;
}
function isTypeReference(type) {
  return isObjectType(type) && (type.objectFlags & ObjectFlags.Reference) !== 0;
}
function isTupleType(type) {
  return isObjectType(type) && (type.objectFlags & ObjectFlags.Tuple) !== 0;
}
function isIndexType(type) {
  return (type.flags & TypeFlags.Index) !== 0;
}
function isIndexedAccessType(type) {
  return (type.flags & TypeFlags.IndexedAccess) !== 0;
}
function isConditionalType(type) {
  return (type.flags & TypeFlags.Conditional) !== 0;
}
function isSubstitutionType(type) {
  return (type.flags & TypeFlags.Substitution) !== 0;
}
function isTemplateLiteralType(type) {
  return (type.flags & TypeFlags.TemplateLiteral) !== 0;
}
function isStringMappingType(type) {
  return (type.flags & TypeFlags.StringMapping) !== 0;
}
function isTypeParameter(type) {
  return (type.flags & TypeFlags.TypeParameter) !== 0;
}
var Signature = class {
  flags;
  objectRegistry;
  id;
  declaration;
  typeParameters;
  parameters;
  thisParameter;
  target;
  constructor(data, project, objectRegistry) {
    this.id = data.id;
    this.flags = data.flags;
    this.objectRegistry = objectRegistry;
    this.declaration = data.declaration ? new NodeHandle(data.declaration, project) : void 0;
    this.typeParameters = data.typeParameters ?? [];
    this.parameters = data.parameters ?? [];
    this.thisParameter = data.thisParameter;
    this.target = data.target;
  }
  getTypeParameters() {
    return this.objectRegistry.fetchTypes(this, "getTypeParametersOfSignature", this.typeParameters);
  }
  getParameters() {
    return this.objectRegistry.fetchSymbols(this, "getParametersOfSignature", this.parameters);
  }
  getThisParameter() {
    return this.objectRegistry.fetchSymbol(this, "getThisParameterOfSignature", this.thisParameter);
  }
  getTarget() {
    return this.objectRegistry.fetchSignature(this, "getTargetOfSignature", this.target);
  }
  get hasRestParameter() {
    return (this.flags & SignatureFlags.HasRestParameter) !== 0;
  }
  get isConstruct() {
    return (this.flags & SignatureFlags.Construct) !== 0;
  }
  get isAbstract() {
    return (this.flags & SignatureFlags.Abstract) !== 0;
  }
};

// node_modules/typescript/dist/enums/moduleResolutionKind.js
var ModuleResolutionKind;
(function(ModuleResolutionKind2) {
  ModuleResolutionKind2[ModuleResolutionKind2["Unknown"] = 0] = "Unknown";
  ModuleResolutionKind2[ModuleResolutionKind2["Classic"] = 1] = "Classic";
  ModuleResolutionKind2[ModuleResolutionKind2["Node10"] = 2] = "Node10";
  ModuleResolutionKind2[ModuleResolutionKind2["Node16"] = 3] = "Node16";
  ModuleResolutionKind2[ModuleResolutionKind2["NodeNext"] = 99] = "NodeNext";
  ModuleResolutionKind2[ModuleResolutionKind2["Bundler"] = 100] = "Bundler";
})(ModuleResolutionKind || (ModuleResolutionKind = {}));

// node_modules/typescript/dist/enums/jsxEmit.js
var JsxEmit;
(function(JsxEmit2) {
  JsxEmit2[JsxEmit2["None"] = 0] = "None";
  JsxEmit2[JsxEmit2["Preserve"] = 1] = "Preserve";
  JsxEmit2[JsxEmit2["ReactNative"] = 2] = "ReactNative";
  JsxEmit2[JsxEmit2["React"] = 3] = "React";
  JsxEmit2[JsxEmit2["ReactJSX"] = 4] = "ReactJSX";
  JsxEmit2[JsxEmit2["ReactJSXDev"] = 5] = "ReactJSXDev";
})(JsxEmit || (JsxEmit = {}));

// node_modules/typescript/dist/enums/newLineKind.js
var NewLineKind;
(function(NewLineKind2) {
  NewLineKind2[NewLineKind2["None"] = 0] = "None";
  NewLineKind2[NewLineKind2["CRLF"] = 1] = "CRLF";
  NewLineKind2[NewLineKind2["LF"] = 2] = "LF";
})(NewLineKind || (NewLineKind = {}));

// node_modules/typescript/dist/enums/moduleDetectionKind.js
var ModuleDetectionKind;
(function(ModuleDetectionKind2) {
  ModuleDetectionKind2[ModuleDetectionKind2["None"] = 0] = "None";
  ModuleDetectionKind2[ModuleDetectionKind2["Auto"] = 1] = "Auto";
  ModuleDetectionKind2[ModuleDetectionKind2["Legacy"] = 2] = "Legacy";
  ModuleDetectionKind2[ModuleDetectionKind2["Force"] = 3] = "Force";
})(ModuleDetectionKind || (ModuleDetectionKind = {}));

// compiler-client-entry.mjs
function freeze(value) {
  if (value && typeof value === "object") {
    for (const v of Object.values(value)) freeze(v);
    Object.freeze(value);
  }
  return value;
}
var compilerComponents = freeze({ "format": "lekalo/typescript-native-components/1", "typescript": "7.0.2", "apiPackageIntegrity": "sha512-8FYau96o3NKOhbjKi/qNvG/W5jhzxkbdm5sj9AbZ/5T5sWqn3hJgLfGx27sRKZWTvyzCP8dLRBTf5tBTSRVUNA==", "apiSources": { "sha256": "7f3219f95bc1436d3d78a04554584fad6de91bc6620f2b95da4e5f0143ef2905", "files": 416 }, "standardLibraries": { "sha256": "04eee9a4783996a80801fc88e554b61118516b2411e8e8cf853eff68cf2b8c15", "files": 108 }, "resources": [{ "platform": "darwin-arm64", "target": "aarch64-apple-darwin", "package": "@typescript/typescript-darwin-arm64", "version": "7.0.2", "archive": "https://registry.npmjs.org/@typescript/typescript-darwin-arm64/-/typescript-darwin-arm64-7.0.2.tgz", "integrity": "sha512-gowzar9MwS/aRWp6f3a4KUqzRjAZjOsmGNCM6LcTgXum+dBfgsBVMN+AgvOCCbguXyick6LJhpBszxMebJ8syA==", "file": "lib/tsc", "sha256": "a82f731365ad69d5c4c15f5e18fba4584bf3b7b839960172a76c3462b5114bf2", "bytes": 23653616, "licenseSha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47", "noticesSha256": "f5c708b59114507b8b27b48181b6883d106bbca0c1634bbee45b5e344237b66b" }, { "platform": "darwin-x64", "target": "x86_64-apple-darwin", "package": "@typescript/typescript-darwin-x64", "version": "7.0.2", "archive": "https://registry.npmjs.org/@typescript/typescript-darwin-x64/-/typescript-darwin-x64-7.0.2.tgz", "integrity": "sha512-SZ9xZInqApNlNGc9s0W1VSsktYSOe9cFqNOIqmN1Gs8SmkjKZYFt017G4VwPxASInODuAdbTW7sXiFUf893RgA==", "file": "lib/tsc", "sha256": "76e4987d060ef8207875a53a73619ce665ff16fb2800c0d57a452e21c829bd7b", "bytes": 24971184, "licenseSha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47", "noticesSha256": "f5c708b59114507b8b27b48181b6883d106bbca0c1634bbee45b5e344237b66b" }, { "platform": "linux-arm64", "target": "aarch64-unknown-linux-gnu", "package": "@typescript/typescript-linux-arm64", "version": "7.0.2", "archive": "https://registry.npmjs.org/@typescript/typescript-linux-arm64/-/typescript-linux-arm64-7.0.2.tgz", "integrity": "sha512-Qh4eU4/y3yDjnfjjyPYihMj5/ODIlmt+Bzu17OI+fiSRDW57QmU5SiN63exPRNJPKUzcc1INa1NXdrJ+MqHjUQ==", "file": "lib/tsc", "sha256": "f86e8070c267b12d6c119bf567cb82d8b245e9d93da1864bc7c603cea10a8b2f", "bytes": 22872226, "licenseSha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47", "noticesSha256": "f5c708b59114507b8b27b48181b6883d106bbca0c1634bbee45b5e344237b66b" }, { "platform": "linux-x64", "target": "x86_64-unknown-linux-gnu", "package": "@typescript/typescript-linux-x64", "version": "7.0.2", "archive": "https://registry.npmjs.org/@typescript/typescript-linux-x64/-/typescript-linux-x64-7.0.2.tgz", "integrity": "sha512-EYdf2cNg7rgCWJnxCdJ+F3V39O8ihb37eHAu1LK8oAFizgTQbPOK7zHHXbPt8rX24COqODXeI3sIf0fCXG7H/A==", "file": "lib/tsc", "sha256": "4f2de678286401759b3fb4475bafe35b8f32b4b3a07d92642bbf37eadc9b34a4", "bytes": 24101026, "licenseSha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47", "noticesSha256": "f5c708b59114507b8b27b48181b6883d106bbca0c1634bbee45b5e344237b66b" }, { "platform": "win32-x64", "target": "x86_64-pc-windows-msvc", "package": "@typescript/typescript-win32-x64", "version": "7.0.2", "archive": "https://registry.npmjs.org/@typescript/typescript-win32-x64/-/typescript-win32-x64-7.0.2.tgz", "integrity": "sha512-0BQ3HkAHHlKLSp1qRvf3SUhGpGsDuhB/jgFw75guyqbxJqEaS0Cw/VFO8i2nHglJUzQCRtMMR/IBAKE3ETMC4g==", "file": "lib/tsc.exe", "sha256": "f9ecfbdc93753d2c972d66a8d0d75f5bd737fd4a5f88b422d9091ea282bcb2c7", "bytes": 24520544, "licenseSha256": "a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47", "noticesSha256": "f5c708b59114507b8b27b48181b6883d106bbca0c1634bbee45b5e344237b66b" }] });
export {
  API,
  Checker,
  CompletionItemKind,
  DiagnosticCategory,
  ElementFlags,
  Emitter,
  InternalAPI,
  JsxEmit,
  ModifierFlags,
  ModuleDetectionKind,
  ModuleKind,
  ModuleResolutionKind,
  NewLineKind,
  NodeBuilderFlags,
  NodeHandle,
  ObjectFlags,
  Program,
  Project,
  Signature,
  SignatureFlags,
  SignatureKind,
  Snapshot,
  Symbol2 as Symbol,
  SymbolFlags,
  TypeFlags,
  TypePredicateKind,
  ast_exports as ast,
  compilerComponents,
  documentURIToFileName,
  fileNameToDocumentURI,
  isBigIntLiteralType,
  isBooleanLiteralType,
  isClassOrInterfaceType,
  isConditionalType,
  isErrorType,
  isIndexType,
  isIndexedAccessType,
  isIntersectionType,
  isIntrinsicType,
  isLiteralType,
  isNumberLiteralType,
  isObjectType,
  isStringLiteralType,
  isStringMappingType,
  isSubstitutionType,
  isTemplateLiteralType,
  isTupleType,
  isTypeParameter,
  isTypeReference,
  isUnionType
};
