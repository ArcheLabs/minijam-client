// Abstract refine model.
//
// MiniJAM v1 does not execute a real PVM here. Refine is represented by
// workers producing a candidate report whose envelope carries abstract
// metadata, delta, service outputs, preimages and exports.
module refine {
  import types.* from "./types"
  import messages.* from "./messages"

  pure def packageHash(pkg: WorkPackage): Hash =
    match pkg.workId {
      | MkWorkId(i) => hashOf(i)
      | _WorkIdSentinel => hashOf(0)
    }

  pure def defaultMetadata(pkg: WorkPackage): ReportMetadata = {
    packageHash: packageHash(pkg),
    contextHash: pkg.contextHash,
    exportsRoot: hashOf(900),
    accumulateGas: 10
  }

  pure def defaultWorkPackage(
    workId: WorkId,
    owner: AccountId,
    serviceId: ServiceId,
    parentRoot: Hash,
    codeHash: Hash
  ): WorkPackage = {
    workId: workId,
    owner: owner,
    service: serviceId,
    payloadHash: packageHash({
      workId: workId,
      owner: owner,
      service: serviceId,
      payloadHash: hashOf(0),
      contextHash: hashOf(0),
      parentStateRoot: parentRoot,
      serviceCodeHash: codeHash,
      gasLimit: MaxGas,
      importsRoot: hashOf(0)
    }),
    contextHash: ChainId,
    parentStateRoot: parentRoot,
    serviceCodeHash: codeHash,
    gasLimit: MaxGas,
    importsRoot: hashOf(11)
  }

  pure def validServiceDelta(local: int, value: int): StateChange = {
    key: { namespace: ServiceStorage, local: local },
    op: Upsert,
    value: Some(value)
  }

  pure def validPreimageDelta(local: int, value: int): StateChange = {
    key: { namespace: Preimage, local: local },
    op: Upsert,
    value: Some(value)
  }

  pure def forbiddenDelta(local: int): StateChange = {
    key: { namespace: ForbiddenNamespace, local: local },
    op: Upsert,
    value: Some(1)
  }

  pure def candidateReport(
    pkg: WorkPackage,
    round: Round,
    submitter: AccountId,
    delta: List[StateChange],
    outcome: ExecutionOutcome
  ): ReportEnvelope = {
    protocolVersion: ProtocolVersion,
    chainId: ChainId,
    workId: pkg.workId,
    service: pkg.service,
    round: round,
    reportHash: match pkg.workId { | MkWorkId(i) => hashOf(i * 100 + round) | _WorkIdSentinel => hashOf(round) },
    metadata: defaultMetadata(pkg),
    parentStateRoot: pkg.parentStateRoot,
    postStateRoot: stateRootAfter(pkg.parentStateRoot, match pkg.workId { | MkWorkId(i) => hashOf(i * 100 + round) | _WorkIdSentinel => hashOf(round) }),
    bulletinFault: NoBulletinFault,
    delta: delta,
    outputs: List({ service: MkServiceId(1), outputHash: hashOf(700 + round) }),
    bridgeEffects: List(),
    gasUsed: 10,
    outcome: outcome
  }
}
