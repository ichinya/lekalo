// Independent writer for the published #22 contract, whose object keys
// have fixed field order rather than the requirements report's byte order.
export function canonicalTrace(manifest) {
  const value = structuredClone(manifest);
  const bytes = (a, b) => Buffer.compare(Buffer.from(a ?? ''), Buffer.from(b ?? ''));
  const nodes = ['requirement','symbol','artifact','scenario','native_test','gate','diagnostic'];
  const relations = ['implements','binds','covers','verifies','evidences','derived_from','references','supersedes'];
  const gaps = ['missing-requirement','missing-symbol','missing-binding','missing-artifact','missing-scenario','missing-native-test','missing-gate','stale-revision','digest-mismatch','conflict','unsupported-provider','privacy-redacted'];
  value.nodes.sort((a,b) => nodes.indexOf(a.nodeKind)-nodes.indexOf(b.nodeKind) || bytes(a.nodeId,b.nodeId));
  for (const node of value.nodes) node.externalRefs?.sort((a,b) => bytes(a.system,b.system) || bytes(a.originalId,b.originalId));
  value.relations.sort((a,b) => relations.indexOf(a.relationKind)-relations.indexOf(b.relationKind) || bytes(a.fromNode,b.fromNode) || bytes(a.toNode,b.toNode) || bytes(a.occurrence,b.occurrence));
  for (const relation of value.relations) relation.evidenceRefs.sort(bytes);
  value.gaps?.sort((a,b) => gaps.indexOf(a.gapKind)-gaps.indexOf(b.gapKind) || bytes(a.anchorNode,b.anchorNode) || bytes(a.expected,b.expected));
  const orders = {
    manifest: ['schemaVersion','identity','manifestId','projectRef','completeness','sourceRevision','modelRef','irRef','graphRef','artifactManifestRef','exportProfile','nodes','relations','gaps'],
    node: ['nodeId','nodeKind','requirementId','semanticId','artifactId','ownership','path','contentDigest','revision','generatorRef','manifestDigest','scenarioId','testId','gateId','diagnosticId','contractVersion','evidenceDigest','externalRefs'],
    relation: ['relationId','relationKind','fromNode','toNode','occurrence','provenance','confidence','status','evidenceRefs'],
    provenance: ['origin','sourceSystem','sourceRevision','sourceDigest','recordedBy','sourcePath'],
    external: ['system','originalId','contractVersion','revision','digest'],
    gap: ['gapKind','status','anchorNode','expected','sourcePath'],
    ref: ['schemaVersion','digest'],
  };
  function write(value) {
    if (Array.isArray(value)) return `[${value.map(write).join(',')}]`;
    if (value !== null && typeof value === 'object') {
      const kind = 'nodes' in value ? 'manifest' : 'nodeKind' in value ? 'node' : 'relationKind' in value ? 'relation' : 'origin' in value ? 'provenance' : 'originalId' in value ? 'external' : 'gapKind' in value ? 'gap' : 'ref';
      const order = orders[kind];
      if (Object.keys(value).some(k => !order.includes(k))) throw Error('unknown trace field');
      return `{${order.filter(k => k in value).map(k => `${JSON.stringify(k)}:${write(value[k])}`).join(',')}}`;
    }
    return JSON.stringify(value);
  }
  return write(value);
}
