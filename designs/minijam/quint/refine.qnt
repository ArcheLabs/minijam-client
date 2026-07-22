// Abstract refine model.
//
// MiniJAM Runtime does not interpret refine output as a MiniJAM delta. Workers
// submit an opaque canonical JAM WorkReport projection that the Executive later
// imports into a virtual JAM block.
module refine {
  import types.* from "./types"
  import messages.* from "./messages"

  pure def packageHash(pkg: WorkPackage): Hash =
    match pkg.workId {
      | MkWorkId(i) => hashOf(i)
      | _WorkIdSentinel => hashOf(0)
    }

  pure def reportHashOf(pkg: WorkPackage, round: Round): Hash =
    match pkg.workId {
      | MkWorkId(i) => hashOf(i * 100 + round)
      | _WorkIdSentinel => hashOf(round)
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

  pure def canonicalReport(pkg: WorkPackage, round: Round, deps: Set[Hash], gas: Gas): CanonicalWorkReport = {
    reportHash: reportHashOf(pkg, round),
    packageHash: packageHash(pkg),
    exportsRoot: defaultMetadata(pkg).exportsRoot,
    authorizerHash: hashOf(300 + round),
    core: match pkg.workId { | MkWorkId(i) => i % 2 | _WorkIdSentinel => 0 },
    accumulateGas: gas,
    dependencies: deps
  }

  pure def candidateReport(
    pkg: WorkPackage,
    round: Round,
    submitter: AccountId,
    deps: Set[Hash],
    gas: Gas
  ): ReportEnvelope = {
    protocolVersion: ProtocolVersion,
    chainId: ChainId,
    workId: pkg.workId,
    service: pkg.service,
    round: round,
    reportHash: reportHashOf(pkg, round),
    metadata: { ...defaultMetadata(pkg), accumulateGas: gas },
    canonical: canonicalReport(pkg, round, deps, gas),
    bulletinFault: NoBulletinFault
  }
}
